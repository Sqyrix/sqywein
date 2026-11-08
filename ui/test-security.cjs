// Tests the actual embedded scripts with a simulated WebView2 bridge.
const fs=require('node:fs'),path=require('node:path'),http=require('node:http'),assert=require('node:assert/strict');
const {chromium}=require(process.env.PLAYWRIGHT_MODULE || 'playwright');
const root=path.join(__dirname,'..');
const engine=fs.readFileSync(path.join(root,'src/engine.rs'),'utf8');
const guard=engine.match(/const REST_GUARD: &str = r#"([\s\S]*?)"#;/)[1];
const collector=fs.readFileSync(path.join(root,'src/adblock.rs'),'utf8').match(/pub const COLLECTOR_SCRIPT: &str = r#"([\s\S]*?)"#;/)[1];
const autofill=fs.readFileSync(path.join(__dirname,'autofill.js'),'utf8');
const adblockSource=fs.readFileSync(path.join(root,'src/adblock.rs'),'utf8');
const appendTemplate=JSON.parse('"'+adblockSource.match(/pub fn append_css_script[\s\S]*?format!\("(.*?)"\)/)[1]+'"');
const appendScript=rules=>appendTemplate.replace('{rules}',JSON.stringify(rules)).replaceAll('{{','{').replaceAll('}}','}');
const pageStyleTemplate=JSON.parse('"'+adblockSource.match(/fn style_script[\s\S]*?format!\("(.*?)"\)/)[1]+'"');
const pageStyle=(host,css)=>pageStyleTemplate.replaceAll('{host}',JSON.stringify(host)).replaceAll('{css}',JSON.stringify(css)).replaceAll('{{','{').replaceAll('}}','}');
const template=fs.readFileSync(path.join(root,'src/app/vault.rs'),'utf8').match(/let script = format!\("(.*?)", serde_json::to_string\(&token\)/)[1];
function delivery(token,source,message){let values=[token,source,message];return template.replace(/\{\}/g,()=>JSON.stringify(values.shift())).replaceAll('{{','{').replaceAll('}}','}');}
let checks=0;
function check(value,label){assert.ok(value,label);checks++;}
const server=http.createServer((req,res)=>{res.setHeader('Content-Type','text/html');res.end('<!doctype html><html><body><form><input id="user" autocomplete="username"><input id="pw" type="password"></form></body></html>');});
(async()=>{
 await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));const base=`http://127.0.0.1:${server.address().port}`;
 const executablePath=['C:/Program Files/Google/Chrome/Application/chrome.exe','C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe'].find(fs.existsSync);
 const browser=await chromium.launch({headless:true,executablePath});
 try{
  const context=await browser.newContext();
  await context.addInitScript(()=>{window.__messages=[];window.__bridgeListeners=[];window.chrome={webview:{postMessage:m=>window.__messages.push(m),addEventListener:(type,callback)=>window.__bridgeListeners.push(callback)}};});
  const page=await context.newPage();
  await page.addInitScript(guard);
  await page.goto(base);check(await page.evaluate(()=>window.__sqCanRest),'idle page can rest');
  await page.locator('#user').fill('unsaved');check(!(await page.evaluate(()=>window.__sqCanRest)),'edited form protected');
  await page.reload();await page.evaluate(()=>window.addEventListener('beforeunload',()=>{}));check(!(await page.evaluate(()=>window.__sqCanRest)),'beforeunload listener protected');
  await page.reload();await page.evaluate(()=>{const f=document.createElement('iframe');document.body.append(f);});check(!(await page.evaluate(()=>window.__sqCanRest)),'uninspected frame protected');
  check(await page.evaluate(()=>window.__sqCheckRest()),'idle frame tree can rest after child verification');
  await page.frameLocator('iframe').locator('body').evaluate(body=>{const input=body.ownerDocument.createElement('input');body.append(input);input.dispatchEvent(new Event('input',{bubbles:true}));});
  check(!(await page.evaluate(()=>window.__sqCheckRest())),'edited child frame prevents parent rest');
  await page.reload();await page.evaluate(()=>{const v=document.createElement('video');Object.defineProperty(v,'paused',{value:false});document.body.append(v);});check(!(await page.evaluate(()=>window.__sqCanRest)),'playing media protected');
  const capture=await context.newPage();
  await capture.addInitScript(()=>{Object.defineProperty(navigator,'mediaDevices',{value:{getUserMedia:()=>Promise.resolve({})},configurable:true});window.fetch=()=>new Promise(()=>{});});
  await capture.addInitScript(guard);await capture.goto(base);
  await capture.evaluate(()=>navigator.mediaDevices.getUserMedia({video:true}));check(!(await capture.evaluate(()=>window.__sqCanRest)),'capture request protected');
  await capture.reload();await capture.evaluate(()=>{fetch('/upload',{method:'POST',body:'x'});});check(!(await capture.evaluate(()=>window.__sqCanRest)),'pending upload protected');
  await capture.close();
  const live=await context.newPage();
  await live.addInitScript(()=>{for(const name of ['WebSocket','EventSource'])window[name]=class {CLOSED=3;readyState=0;close(){this.readyState=3;}};});
  await live.addInitScript(guard);await live.goto(base);
  for(const name of ['WebSocket','EventSource']){
    await live.evaluate(name=>window.connection=new window[name]('unused'),name);
    check(!(await live.evaluate(()=>window.__sqCanRest)),name+' live connection prevents rest');
    await live.evaluate(()=>window.connection.close());
    check(await live.evaluate(()=>window.__sqCanRest),name+' closed connection releases protection');
  }
  const styleScript=appendScript(['.ad{display:none!important}', '.another{display:none!important}']);
  await live.evaluate(script=>{for(let i=0;i<100;i++)eval(script);},styleScript);
  check(await live.evaluate(()=>document.querySelectorAll('style').length===1 && window.__sqGenericStyle.node.sheet.cssRules.length===2),'repeated ad rules retain one style and two distinct rules');
  await live.evaluate(()=>window.__sqGenericStyle.node.remove());await live.evaluate(styleScript);
  check(await live.evaluate(()=>window.__sqGenericStyle.node.isConnected && window.__sqGenericStyle.node.sheet.cssRules.length===2),'removed cosmetic stylesheet is restored');
  await live.evaluate(pageStyle('127.0.0.1','.first{display:none}'));
  await live.evaluate(pageStyle('127.0.0.1','.second{display:none}'));
  check(await live.evaluate(()=>document.querySelectorAll('style').length===2 && window.__sqPageStyle.textContent.includes('.second') && !window.__sqPageStyle.textContent.includes('.first')),'new site cosmetics replace the old stylesheet without extra nodes');
  await live.close();
  const cosmetic=await context.newPage();await cosmetic.addInitScript(collector);await cosmetic.goto(base);
  await cosmetic.evaluate(()=>{const n=document.createElement('div');n.className='ad-box';document.body.append(n);});await cosmetic.waitForTimeout(350);
  check((await cosmetic.evaluate(()=>window.__messages.length))===0,'disabled cosmetic collector does no work');
  await cosmetic.evaluate(()=>window.__sqCollect(true));await cosmetic.waitForTimeout(350);
  check((await cosmetic.evaluate(()=>window.__messages.filter(m=>m.type==='sq.cosmetic').length))===1,'collector starts on demand');
  await cosmetic.evaluate(()=>window.__sqCollect(false));await cosmetic.evaluate(()=>{const n=document.createElement('div');n.className='another-ad';document.body.append(n);});await cosmetic.waitForTimeout(350);
  check((await cosmetic.evaluate(()=>window.__messages.filter(m=>m.type==='sq.cosmetic').length))===1,'collector disconnects when disabled');
  await cosmetic.evaluate(()=>{window.__messages=[];window.__sqCollect(true);window.__sqCollect(true);const f=document.createDocumentFragment();for(let i=0;i<2400;i++){const n=document.createElement('div');n.className='bulk-'+i;f.append(n);}document.body.append(f);});
  await cosmetic.waitForFunction(()=>new Set(window.__messages.flatMap(m=>m.classes||[]).filter(c=>c.startsWith('bulk-'))).size===2400,undefined,{timeout:12000});
  check(await cosmetic.evaluate(()=>window.__messages.every(m=>m.classes.length<=2000 && m.ids.length<=2000)),'large cosmetic batches are bounded without losing names');
  const batches=await cosmetic.evaluate(()=>window.__messages.length);
  await cosmetic.evaluate(()=>window.__sqCollect(true));await cosmetic.waitForTimeout(400);
  check((await cosmetic.evaluate(()=>window.__messages.length))===batches,'enabling collector twice does not rescan');
  await cosmetic.evaluate(()=>{window.__sqCollect(false);window.__messages=[];window.__sqCollect(true);window.__sqCollect(false);});await cosmetic.waitForTimeout(400);
  check((await cosmetic.evaluate(()=>window.__messages.length))===0,'disabling collector cancels pending scan');await cosmetic.close();
  const pw=await context.newPage();await pw.addInitScript(autofill);await pw.goto(base);
  await pw.waitForFunction(()=>window.__messages.some(m=>m.type==='pw.query'));
  await pw.evaluate(()=>{
    window.__formScans=0;
    const original=document.querySelectorAll.bind(document);
    document.querySelectorAll=function(selector){if(selector==='input' || selector==='input[type="password"]')window.__formScans++;return original(selector);};
    const feed=document.createElement('div');feed.id='feed';document.body.append(feed);
    for(let i=0;i<100;i++){const item=document.createElement('div');item.className='feed-item';feed.append(item);item.className='feed-item visible';item.style.opacity='0.9';}
  });
  await pw.waitForTimeout(450);
  check((await pw.evaluate(()=>window.__formScans))===0,'non-form feed mutations do not rescan login forms');
  await pw.evaluate(()=>{const input=document.createElement('input');input.type='password';document.getElementById('feed').append(input);});
  await pw.waitForTimeout(250);
  check((await pw.evaluate(()=>window.__formScans))>0,'new login fields still trigger scanning');

  await pw.evaluate(()=>window.__bridgeListeners.forEach(cb=>cb({data:{type:'pw.accounts',accounts:[{id:1,username:'alice'}]}})));
  await pw.locator('#pw').focus();await pw.locator('#pw').press('Enter');
  const request=await pw.evaluate(()=>window.__messages.find(m=>m.type==='pw.fill'));
  check(typeof request.documentToken==='string','fill request bound to document');
  await pw.evaluate(delivery('wrong-token',pw.url(),{type:'pw.secret',id:1,username:'alice',password:'secret'}));
  check((await pw.locator('#pw').inputValue())==='','wrong document token rejected');
  await pw.evaluate(delivery(request.documentToken,pw.url(),{type:'pw.secret',documentToken:request.documentToken,id:1,username:'alice',password:'secret'}));
  check((await pw.locator('#pw').inputValue())==='secret','current document fills after verification');
  const oldSource=pw.url();await pw.reload();
  await pw.evaluate(()=>{window.__leaks=[];window.addEventListener('sq-password',e=>window.__leaks.push(e.data));});
  await pw.evaluate(delivery(request.documentToken,oldSource,{type:'pw.secret',documentToken:request.documentToken,id:1,username:'alice',password:'secret'}));
  check((await pw.evaluate(()=>window.__leaks.length))===0,'same-URL replacement document receives no secret event');
  await context.close();console.log(`PASS: ${checks} security and rest-guard assertions (simulated media/capture and WebView2 bridge)`);
 }finally{await browser.close();}
})().catch(e=>{console.error(e);process.exitCode=1;}).finally(()=>server.close());
