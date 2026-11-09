// Windows launch handoff, local OAuth callback, portal redirects and file opening.
// Does not change default associations or use real accounts/networks.
const fs=require('node:fs'),path=require('node:path'),http=require('node:http'),net=require('node:net'),assert=require('node:assert/strict');
const {spawn,execFile}=require('node:child_process'),{promisify}=require('node:util'),run=promisify(execFile);
const {chromium}=require(process.env.PLAYWRIGHT_MODULE||'playwright');
const exe=process.env.SQYWEIN_TEST_EXE||path.resolve(__dirname,'../target/release/sqywein.exe');
const data=path.resolve(__dirname,'../ui/.qa',`integration-${Date.now()}`);fs.mkdirSync(data,{recursive:true});
fs.writeFileSync(path.join(data,'state.json'),JSON.stringify({DefaultPromptSeen:true,CurrentProfile:'default',Settings:{adblock:false,autoUpdate:false,searchSuggestions:false,privacyShield:false}}));
const delay=ms=>new Promise(r=>setTimeout(r,ms));let checks=0,callback,portalCookie;
function check(v,label){assert.ok(v,label);checks++;}
const state='sq-test+a/b=42';
const server=http.createServer((req,res)=>{
 const url=new URL(req.url,'http://localhost');res.setHeader('Content-Type','text/html');
 if(url.pathname==='/auth/start'){res.writeHead(302,{Location:'/auth/provider?state='+encodeURIComponent(url.searchParams.get('state'))});res.end();}
 else if(url.pathname==='/auth/provider')res.end('<a id="approve" href="/desktop/callback?code=test%2Bcode%2Fvalue&amp;state='+encodeURIComponent(url.searchParams.get('state'))+'">Approve local test</a>');
 else if(url.pathname==='/desktop/callback'){callback={code:url.searchParams.get('code'),state:url.searchParams.get('state')};res.end('<h1 id="done">Returned to desktop listener</h1>');}
 else if(url.pathname==='/portal/start'){res.writeHead(302,{Location:'/portal/login'});res.end();}
 else if(url.pathname==='/portal/login')res.end('<form action="/portal/accept" method="post"><button id="accept">Accept test network</button></form>');
 else if(url.pathname==='/portal/accept'){req.resume();req.on('end',()=>{res.writeHead(303,{'Set-Cookie':'portal=accepted; HttpOnly; Path=/','Location':'/portal/connected'});res.end();});}
 else if(url.pathname==='/portal/connected'){portalCookie=req.headers.cookie||'';res.end('<h1 id="connected">Network test complete</h1>');}
 else res.end('<h1>Integration test</h1>');
});
async function freePort(){return new Promise(resolve=>{const s=net.createServer();s.listen(0,'127.0.0.1',()=>{const p=s.address().port;s.close(()=>resolve(p))})})}
(async()=>{
 await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));const base=`http://127.0.0.1:${server.address().port}`;
 const port=await freePort(),env={...process.env,SQYWEIN_DATA:data,SQYWEIN_LOG:path.join(data,'browser.log'),WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:`--remote-debugging-port=${port} --remote-debugging-address=127.0.0.1`};
 const child=spawn(exe,['--',base+'/auth/start?state='+encodeURIComponent(state)],{env,windowsHide:true,stdio:'ignore'});let browser;
 try{
  for(let i=0;i<200;i++){try{if((await fetch(`http://127.0.0.1:${port}/json/version`)).ok)break}catch{}await delay(100)}
  browser=await chromium.connectOverCDP(`http://127.0.0.1:${port}`);const context=browser.contexts()[0];let page;
  for(let i=0;i<100;i++){page=context.pages().find(p=>p.url().includes('/auth/provider'));if(page)break;await delay(100)}
  check(page,'cold OS-style launch follows auth redirect');await page.locator('#approve').click();await page.locator('#done').waitFor();
  check(callback.code==='test+code/value'&&callback.state===state,'loopback app receives exact code and state');
  async function forward(target){
   const next=context.waitForEvent('page');const forwarded=spawn(exe,['--',target],{env,windowsHide:true,stdio:'ignore'});
   const exited=new Promise(resolve=>forwarded.once('exit',resolve));const p=await next;
   check(await exited===0,'second process hands off to running browser');return p;
  }
  const portal=await forward(base+'/portal/start');await portal.locator('#accept').click();await portal.locator('#connected').waitFor();
  check(portalCookie.includes('portal=accepted'),'portal POST, session cookie and 303 redirect survive');
  check(context.pages().includes(page),'original page survives external launch');
  const localFile=path.join(data,'Örnek #100.html');fs.writeFileSync(localFile,'<!doctype html><title>Local HTML</title><h1 id="local">Yerel belge</h1>');
  const local=await forward(localFile);await local.locator('#local').waitFor();check(local.url().startsWith('file:///')&&local.url().includes('%23'),'Windows path with spaces, Unicode and # becomes correct file URL');
  const svgFile=path.join(data,'image.svg');fs.writeFileSync(svgFile,'<svg xmlns="http://www.w3.org/2000/svg" width="40" height="40"><rect id="shape" width="40" height="40" fill="purple"/></svg>');
  const svg=await forward(svgFile);await svg.locator('#shape').waitFor();check(svg.url().startsWith('file:///'),'associated SVG renders as a document');
  const stream='BT /F1 18 Tf 40 100 Td (Sqywein PDF integration fixture) Tj ET';
  const objects=['<< /Type /Catalog /Pages 2 0 R >>','<< /Type /Pages /Kids [3 0 R] /Count 1 >>','<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 200] /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>',`<< /Length ${Buffer.byteLength(stream)} >>\nstream\n${stream}\nendstream`,'<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>'];
  let pdfData='%PDF-1.4\n';const offsets=[0];for(let i=0;i<objects.length;i++){offsets.push(Buffer.byteLength(pdfData));pdfData+=`${i+1} 0 obj\n${objects[i]}\nendobj\n`;}
  const xref=Buffer.byteLength(pdfData);pdfData+='xref\n0 6\n0000000000 65535 f \n'+offsets.slice(1).map(o=>String(o).padStart(10,'0')+' 00000 n \n').join('')+`trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n${xref}\n%%EOF\n`;
  const pdfFile=path.join(data,'document.pdf');fs.writeFileSync(pdfFile,pdfData);
  const pdf=await forward(pdfFile);await pdf.waitForFunction(()=>document.contentType==='application/pdf'||document.querySelector('embed[type="application/pdf"]'));
  check(pdf.url().startsWith('file:///')&&pdf.url().endsWith('document.pdf'),'associated local PDF reaches the built-in viewer');
  const settings=await forward('sqywein://settings');await settings.locator('#settings:not([hidden])').waitFor();
  check(typeof await settings.evaluate(()=>new Promise(resolve=>{const off=on('settings.get',d=>{off();resolve(d.defaultBrowser)});send('settings.get')}))==='boolean','Windows default-browser status is queried');
  await delay(5500);const saved=require('../ui/qa-storage.cjs').readJson(path.join(data,'state.json'));
  check(saved.Tabs.length===6&&!saved.Tabs.some(t=>t.Url.includes('q=--')),'launch separator never becomes a search or extra tab');
  fs.writeFileSync(path.join(data,'results.json'),JSON.stringify({checks,description:'Local mock auth and captive portal; real Windows WM_COPYDATA handoff and file URLs. No real Google account, Wi-Fi gateway or changed default associations.'},null,2));
  console.log(`PASS: ${checks} Windows/browser integration assertions. ${data}`);
 }finally{
  if(browser)await browser.close().catch(()=>{});await run('taskkill.exe',['/PID',String(child.pid),'/T','/F'],{windowsHide:true}).catch(()=>{});
  await new Promise(resolve=>server.close(resolve));
 }
})().catch(e=>{console.error(e);process.exitCode=1;server.close()});
