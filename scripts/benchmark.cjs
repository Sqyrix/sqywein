// Visible, isolated, single-tab comparison. Requires Playwright via PLAYWRIGHT_MODULE.
// Optional BRAVE_EXE / OPERA_EXE; never touches existing browser profiles.
const fs=require('node:fs'),path=require('node:path'),http=require('node:http'),net=require('node:net');
const {spawn,execFile}=require('node:child_process');
const {promisify}=require('node:util');
const run=promisify(execFile),{chromium}=require(process.env.PLAYWRIGHT_MODULE||'playwright');
const root=path.resolve(__dirname,'..'),out=path.join(root,'ui/.qa',`benchmark-${Date.now()}`);
fs.mkdirSync(out,{recursive:true});
const browsers=[
 ['Sqywein',process.env.SQYWEIN_TEST_EXE||path.join(root,'target/release/sqywein.exe')],
 ['Edge','C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe'],
 ['Chrome','C:/Program Files/Google/Chrome/Application/chrome.exe'],
 ['Brave',process.env.BRAVE_EXE],['Opera',process.env.OPERA_EXE]
];
if(process.env.BENCH_BEFORE_EXE){
 const beforeDir=path.join(out,'before-binary');fs.mkdirSync(beforeDir,{recursive:true});
 const beforeExe=path.join(beforeDir,'sqywein.exe');fs.copyFileSync(process.env.BENCH_BEFORE_EXE,beforeExe);
 browsers.splice(0,browsers.length,['Before',beforeExe],['After',process.env.SQYWEIN_TEST_EXE||path.join(root,'target/release/sqywein.exe')]);
}
const available=browsers.filter(([,exe])=>exe&&fs.existsSync(exe));
const delay=ms=>new Promise(r=>setTimeout(r,ms));
const rounds=Number(process.env.BENCH_ROUNDS||3),seconds=Number(process.env.BENCH_SECONDS||5);
const fixture=`<!doctype html><meta charset="utf-8"><title>Sqywein resource benchmark</title>
<style>*{box-sizing:border-box}body{margin:0;background:#15151a;color:#ddd;font:16px Arial}header{position:sticky;top:0;background:#24242e;padding:18px}main{width:650px;margin:auto}article{margin:20px;background:#26262e;border-radius:12px;overflow:hidden}p{padding:15px}img{width:650px;height:300px;object-fit:cover}</style>
<header>Deterministic local feed • no network, extensions or accounts</header><main></main>
<script>const svg='<svg xmlns="http://www.w3.org/2000/svg" width="650" height="300"><defs><linearGradient id="g"><stop stop-color="#8a63ce"/><stop offset="1" stop-color="#278587"/></linearGradient></defs><rect width="650" height="300" fill="url(#g)"/><circle cx="300" cy="150" r="80" fill="#ffffff33"/></svg>';
for(let i=0;i<150;i++){const a=document.createElement('article');a.innerHTML='<img alt="fixture" src="data:image/svg+xml,'+encodeURIComponent(svg)+'"><p>Feed item '+i+' <span>Ready</span></p>';document.querySelector('main').append(a)}
window.ticks=0;window.startWork=()=>{window.work=setInterval(()=>{ticks++;scrollTo(0,(ticks*55)%(document.body.scrollHeight-innerHeight));document.querySelectorAll('span').forEach((s,i)=>{s.textContent='Update '+ticks+' / '+i})},100)};window.stopWork=()=>{clearInterval(work);scrollTo(0,0)};
Promise.all([...document.images].map(i=>i.decode())).then(()=>window.ready=true);</script>`;
const server=http.createServer((req,res)=>{res.setHeader('Content-Type','text/html');res.end(fixture)});
async function freePort(){return new Promise(resolve=>{const s=net.createServer();s.listen(0,'127.0.0.1',()=>{const p=s.address().port;s.close(()=>resolve(p))})})}
async function measure(pid){const {stdout}=await run('powershell.exe',['-NoProfile','-File',path.join(__dirname,'measure.ps1'),'-BrowserId',String(pid),'-Seconds',String(seconds)],{windowsHide:true});return JSON.parse(stdout.replace(/^\uFEFF/,''))}
async function sample(name,exe,round,url){
 const native=['Sqywein','Before','After'].includes(name);
 const profile=path.join(out,`${name}-${round}`);fs.mkdirSync(profile,{recursive:true});
 const port=await freePort();const env={...process.env};let args;
 if(native){
  fs.writeFileSync(path.join(profile,'state.json'),JSON.stringify({DefaultPromptSeen:true,CurrentProfile:'default',Settings:{adblock:false,autoUpdate:false,searchSuggestions:false,privacyShield:false}}));
  env.SQYWEIN_DATA=profile;env.SQYWEIN_LOG=path.join(profile,'browser.log');
  env.WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=`--remote-debugging-port=${port} --remote-debugging-address=127.0.0.1`;
  args=[url];
 }else args=[`--user-data-dir=${profile}`,'--no-first-run','--no-default-browser-check','--disable-extensions',`--remote-debugging-port=${port}`,'--window-size=1200,850',url];
 const child=spawn(exe,args,{env,windowsHide:false,stdio:'ignore'});let browser,browserId=child.pid;
 try{
  let connected=false;
  for(let i=0;i<200;i++){try{const r=await fetch(`http://127.0.0.1:${port}/json/version`);if(r.ok){connected=true;break}}catch{}await delay(100)}
  if(!connected)throw Error(name+' did not expose its isolated debugging endpoint');
  browser=await chromium.connectOverCDP(`http://127.0.0.1:${port}`);
  if(!native){
   const session=await browser.newBrowserCDPSession();
   const info=await session.send('SystemInfo.getProcessInfo');
   browserId=Number(info.processInfo.find(p=>p.type==='browser').id);
   if(!Number.isSafeInteger(browserId)||browserId<=0)throw Error('Invalid browser PID');
  }
  const context=browser.contexts()[0];let page;
  for(let i=0;i<100;i++){page=context.pages().find(p=>p.url()===url);if(page)break;await delay(100)}
  if(!page)throw Error('Fixture tab missing');
  await page.waitForFunction(()=>window.ready===true);
  await run('powershell.exe',['-NoProfile','-Command',`$ErrorActionPreference='Stop'; Add-Type -Name BenchWindow -Namespace Native -MemberDefinition '[System.Runtime.InteropServices.DllImport("user32.dll")] public static extern bool SetWindowPos(System.IntPtr h, System.IntPtr after, int x, int y, int cx, int cy, uint flags); [System.Runtime.InteropServices.DllImport("user32.dll")] public static extern bool IsWindowVisible(System.IntPtr h);'; $w=(Get-Process -Id ${browserId}).MainWindowHandle; if($w -eq 0 -or ![Native.BenchWindow]::IsWindowVisible($w)){throw 'Browser window is not visible'}; if(![Native.BenchWindow]::SetWindowPos($w,0,40,40,1200,850,0x0040)){throw 'Window resize failed'}`],{windowsHide:true});
  const cdp=await context.newCDPSession(page);
  await cdp.send('Emulation.setDeviceMetricsOverride',{width:1100,height:700,deviceScaleFactor:1,mobile:false});
  if(await page.locator('article').count()!==150)throw Error('Incomplete fixture');
  await page.bringToFront();await delay(5000);
  const idle=await measure(browserId);
  await page.evaluate(()=>window.startWork());const active=await measure(browserId);
  const ticks=await page.evaluate(()=>{window.stopWork();return window.ticks});
  if(ticks<seconds*5)throw Error(name+' did not complete the workload');
  const result={name,round,version:browser.version(),viewport:{width:1100,height:700},ticks,idle,active};
  console.log(JSON.stringify({name,round,idleMB:idle.TotalMB,activeMB:active.TotalMB,idleCpu:idle.CpuPercent,activeCpu:active.CpuPercent,ticks}));return result;
 }finally{
  if(browser)await browser.close().catch(()=>{});
  await run('taskkill.exe',['/PID',String(browserId),'/T','/F'],{windowsHide:true}).catch(()=>{});
  await delay(1500);
 }
}
(async()=>{
 await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
 const results=[],url=`http://127.0.0.1:${server.address().port}/fixture`;
 try{
  for(let round=1;round<=rounds;round++)for(const [name,exe] of (round%2?available:[...available].reverse())){
   results.push(await sample(name,exe,round,url));
   fs.writeFileSync(path.join(out,'results.json'),JSON.stringify({description:'Visible local feed, clean profiles, no extensions, adblock off; total descendant private working set; CPU normalized to all logical processors. Not a live Instagram benchmark.',missing:browsers.filter(b=>!available.includes(b)).map(b=>b[0]),results},null,2));
  }
  const median=values=>{const v=values.sort((a,b)=>a-b);return v[Math.floor(v.length/2)]};
  const summary=available.map(([name])=>{const rows=results.filter(r=>r.name===name);return {name,rounds:rows.length,idleMB:median(rows.map(r=>r.idle.TotalMB)),activeMB:median(rows.map(r=>r.active.TotalMB)),idleCpu:median(rows.map(r=>r.idle.CpuPercent)),activeCpu:median(rows.map(r=>r.active.CpuPercent))}});
  fs.writeFileSync(path.join(out,'summary.json'),JSON.stringify(summary,null,2));console.log('MEDIANS',JSON.stringify(summary));console.log('Report:',out);
 }finally{await new Promise(resolve=>server.close(resolve))}
})().catch(e=>{console.error(e);process.exitCode=1;server.close()});
