// Integration smoke test against the actual Rust + WebView2 application.
// Uses an isolated profile and a loopback-only debugging port for this process.
const fs=require('node:fs'),path=require('node:path'),net=require('node:net'),http=require('node:http'),assert=require('node:assert/strict'),{spawn}=require('node:child_process');
const {chromium}=require(process.env.PLAYWRIGHT_MODULE || 'playwright');
const root=path.join(__dirname,'..');
const exe=process.env.SQYWEIN_TEST_EXE || path.join(root,'target/release/sqywein.exe');
const data=path.join(__dirname,'.qa',`native-${Date.now()}`);
fs.mkdirSync(data,{recursive:true});
fs.writeFileSync(path.join(data,'state.json'),JSON.stringify({DefaultPromptSeen:true,CurrentProfile:'default',Settings:{adblock:false,autoUpdate:false,searchSuggestions:false,privacyShield:true}}));
const delay=ms=>new Promise(r=>setTimeout(r,ms));let checks=0;
function check(value,label){assert.ok(value,label);checks++;}
(async()=>{
 const authServer=http.createServer((req,res)=>{
  res.setHeader('Content-Type','text/html');
  if(req.url==='/finish')res.end('<button id="complete" onclick="window.opener.postMessage(\'oauth-complete\',location.origin);window.close()">Complete</button>');
  else res.end('<title>Sqywein QA</title><button id="login" onclick="window.authWindow=window.open(\'/finish\',\'oauth-login\',\'popup,width=450,height=500\')">Login</button><button id="blank-login" onclick="const child=window.authWindow=window.open(\'about:blank\',\'blank-login\');child.location.href=\'/finish\'">Blank login</button><script>addEventListener(\'message\',e=>{if(e.origin===location.origin)document.body.dataset.result=e.data})<\/script>');
 });
 await new Promise(resolve=>authServer.listen(0,'127.0.0.1',resolve));
 const authBase=`http://127.0.0.1:${authServer.address().port}`;
 const port=await new Promise(resolve=>{const s=net.createServer();s.listen(0,'127.0.0.1',()=>{const port=s.address().port;s.close(()=>resolve(port));});});
 const child=spawn(exe,['sqywein://settings','--wait-pid=0'],{windowsHide:!process.env.SQYWEIN_VISUAL_CHECK,env:{...process.env,SQYWEIN_DATA:data,SQYWEIN_LOG:path.join(data,'browser.log'),WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:`--remote-debugging-port=${port} --remote-debugging-address=127.0.0.1`},stdio:'ignore'});
 let browser;
 try{
  let connected=false;
  for(let i=0;i<200;i++){try{const r=await fetch(`http://127.0.0.1:${port}/json/version`);if(r.ok){connected=true;break;}}catch{}await delay(100);}
  check(connected,'native debugging endpoint starts');
  browser=await chromium.connectOverCDP(`http://127.0.0.1:${port}`);
  const context=browser.contexts()[0];let page;
  for(let i=0;i<100;i++){page=context.pages().find(p=>p.url().startsWith('https://sqywein.ui/settings.html'));if(page)break;await delay(100);}
  check(page,'native settings page loads');await page.locator('#settings:not([hidden])').waitFor();
  check(!context.pages().some(p=>p.url().includes('google.com/search')),'restart flag does not become search tab');
  const rpc=(type,payload={})=>page.evaluate(({type,payload})=>new Promise((resolve,reject)=>{const timeout=setTimeout(()=>reject(Error(type+' timed out')),5000);const off=on(type,message=>{clearTimeout(timeout);off();resolve(message);});send(type,payload);}),{type,payload});
  let settings=await rpc('settings.get');check(settings.settings.adblock===false,'native preferences loaded');
  await page.locator('#searchEngine').selectOption('duckduckgo');settings=await rpc('settings.get');check(settings.settings.searchEngine==='duckduckgo','native search preference persists');
  await page.locator('#neverSleep').fill('meet.google.com, localhost');await page.locator('#neverSleep').press('Tab');settings=await rpc('settings.get');check(settings.settings.neverSleep.length===2,'native sleep exceptions stored as list');
  await page.locator('#permission-origin').fill('https://example.com/path');await page.locator('#permission-kind').selectOption('2');await page.locator('#permission-state').selectOption('2');await page.locator('#permission-form button').click();
  await page.waitForFunction(()=>document.getElementById('permission-list').textContent.includes('example.com'));
  let permissions=await rpc('permissions.list');check(permissions.items.some(i=>i.kind===2 && i.state===2),'WebView2 camera permission is actually denied');
  await page.locator('#permission-list button').click();await delay(100);permissions=await rpc('permissions.list');check(!permissions.items.some(i=>i.kind===2),'WebView2 permission reset removes exception');
  const imported=await rpc('bookmarks.import',{items:[{title:'Example',url:'https://example.com'},{title:'Unsafe',url:'javascript:alert(1)'}]});check(imported.added===1,'native import validates URLs');
  const again=await rpc('bookmarks.import',{items:[{title:'Example',url:'https://example.com'}]});check(again.added===0,'native import deduplicates bookmarks');
  const cleared=await rpc('data.clear',{kinds:['cookies']});check(cleared.ok,'native browsing data clear completes');
  await delay(5500);
  const saved=JSON.parse(fs.readFileSync(path.join(data,'state.json'),'utf8'));check(saved.Settings.neverSleep.length===2,'native settings reach disk');check(saved.Favorites.length===1,'native bookmarks reach disk');
  check(saved.Tabs.length===1 && saved.Tabs[0].Url.includes('settings.html'),'native restart flag does not create extra saved tab');
  await page.screenshot({path:path.join(data,'settings-native.png'),fullPage:true});
  await page.locator('[data-setting=animations]').click();
  settings=await rpc('settings.get');check(settings.settings.animations===false && settings.motion===false,'native reduced motion preference');
  await page.locator('[data-setting=animations]').click();
  for(const theme of ['light','dark','sqywein','system']){
   await page.emulateMedia({colorScheme:null});
   await page.locator('#theme').selectOption(theme);
   settings=await rpc('settings.get');check(settings.settings.theme===theme,'native theme preference: '+theme);
   const appearance=await page.evaluate(()=>new Promise(resolve=>{const off=on('theme',d=>{off();resolve(d)});send('theme.get')}));
   await page.waitForFunction(d=>document.documentElement.style.getPropertyValue('--base')===d.colors.base,appearance);
   await page.waitForFunction(d=>matchMedia('(prefers-color-scheme: dark)').matches===d.dark,appearance);
   check(await page.evaluate(()=>matchMedia('(prefers-color-scheme: dark)').matches)===appearance.dark,'WebView2 profile color scheme: '+theme);
   await page.screenshot({path:path.join(data,'theme-'+theme+'.png'),fullPage:true});
  }
  const openerReady=context.waitForEvent('page');
  await page.evaluate(url=>send('open',{url,newTab:true}),authBase+'/opener');
  const opener=await openerReady;await opener.waitForURL(authBase+'/opener');
  for(const button of ['#login','#blank-login']){
   await opener.evaluate(()=>{delete document.body.dataset.result;});
   await opener.locator(button).click();
   await opener.waitForFunction(()=>{try{return window.authWindow?.location.pathname==='/finish' && !!window.authWindow.document.getElementById('complete');}catch{return false;}},undefined,{polling:100,timeout:10000});
   check(await opener.evaluate(()=>window.authWindow.opener===window),'popup keeps its opener: '+button);
   await opener.evaluate(()=>window.authWindow.document.getElementById('complete').click());
   await opener.waitForFunction(()=>document.body.dataset.result==='oauth-complete',undefined,{polling:100,timeout:10000});
   check(await opener.evaluate(()=>document.body.dataset.result==='oauth-complete'),'popup message returns to opener: '+button);
   await opener.waitForFunction(()=>window.authWindow.closed,undefined,{polling:100,timeout:10000});
   check(await opener.evaluate(()=>window.authWindow.closed),'window.close closes hosted popup tab: '+button);
  }

  console.log(`PASS: ${checks} native Rust/WebView2 integration assertions. Isolated data: ${data}`);
  if(process.env.SQYWEIN_VISUAL_CHECK)await delay(60000);
 }finally{
  // Disconnect and terminate only the application created by this test.
  if(browser)await browser.close();child.kill();await new Promise(resolve=>authServer.close(resolve));
 }
})().catch(e=>{console.error(e);process.exitCode=1;});

