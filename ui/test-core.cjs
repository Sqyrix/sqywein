// Real Windows/WebView2 security, accessibility, site controls and recovery.
const fs=require('node:fs'),path=require('node:path'),http=require('node:http'),net=require('node:net'),assert=require('node:assert/strict');
const {spawn,execFile}=require('node:child_process'),{promisify}=require('node:util'),run=promisify(execFile);
const {chromium}=require(process.env.PLAYWRIGHT_MODULE||'playwright'),{readJson}=require('./qa-storage.cjs');
const exe=process.env.SQYWEIN_TEST_EXE||path.resolve(__dirname,'../target/release/sqywein.exe');
const data=path.join(__dirname,'.qa','core-'+Date.now());fs.mkdirSync(data,{recursive:true});
fs.writeFileSync(path.join(data,'state.json'),JSON.stringify({DefaultPromptSeen:true,CurrentProfile:'default',Settings:{adblock:false,autoUpdate:false,searchSuggestions:false,translationAuto:false,showWidgets:false,animations:false}}));
let count=0;const check=(v,label)=>{assert.ok(v,label);count++;},sleep=ms=>new Promise(r=>setTimeout(r,ms));
const qa=async(child,action,name='',key=0)=>run('powershell.exe',['-NoProfile','-File',path.resolve(__dirname,'../scripts/qa-native.ps1'),'-BrowserProcessId',String(child.pid),'-Action',action,'-Name',name,'-Key',String(key)],{windowsHide:true,timeout:20000});
const server=http.createServer((req,res)=>{
 if(req.url==='/portal'){res.writeHead(302,{Location:'/wifi-login'});res.end();return;}
 if(req.url.startsWith('/page'))res.setHeader('Set-Cookie','sid=SQY_SESSION_MARKER; HttpOnly; SameSite=Lax; Path=/');
 res.setHeader('Content-Type','text/html');res.end(`<title>${req.url.startsWith('/page')?'Protected session fixture':'Core fixture'}</title><main style="min-height:7000px"><h1>Core fixture</h1><button id="notify" onclick="Notification.requestPermission().then(p=>document.body.dataset.permission=p)">Enable notifications</button><input id="draft"><div id="cookie">${req.headers.cookie||''}</div></main>`);
});
let child,browser,context,settings,base;
async function launch(){
 const port=await new Promise(resolve=>{const s=net.createServer();s.listen(0,'127.0.0.1',()=>{const p=s.address().port;s.close(()=>resolve(p));});});
 child=spawn(exe,['sqywein://settings','--wait-pid=0'],{windowsHide:true,env:{...process.env,SQYWEIN_DATA:data,WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:`--remote-debugging-port=${port} --remote-debugging-address=127.0.0.1`},stdio:'ignore'});
 for(let i=0;i<200;i++){try{browser=await chromium.connectOverCDP('http://127.0.0.1:'+port);break;}catch{}await sleep(100);}
 assert.ok(browser,'fixture runtime starts');context=browser.contexts()[0];
 for(let i=0;i<100;i++){settings=context.pages().find(p=>p.url().includes('settings.html'));if(settings)break;await sleep(100);}await settings.locator('#settings:not([hidden])').waitFor();await qa(child,'show');
}
async function open(url){const ready=context.waitForEvent('page');await settings.evaluate(url=>send('open',{url,newTab:true}),url);const p=await ready;await p.waitForURL(u=>u.origin===new URL(url).origin);return p;}
async function panel(p){const ready=context.waitForEvent('page');await qa(child,'invoke','Site bilgileri ve izinleri');const view=await ready;await view.waitForURL('**/overlay.html#site');await view.getByRole('heading',{name:new URL(p.url()).origin,exact:true}).waitFor();return view;}
async function stop(){await qa(child,'close');for(let i=0;i<100&&child.exitCode===null;i++)await sleep(100);check(child.exitCode!==null,'graceful close completes');await browser.close().catch(()=>{});browser=null;}
(async()=>{await new Promise(r=>server.listen(0,'127.0.0.1',r));base='http://127.0.0.1:'+server.address().port;
 try{
  await launch();const names=JSON.parse((await qa(child,'names')).stdout);check(names.includes('Yeni sekme')&&names.includes('Adres çubuğu'),'custom chrome appears in real UI Automation tree: '+JSON.stringify(names));
  check(names.some(n=>n.includes('Ayarlar')),'tab titles are accessible');
  check(JSON.parse((await qa(child,'selected','Ayarlar')).stdout)===true,'UI Automation exposes selected tab state');
  check(JSON.parse((await qa(child,'selection','Ayarlar')).stdout).includes('Ayarlar'),'UI Automation selection container reports the active tab');
  const page=await open(base+'/page?code=SQY_URL_TOKEN');await page.locator('#notify').waitFor();
  const pageNames=JSON.parse((await qa(child,'names-deep')).stdout);check(pageNames.includes('Enable notifications'),'web page accessibility remains reachable beside the custom chrome: '+JSON.stringify(pageNames));
  await qa(child,'select','Ayarlar');await qa(child,'select','Protected session fixture');await sleep(200);check(JSON.parse((await qa(child,'selected','Protected session fixture')).stdout)===true,'UI Automation Select switches the actual active tab');
  await page.evaluate(()=>localStorage.setItem('fixture','SQY_LOCAL_MARKER'));
  await page.evaluate(()=>{chrome.webview.postMessage({type:'settings.set',key:'searchEngine',value:'attacker'});});await sleep(100);
  const engine=await settings.evaluate(()=>new Promise(resolve=>{const off=on('settings.get',m=>{off();resolve(m.settings.searchEngine)});send('settings.get')}));check(engine==='google','external page cannot invoke privileged settings bridge');
  let view=await panel(page);check(await view.getByText('Bu bağlantı HTTP kullanıyor; şifreli değil.',{exact:true}).count()===1,'site panel reports unencrypted HTTP without claiming secure');
  await view.screenshot({path:path.join(data,'site-info.png')});
  await view.setViewportSize({width:360,height:600});check(await view.locator('#results').evaluate(n=>n.scrollWidth<=n.clientWidth),'site panel fits a narrow viewport');await view.screenshot({path:path.join(data,'site-info-narrow.png')});await view.setViewportSize({width:680,height:640});
  await view.getByRole('combobox',{name:'Bildirimler',exact:true}).selectOption('2');await view.waitForFunction(()=>document.querySelector('select[aria-label="Bildirimler"]').value==='2'&&!document.querySelector('select[aria-label="Bildirimler"]').disabled);
  await view.getByRole('button',{name:'Paneli kapat'}).click();await page.locator('#notify').click();await page.waitForFunction(()=>document.body.dataset.permission==='denied');check(true,'site permission change reaches the real Notification API');
  view=await panel(page);await view.getByRole('button',{name:'Site verilerini temizle…',exact:true}).click();await view.getByRole('button',{name:'Bu sitenin verilerini temizle',exact:true}).click();await view.waitForFunction(()=>document.getElementById('site-status').textContent.startsWith('Site verileri temizlendi'));
  await view.getByRole('button',{name:'Paneli kapat'}).click();check(await page.evaluate(()=>localStorage.getItem('fixture'))===null,'origin-scoped clearing removes local storage');
  await page.goto(base+'/echo');check(!await page.locator('#cookie').textContent(),'origin-scoped clearing removes live session cookies');
  const portal=await open(base+'/portal');await portal.waitForURL(base+'/wifi-login');check(true,'HTTP captive-portal redirect reaches the login page');
  // Restore a tab with a fresh session cookie and a scroll position, without form data.
  await portal.close();await page.goto(base+'/page?code=SQY_URL_TOKEN');await page.evaluate(()=>scrollTo(0,1700));await sleep(7500);
  const saved=readJson(path.join(data,'state.json'));check(saved.Tabs.some(t=>t.Url.includes('SQY_URL_TOKEN')&&t.Scroll[1]>=1600),'scroll position checkpoints durably');
  const raw=fs.readFileSync(path.join(data,'state.json'));check(raw.subarray(0,8).equals(Buffer.from('SQYSEAL\x01'))&&!raw.includes(Buffer.from('SQY_URL_TOKEN')),'session URLs and tokens are encrypted at rest');
  const history=fs.readFileSync(path.join(data,'history.json'));check(!history.includes(Buffer.from('SQY_URL_TOKEN')),'history URLs are encrypted at rest');
  await stop();const cookies=fs.readFileSync(path.join(data,'session-default.bin'));check(!cookies.includes(Buffer.from('SQY_SESSION_MARKER')),'session cookie checkpoint is encrypted');
  // Simulate a torn/corrupt main record with a known valid encrypted backup.
  const stateFile=path.join(data,'state.json');fs.copyFileSync(stateFile,stateFile+'.bak');const broken=fs.readFileSync(stateFile);broken[broken.length-1]^=1;fs.writeFileSync(stateFile,broken);
  await launch();let restored;for(let i=0;i<100;i++){restored=context.pages().find(p=>p.url().includes('SQY_URL_TOKEN'));if(restored)break;await sleep(100);}
  // Restored background tabs are intentionally lazy; activate through UI Automation.
  if(!restored){const tabName=JSON.parse((await qa(child,'names')).stdout).find(n=>n==='Protected session fixture');assert.ok(tabName,'restored accessible tab exists');await qa(child,'invoke',tabName);for(let i=0;i<100;i++){restored=context.pages().find(p=>p.url().includes('SQY_URL_TOKEN'));if(restored)break;await sleep(100);}}
  check(restored,'corrupted main state recovers tabs from encrypted backup');await restored.waitForFunction(()=>scrollY>=1600);check(true,'restored page returns to checkpointed scroll position');
  check((await restored.locator('#cookie').textContent()).includes('SQY_SESSION_MARKER'),'HttpOnly session cookie survives a browser restart');
  await stop();console.log(`PASS: ${count} browser core assertions. Isolated profile: ${data}`);
 }finally{if(browser)await browser.close().catch(()=>{});if(child&&child.exitCode===null)child.kill();await new Promise(r=>server.close(r));}
})().catch(e=>{console.error(e);process.exitCode=1;});
