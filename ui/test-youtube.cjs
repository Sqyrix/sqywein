// Offline regressions for the exact embedded YouTube module. No account/cookies.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const {chromium}=require(process.env.PLAYWRIGHT_MODULE||'playwright');
const source=fs.readFileSync(path.join(__dirname,'../src/adblock.rs'),'utf8');
const script=source.match(/pub const YOUTUBE_SCRIPT: &str = r#"([\s\S]*?)"#;/)[1];
let count=0;const check=(value,label)=>{assert.ok(value,label);count++;};
(async()=>{
 const browser=await chromium.launch({channel:'msedge',headless:true});
 try{
  const context=await browser.newContext();
  await context.route('https://www.youtube.com/**',route=>{
   if(route.request().url().includes('/youtubei/'))return route.fulfill({contentType:'application/json',body:JSON.stringify({adPlacements:[1],playerAds:[2],adSlots:[3],videoDetails:{title:'Real content'},streamingData:{formats:[{url:'https://media.example/content'}]}})});
   return route.fulfill({contentType:'text/html',body:`<!doctype html><style>[hidden]{display:none!important}#backdrop{position:fixed;inset:0;background:#0005;z-index:999}tp-yt-paper-dialog{position:fixed;z-index:1000}</style><body style="overflow:hidden"><button id="target" onclick="window.clicked=true">Page control</button><div class="html5-video-player"><video></video><button class="ytp-skip-ad-button-v2" onclick="window.skips=(window.skips||0)+1">Skip</button></div><tp-yt-iron-overlay-backdrop id="backdrop" opened></tp-yt-iron-overlay-backdrop><tp-yt-paper-dialog id="enforcement" opened><ytd-enforcement-message-view-model>Ad blocker warning</ytd-enforcement-message-view-model></tp-yt-paper-dialog><script>const d=document.getElementById('enforcement');d.opened=true;d.close=()=>{d.opened=false;d.removeAttribute('opened');d.hidden=true;document.getElementById('backdrop').hidden=true;document.body.style.overflow='';};<\/script>`});
  });
  const page=await context.newPage();await page.addInitScript(()=>window.originalFetch=window.fetch);await page.addInitScript(script);await page.goto('https://www.youtube.com/watch?v=fixture');
  await page.waitForFunction(()=>document.getElementById('enforcement')?.opened===false);
  check(await page.locator('#backdrop').isHidden(),'framework close releases the blocking backdrop');
  await page.locator('#target').click({timeout:2000});check(await page.evaluate(()=>window.clicked),'page controls remain clickable after enforcement dialog closes');
  check(await page.evaluate(()=>document.body.style.overflow===''),'modal scroll lock is released by the framework');
  check(await page.evaluate(()=>window.fetch===window.originalFetch),'native fetch identity and Response metadata are preserved');
  const response=await page.evaluate(async()=>{const r=await fetch('/youtubei/v1/player');const clone=r.clone();const data=await r.json();const copied=await clone.json();return {url:r.url,status:r.status,data,copied};});
  check(response.url==='https://www.youtube.com/youtubei/v1/player'&&response.status===200,'player response retains native URL/status');
  check(!response.data.adPlacements&&!response.data.playerAds&&!response.data.adSlots,'JSON response strips player ad fields');
  check(response.data.streamingData.formats[0].url==='https://media.example/content','real video stream remains intact');
  check(!response.copied.adPlacements,'cloned response is covered without replacing Response objects');
  const xhr=await page.evaluate(()=>new Promise(resolve=>{const x=new XMLHttpRequest();x.open('GET','/youtubei/v1/player');x.responseType='json';x.onload=()=>resolve(x.response);x.send();}));
  check(!xhr.playerAds&&!!xhr.streamingData,'native JSON XHR is covered');
  await page.evaluate(()=>{const v=document.querySelector('video');window.position=0;Object.defineProperty(v,'duration',{configurable:true,get:()=>30});Object.defineProperty(v,'currentTime',{configurable:true,get:()=>window.position,set:t=>window.position=t});v.muted=false;document.querySelector('.html5-video-player').classList.add('ad-showing');});
  await page.waitForFunction(()=>window.position===30);check(await page.locator('video').evaluate(v=>v.muted),'ad media is temporarily muted');
  check(await page.evaluate(()=>window.skips>0),'current skip-button selector is activated');
  await page.evaluate(()=>document.querySelector('.html5-video-player').classList.remove('ad-showing'));await page.waitForFunction(()=>!document.querySelector('video').muted);check(true,'original audio state returns after the ad');
  await page.evaluate(()=>{const v=document.querySelector('video');Object.defineProperty(v,'duration',{configurable:true,get:()=>Infinity});window.position=0;document.querySelector('.html5-video-player').classList.add('ad-showing');});await page.waitForTimeout(600);check(await page.evaluate(()=>window.position===0),'live/infinite streams are not sought to Infinity');
  await page.evaluate(script);check(await page.locator('#__sqYtStyle').count()===1,'reinstallation does not accumulate styles or workers');
  await page.evaluate(()=>window.__sqYt.setEnabled(false));check(await page.locator('#__sqYtStyle').count()===0,'disabling removes cosmetic rules');
  check(await page.locator('video').evaluate(v=>!v.muted),'disabling restores the prior mute state');
  const off=await page.evaluate(async()=>{const r=await fetch('/youtubei/v1/player');return r.json();});check(!!off.adPlacements&&!!off.playerAds,'disabled module leaves future player data unchanged');
  await page.evaluate(()=>{const d=document.createElement('tp-yt-paper-dialog');d.id='legitimate';d.setAttribute('opened','');d.opened=true;d.textContent='Account settings';d.close=()=>window.unwantedClose=true;document.body.append(d);window.__sqYt.setEnabled(true);});await page.waitForTimeout(600);
  check(await page.locator('#legitimate').isVisible()&&await page.evaluate(()=>!window.unwantedClose),'unrelated account/dialog overlays are preserved');
  await context.close();console.log(`PASS: ${count} YouTube modal, playback and toggle regressions (offline fixtures)`);
 }finally{await browser.close();}
})().catch(error=>{console.error(error);process.exitCode=1;});
