// Actual native application: normal tabs, rest protection and repeated view disposal.
const fs=require('node:fs'),path=require('node:path'),http=require('node:http'),net=require('node:net'),assert=require('node:assert/strict');
const {spawn,execFile}=require('node:child_process'),{promisify}=require('node:util');
const run=promisify(execFile),{chromium}=require(process.env.PLAYWRIGHT_MODULE||'playwright');
const root=path.resolve(__dirname,'..'),exe=process.env.SQYWEIN_TEST_EXE||path.join(root,'target/release/sqywein.exe');
const data=path.join(root,'ui/.qa',`resources-${Date.now()}`);fs.mkdirSync(data,{recursive:true});
fs.writeFileSync(path.join(data,'state.json'),JSON.stringify({DefaultPromptSeen:true,CurrentProfile:'default',Settings:{adblock:false,autoUpdate:false,searchSuggestions:false,privacyShield:true,startup:'restore',neverSleep:['sqywein.ui']}}));
let checks=0;const check=(value,label)=>{assert.ok(value,label);checks++};const delay=ms=>new Promise(r=>setTimeout(r,ms));
const server=http.createServer((req,res)=>{
 res.setHeader('Content-Type','text/html');
 res.end(`<!doctype html><title>Resource fixture</title><input id="edit"><main></main><script>
 for(let i=0;i<100;i++){const p=document.createElement('p');p.textContent='Text '+i;document.querySelector('main').append(p)}
 window.ready=true;
 </script>`);
});
async function port(){return new Promise(resolve=>{const s=net.createServer();s.listen(0,'127.0.0.1',()=>{const p=s.address().port;s.close(()=>resolve(p))})})}
(async()=>{
 await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));const base=`http://127.0.0.1:${server.address().port}`;
 const debug=await port(),log=path.join(data,'browser.log');
 const child=spawn(exe,['sqywein://settings'],{windowsHide:!!process.env.RESOURCE_HIDDEN,stdio:'ignore',env:{...process.env,SQYWEIN_DATA:data,SQYWEIN_LOG:log,WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:`--remote-debugging-port=${debug} --remote-debugging-address=127.0.0.1`}});
 child.on('exit',(code,signal)=>{if(!cleaning)console.log(JSON.stringify({unexpectedBrowserExitCode:code,signal}))});
 let browser,cleaning=false;
 try{
  for(let i=0;i<200;i++){try{if((await fetch(`http://127.0.0.1:${debug}/json/version`)).ok)break}catch{}await delay(100)}
  browser=await chromium.connectOverCDP(`http://127.0.0.1:${debug}`);const context=browser.contexts()[0];
  let settings;
  for(let i=0;i<100;i++){settings=context.pages().find(p=>p.url().includes('sqywein.ui/settings'));if(settings)break;await delay(100)}
  check(settings,'settings control page ready');await settings.waitForFunction(()=>typeof send==='function');
  async function open(suffix){const [p]=await Promise.all([context.waitForEvent('page'),settings.evaluate(url=>send('open',{url,newTab:true}),base+'/'+suffix)]);await p.waitForFunction(()=>window.ready);return p}
  async function close(p){const done=p.waitForEvent('close',{timeout:10000});await p.evaluate(()=>window.close());await done}
  async function sample(){const {stdout}=await run('powershell.exe',['-NoProfile','-File',path.join(__dirname,'measure.ps1'),'-BrowserId',String(child.pid),'-Seconds','1'],{windowsHide:true});return JSON.parse(stdout)}
  const probe=await open('probe');await close(probe);check(!context.pages().includes(probe),'normal view closes through native WindowCloseRequested');
  const samples=[];
  for(let round=0;round<Number(process.env.RESOURCE_ROUNDS||3);round++){
   for(let n=0;n<Number(process.env.RESOURCE_CYCLES||30);n++){const p=await open(`cycle-${round}-${n}`);await close(p)}
   await delay(3000);check(context.pages().length===1,'no orphan document targets after round '+round);
   const measured=await sample();samples.push(measured);
   console.log(JSON.stringify({round,targets:context.pages().length,shellMB:measured.ShellMB,totalMB:measured.TotalMB}));
  }
  const multi=[];
  for(let count=1;count<=30;count++){
   multi.push(await open('multi-'+count));
   if(count===10||count===30){check(context.pages().length===count+1,'all '+count+' normal tabs remain present');const measured=await sample();console.log(JSON.stringify({tabs:count,totalMB:measured.TotalMB}));}
  }
  for(const p of multi)await close(p);
  const normal=await open('idle'),edited=await open('edited');await edited.locator('#edit').fill('unsaved');
  const peer=await open('peer');await peer.evaluate(()=>window.peer=new RTCPeerConnection());
  const audio=await open('video');await audio.evaluate(async()=>{
   const c=document.createElement('canvas');c.width=c.height=32;const ctx=c.getContext('2d');
   const v=document.createElement('video');v.muted=true;v.autoplay=true;v.srcObject=c.captureStream(10);document.body.append(v);window.keepCanvas=c;
   let n=0;window.paintTimer=setInterval(()=>{ctx.fillStyle=n++%2?'red':'blue';ctx.fillRect(0,0,32,32)},100);
   await Promise.race([v.play(),new Promise((_,reject)=>setTimeout(()=>reject(Error('Video did not start')),8000))]);
  });
  const normalId=Number(fs.readFileSync(log,'utf8').match(/GEZİNME #(\d+) .*\/idle\r?\n/)[1]);
  const protectedIds=['edited','peer','video'].map(name=>Number(fs.readFileSync(log,'utf8').match(new RegExp('GEZİNME #(\\d+) .*\\/'+name+'\\r?\\n'))[1]));
  const parked=await open('active');check(context.pages().length===6,'multiple content types load concurrently');
  const cookieFile=path.join(data,'session-default.bin');
  for(let i=0;i<350&&!fs.existsSync(cookieFile);i++)await delay(100);
  check(fs.existsSync(cookieFile),'session snapshot reaches disk');
  const cookieTime=fs.statSync(cookieFile).mtimeMs;
  await delay(40000);const logs=fs.readFileSync(log,'utf8');
  check(fs.statSync(cookieFile).mtimeMs===cookieTime,'unchanged session cookies are not rewritten on periodic polls');
  check(logs.includes('DONDUR #'+normalId+' true'),'idle normal tab actually suspends');
  for(const id of protectedIds)check(!logs.includes('DONDUR #'+id+' true')&&!logs.includes('UYUT #'+id),'live/edited tab protected: '+id);
  check(context.pages().includes(normal),'normal memory pressure retains suspended document');
  for(const p of [edited,peer,audio,parked])await close(p);
  // CDP can wake a suspended target; closure is intentionally after assertions.
  await close(normal);
  check(context.pages().length===1,'all fixture targets released');
  fs.writeFileSync(path.join(data,'results.json'),JSON.stringify({checks,samples,description:'90 open/close cycles; 10/30 simultaneous tabs; forms, live peer and video protection; unchanged cookie snapshots. No synthetic OS memory pressure; pressure thresholds tested in Rust.'},null,2));
  console.log(`PASS: ${checks} native resource assertions; 90 view cycles + 30 simultaneous tabs; ${data}`);
 }finally{
  cleaning=true;
  if(browser)await browser.close().catch(()=>{});
  await run('taskkill.exe',['/PID',String(child.pid),'/T','/F'],{windowsHide:true}).catch(()=>{});
  await new Promise(resolve=>server.close(resolve));
 }
})().catch(e=>{console.error(e);process.exitCode=1;server.close()});
