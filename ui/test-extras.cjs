// Native utility surfaces, real downloads, plus translation DOM regression tests.
const fs=require('node:fs'),path=require('node:path'),http=require('node:http'),net=require('node:net'),assert=require('node:assert/strict'),{spawn}=require('node:child_process');
const {chromium}=require(process.env.PLAYWRIGHT_MODULE||'playwright');
const root=path.resolve(__dirname,'..'),data=path.join(__dirname,'.qa',`extras-${Date.now()}`),exe=process.env.SQYWEIN_TEST_EXE||path.join(root,'target/debug/sqywein.exe');
fs.mkdirSync(path.join(data,'downloads'),{recursive:true});
fs.writeFileSync(path.join(data,'state.json'),JSON.stringify({DefaultPromptSeen:true,CurrentProfile:'default',Settings:{theme:'dark',adblock:false,autoUpdate:false,searchSuggestions:false,privacyShield:true,downloadFolder:path.join(data,'downloads')}}));
const sleep=ms=>new Promise(r=>setTimeout(r,ms));let count=0;function check(value,label){assert.ok(value,label);count++;}
(async()=>{
 const stream='BT /F1 18 Tf 40 100 Td (Sqywein PDF preview fixture) Tj ET';
 const objects=['<< /Type /Catalog /Pages 2 0 R >>','<< /Type /Pages /Kids [3 0 R] /Count 1 >>','<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 200] /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>',`<< /Length ${Buffer.byteLength(stream)} >>\nstream\n${stream}\nendstream`,'<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>'];
 let pdfData='%PDF-1.4\n';const offsets=[0];for(let i=0;i<objects.length;i++){offsets.push(Buffer.byteLength(pdfData));pdfData+=`${i+1} 0 obj\n${objects[i]}\nendobj\n`;}
 const xref=Buffer.byteLength(pdfData);pdfData+='xref\n0 6\n0000000000 65535 f \n'+offsets.slice(1).map(o=>String(o).padStart(10,'0')+' 00000 n \n').join('')+`trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n${xref}\n%%EOF\n`;
 const server=http.createServer((req,res)=>{
  if(req.url==='/pdf'){res.writeHead(200,{'Content-Type':'application/pdf','Content-Disposition':'attachment; filename=sample.pdf'});res.end(pdfData);return;}
  if(req.url==='/file'){res.writeHead(200,{'Content-Type':'text/plain; charset=utf-8','Content-Disposition':'attachment; filename="quick-look.txt"'});res.end('SQYWEIN QUICK LOOK\n<script>window.pwned=true</script>');return;}
  if(req.url==='/target'){res.end('<!doctype html><html lang="tr"><title>Preview target</title><h1>Preview destination</h1><p>Read-only browser preview.</p>');return;}
  res.setHeader('Content-Type','text/html; charset=utf-8');res.end('<!doctype html><html lang="tr"><title>Utility fixture</title><h1>Fixture</h1><a id="preview" href="/target">Preview link</a><a id="download" href="/file">Download fixture</a><p id="original">Original text</p><pre><span>NEVER TRANSLATE CODE</span></pre><div contenteditable="plaintext-only">NEVER TRANSLATE DRAFT</div><p translate="no">NEVER TRANSLATE EXCLUSION</p>');
 });await new Promise(r=>server.listen(0,'127.0.0.1',r));const base=`http://127.0.0.1:${server.address().port}`;
 const port=await new Promise(r=>{const s=net.createServer();s.listen(0,'127.0.0.1',()=>{const p=s.address().port;s.close(()=>r(p));});});
 const child=spawn(exe,['sqywein://settings'],{windowsHide:true,env:{...process.env,SQYWEIN_DATA:data,SQYWEIN_LOG:path.join(data,'browser.log'),WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:`--remote-debugging-port=${port} --remote-debugging-address=127.0.0.1`},stdio:'ignore'});let browser;
 try{
  for(let i=0;i<200;i++){try{if((await fetch(`http://127.0.0.1:${port}/json/version`)).ok)break;}catch{}await sleep(100);}
  browser=await chromium.connectOverCDP(`http://127.0.0.1:${port}`);const context=browser.contexts()[0];
  async function findPage(fragment){for(let i=0;i<150;i++){const p=context.pages().find(p=>p.url().includes(fragment));if(p)return p;await sleep(100);}throw Error('Missing page: '+fragment);}
  async function dismiss(p,action){const closed=p.waitForEvent('close');try{await action();}catch(e){if(!p.isClosed())throw e;}await closed;}
  const settings=await findPage('settings.html');await settings.locator('#settings:not([hidden])').waitFor();
  const rpc=(type,payload={})=>settings.evaluate(({type,payload})=>new Promise((resolve,reject)=>{const timer=setTimeout(()=>reject(Error(type)),5000);const off=on(type,m=>{clearTimeout(timer);off();resolve(m);});send(type,payload);}),{type,payload});
  let prefs=await rpc('settings.get');check(prefs.systemLanguage&&prefs.translationLanguages.length>=20,'computer locale and available languages delivered');
  await settings.locator('#translationTarget').selectOption('de');prefs=await rpc('settings.get');check(prefs.settings.translationTarget==='de','target language preference persists');
  await settings.locator('#translationAlways').fill('en, xx-invalid');await settings.locator('#translationAlways').press('Tab');prefs=await rpc('settings.get');check(JSON.stringify(prefs.settings.translationAlways)==='["en"]','invalid source preferences are rejected');
  await settings.locator('#translationNever').fill('en');await settings.locator('#translationNever').press('Tab');prefs=await rpc('settings.get');check(prefs.settings.translationNever.includes('en')&&!prefs.settings.translationAlways.includes('en'),'never and always preferences remain exclusive');
  await settings.locator('#translationTarget').selectOption('');
  await rpc('bookmarks.import',{items:[{title:'Spotlight saved destination',url:base+'/saved'}]});
  await settings.locator('[data-setting="animations"]').click();
  await settings.keyboard.press('Control+Space');let overlay=await findPage('overlay.html#spotlight');await overlay.locator('.result').first().waitFor();
  check(await overlay.locator('.surface').evaluate(e=>getComputedStyle(e).animationName)==='none','utility panels respect the disabled animation preference');
  check(await overlay.locator('#query').evaluate(e=>document.activeElement===e),'Spotlight receives keyboard focus');
  await overlay.locator('#query').fill('Spotlight saved');await overlay.locator('.result').filter({hasText:'Spotlight saved destination'}).waitFor();await overlay.waitForFunction(()=>!document.querySelector('#results').textContent.includes('Ayarlar'));check(await overlay.locator('.result').filter({hasText:'Yer işareti'}).count()>0,'unified search includes bookmarks');
  await overlay.screenshot({path:path.join(data,'spotlight-native.png')});await dismiss(overlay,()=>overlay.locator('#close').click());
  await settings.evaluate(url=>send('open',{url,newTab:true}),base+'/fixture');const page=await findPage('/fixture');await page.locator('#preview').waitFor();
  // Exercise the exact Rust-embedded collector, application script and restore code.
  const translation=fs.readFileSync(path.join(root,'src/app/translate.rs'),'utf8');const embedded=name=>translation.match(new RegExp(`const ${name}: &str = r#"([\\s\\S]*?)"#;`))[1];
  await page.evaluate(()=>window.__sqTrGeneration=17);const gathered=await page.evaluate(embedded('COLLECT'));
  check(gathered.items.some(([,t])=>t==='Original text'),'collector finds readable page text');check(!gathered.items.some(([,t])=>t.includes('NEVER TRANSLATE')),'collector protects nested code, editable drafts and exclusions');
  const item=gathered.items.find(([,t])=>t==='Original text');const applyTemplate=translation.match(/r#"(\(\(d\)=>[\s\S]*?)"#/)[1];
  const apply=(epoch,text)=>applyTemplate.replace('{epoch}',String(epoch)).replace('{data}',JSON.stringify([[item[0],text]])).replaceAll('{{','{').replaceAll('}}','}');
  await page.evaluate(apply(16,'Wrong document'));check(await page.locator('#original').textContent()==='Original text','stale translation cannot alter another generation');
  await page.evaluate(apply(17,'Translated text'));check(await page.locator('#original').textContent()==='Translated text','translation updates the original DOM node');
  await page.evaluate(()=>document.getElementById('original').firstChild.nodeValue='Site updated this text');await page.evaluate(embedded('RESTORE'));check(await page.locator('#original').textContent()==='Site updated this text','restoring translation preserves newer site edits');
  await page.locator('#preview').click({modifiers:['Shift']});const preview=await findPage('/target');await preview.locator('h1').waitFor();check(await preview.title()==='Preview target','Shift click opens a real link preview');
  await preview.evaluate(()=>{try{chrome.webview.postMessage({type:'settings.set',key:'theme',value:'light'});}catch{}});
  check((await rpc('settings.get')).settings.theme==='dark','remote preview cannot invoke privileged native commands');
  // Closing via window.close is intercepted by the native popup owner.
  await dismiss(preview,()=>preview.evaluate(()=>window.close()));
  await settings.evaluate(()=>send('browser.action',{action:'tabs'}));overlay=await findPage('overlay.html#tabs');await overlay.locator('.tile').first().waitFor();
  check(await overlay.locator('.tile').count()>=2,'Mission Control contains open tabs');
  await overlay.waitForFunction(()=>document.querySelector('.thumbnail'),{timeout:10000});check(await overlay.locator('.thumbnail').count()>0,'Mission Control captures native WebView thumbnails');
  await overlay.locator('#query').fill('Utility fixture');check(await overlay.locator('.tile').count()===1,'typing filters the tab grid');
  await overlay.screenshot({path:path.join(data,'tabs-native.png')});await dismiss(overlay,()=>overlay.locator('.tile').click());
  await page.evaluate(async()=>{const canvas=document.createElement('canvas');canvas.width=320;canvas.height=180;const g=canvas.getContext('2d');const stream=canvas.captureStream(5);const video=document.createElement('video');video.muted=true;video.autoplay=true;video.width=320;video.height=180;video.srcObject=stream;document.body.append(video);g.fillStyle='red';g.fillRect(0,0,320,180);await video.play();window.__qaStream=stream;});
  await page.waitForFunction(()=>document.querySelector('video').readyState>0);
  await settings.evaluate(()=>send('browser.action',{action:'spotlight'}));overlay=await findPage('overlay.html#spotlight');await overlay.locator('#query').fill('Görüntü içinde');await overlay.locator('.result').filter({hasText:'Görüntü içinde görüntü'}).first().waitFor();
  await dismiss(overlay,()=>overlay.locator('.result').filter({hasText:'Görüntü içinde görüntü'}).first().click());
  await page.waitForFunction(()=>document.pictureInPictureElement,{timeout:10000});check(await page.evaluate(()=>!!document.pictureInPictureElement),'native PiP action retains a valid user gesture');
  await page.evaluate(()=>document.exitPictureInPicture());check(await page.evaluate(()=>!document.pictureInPictureElement),'PiP can return video to its page');
  await page.locator('#download').click();
  let downloads;for(let i=0;i<100;i++){downloads=await rpc('downloads.list');if(downloads.items.some(d=>d.status==='done'))break;await sleep(100);}
  check(downloads.items.some(d=>d.status==='done'),'native download fixture completes');
  await settings.evaluate(()=>send('open',{url:'sqywein://downloads',newTab:true}));const downloadPage=await findPage('downloads.html');await downloadPage.locator('button').filter({hasText:'Hızlı Bakış'}).first().waitFor();
  // Long press the filename, not the explicit fallback button.
  const title=downloadPage.locator('.row-main button.title');await title.dispatchEvent('pointerdown',{button:0});await sleep(600);await title.dispatchEvent('pointerup',{button:0});
  overlay=await findPage('overlay.html#download');await overlay.locator('pre').waitFor();
  check((await overlay.locator('pre').textContent()).includes('SQYWEIN QUICK LOOK'),'holding a completed download previews its contents');
  check(await overlay.evaluate(()=>window.pwned!==true),'HTML-like download contents remain inert text');
  await overlay.screenshot({path:path.join(data,'quick-look-native.png')});await dismiss(overlay,()=>overlay.locator('#close').click());
  await settings.evaluate(url=>send('open',{url,newTab:true}),base+'/pdf');
  let pdfRecord;for(let i=0;i<100;i++){const list=await rpc('downloads.list');pdfRecord=list.items.find(d=>d.name==='sample.pdf'&&d.status==='done');if(pdfRecord)break;await sleep(100);}
  check(pdfRecord,'PDF download completes');await settings.evaluate(id=>send('downloads.preview',{id}),pdfRecord.id);
  const pdf=await findPage('sample.pdf');await pdf.waitForFunction(()=>document.contentType==='application/pdf'||document.querySelector('embed[type="application/pdf"]'));
  check(pdf.url().startsWith('file:///'),'Quick Look opens PDFs in its own built-in viewer');await dismiss(pdf,()=>pdf.evaluate(()=>window.close()));
  check(!context.pages().some(p=>p.url().includes('overlay.html')),'utility views are destroyed after dismissal');
  console.log(`PASS: ${count} utility and translation assertions. Screenshots: ${data}`);
 }finally{if(browser)await browser.close().catch(()=>{});child.kill();server.close();}
})().catch(e=>{console.error(e);process.exitCode=1;});
