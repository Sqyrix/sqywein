/* Run: PLAYWRIGHT_MODULE=<optional module path> node test-ui.cjs
 * Requires an installed playwright or playwright-core and Chrome/Edge.
 * Test server, browser profile and screenshots stay inside rust/ui/.qa.
 */
const fs = require('node:fs');
const path = require('node:path');
const http = require('node:http');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const root = __dirname;
const artifacts = path.join(root, '.qa');
fs.mkdirSync(artifacts, {recursive:true});
process.env.TEMP = artifacts;
process.env.TMP = artifacts;
const {chromium} = require(process.env.PLAYWRIGHT_MODULE || 'playwright');
const pages = ['history','downloads','bookmarks','passwords','extensions','settings'];
const errors = [];
const external = [];
let checks=0;
function check(condition, label) {assert.ok(condition,label);checks++;}
const delay = ms => new Promise(resolve => setTimeout(resolve,ms));
const server = http.createServer((request,response) => {
  const filename = decodeURIComponent(new URL(request.url,'http://localhost').pathname).slice(1);
  if (!['app.css','app.js',...pages.map(p => `${p}.html`)].includes(filename)) {response.writeHead(404);response.end();return;}
  response.setHeader('Content-Type',filename.endsWith('.css')?'text/css':filename.endsWith('.js')?'text/javascript':'text/html; charset=utf-8');
  response.end(fs.readFileSync(path.join(root,filename)));
});
async function main(){
  for(const name of ['app.js',...pages.map(p=>`${p}.html`)]){
    const source=fs.readFileSync(path.join(root,name),'utf8');
    if(name.endsWith('.js'))new vm.Script(source,{filename:name});
    else {for(const match of source.matchAll(/<script(?:\s[^>]*)?>([\s\S]*?)<\/script>/g))new vm.Script(match[1],{filename:name});check(Buffer.byteLength(source)<25000,`${name} size`);}
  }
  await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
  const base=`http://127.0.0.1:${server.address().port}`;
  const candidates=[process.env.BROWSER_EXECUTABLE,'C:/Program Files/Google/Chrome/Application/chrome.exe','C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe'].filter(Boolean);
  const executablePath=candidates.find(file=>fs.existsSync(file));
  const context=await chromium.launchPersistentContext(path.join(artifacts,'browser-profile'),{headless:true,executablePath,viewport:{width:1280,height:900},downloadsPath:artifacts,args:['--no-first-run','--disable-background-networking','--disable-component-update']});
  try{
    await context.route('**/*',route=>{const url=route.request().url();if(!url.startsWith(base) && !url.startsWith('data:')){external.push(url);return route.abort();}return route.continue();});
    const page=await context.newPage();
    page.on('pageerror',error=>errors.push(String(error)));
    page.on('console',msg=>{if(msg.type()==='error' && !msg.text().includes('favicon'))errors.push(msg.text());});
    await page.addInitScript(()=>{
      window.__messages=[];
      window.addEventListener('DOMContentLoaded',()=>{const original=send;window.send=(type,payload)=>{window.__messages.push({type,...payload});return original(type,payload);};});
    });
    const latest=async type=>page.evaluate(type=>window.__messages.filter(m=>m.type===type).at(-1),type);
    const wait=async()=>page.waitForTimeout(140);
    const goto=async name=>{await page.goto(`${base}/${name}.html`);await page.waitForLoadState('networkidle');};
    for(const width of [1280,600]){
      await page.setViewportSize({width,height:900});
      for(const name of pages){
        await goto(name);
        check(await page.locator('nav a').count()===6,`${name} navigation`);
        check(await page.locator('nav [aria-current="page"]').getAttribute('href')===`${name}.html`,`${name} active nav`);
        check(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),`${name} no overflow at ${width}`);
        check(await page.locator('h1').textContent()===await page.title(),`${name} heading/title`);
        await page.screenshot({path:path.join(artifacts,`${name}-${width}.png`),fullPage:true});
      }
    }
    await page.setViewportSize({width:1280,height:900});
    for(const width of [1280,600]){
      await page.setViewportSize({width,height:900});
      for(const name of pages){
        await goto(name);
        for(const theme of ['light','dark','sqywein']){
          if(name==='settings')await page.locator('#theme').selectOption(theme);
          else await page.evaluate(theme=>send('settings.set',{key:'theme',value:theme}),theme);
          await page.waitForFunction(theme=>document.documentElement.dataset.theme===theme,theme);
          check(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),`${name} ${theme} no overflow at ${width}`);
          check(await page.evaluate(()=>document.documentElement.style.colorScheme)===(theme==='light'?'light':'dark'),`${name} ${theme} color scheme`);
          if(name==='settings')await page.screenshot({path:path.join(artifacts,`theme-${theme}-${width}.png`),fullPage:true});
        }
      }
    }
    await page.setViewportSize({width:1280,height:900});await goto('settings');
    check(await page.locator('#theme option').allTextContents().then(v=>v.join('|'))==='Açık|Koyu|Sqywein|Sistem','four named theme choices');
    await page.emulateMedia({colorScheme:'light'});await page.locator('#theme').selectOption('system');
    await page.waitForFunction(()=>document.documentElement.style.colorScheme==='light');
    await page.emulateMedia({colorScheme:'dark'});await page.waitForFunction(()=>document.documentElement.style.colorScheme==='dark');
    check(await page.locator('#theme').inputValue()==='system','system theme follows changes while keeping the System preference');
    await page.locator('#theme').selectOption('sqywein');
    await goto('history');
    check(await page.locator('.row').count()===4,'history mock rows');
    await page.locator('#search').fill('MDN');await page.waitForTimeout(240);
    check(await page.locator('.row').count()===1,'history search');
    check((await latest('history.list')).query==='MDN','history search protocol');
    await page.locator('.row button').click();await wait();check(await page.locator('.row').count()===0,'history remove');
    await page.locator('#search').fill('');await page.waitForTimeout(240);
    await page.locator('#clear').click();await page.getByRole('button',{name:'Vazgeç',exact:true}).click();check(await page.locator('.row').count()===3,'clear cancellation');
    await page.locator('#clear').click();await page.getByRole('button',{name:'Onayla',exact:true}).click();await wait();check(await page.locator('.empty').count()===1,'history clear empty');
    await goto('downloads');
    await page.getByRole('button',{name:'Duraklat',exact:true}).click();await wait();check(await page.getByRole('button',{name:'Sürdür',exact:true}).count()===1,'download pause');
    await page.getByRole('button',{name:'Sürdür',exact:true}).click();await wait();check(await page.getByRole('button',{name:'Duraklat',exact:true}).count()===1,'download resume');
    const before=await page.locator('[role=progressbar]').getAttribute('aria-valuenow');await page.waitForTimeout(1600);const after=await page.locator('[role=progressbar]').getAttribute('aria-valuenow');check(Number(after)>Number(before),'live progress');
    await page.getByRole('button',{name:'İptal et',exact:true}).click();await wait();check(await page.locator('[role=progressbar]').count()===0,'download cancel');
    await page.getByRole('button',{name:'Dosyayı aç',exact:true}).click();check((await latest('downloads.open')).id==='d1','download open');
    await page.getByRole('button',{name:'Klasörde göster',exact:true}).click();check((await latest('downloads.show')).id==='d1','download show');
    await page.locator('#folder').click();check(!!await latest('downloads.openFolder'),'downloads folder');
    await page.evaluate(()=>receive({type:'downloads.update',item:{id:'new',name:'bilinmeyen.zip',url:'https://example.com/new',path:'C:/new',status:'progress',received:1024,total:0,when:new Date().toISOString()}}));
    check(await page.locator('.indeterminate').count()===1,'unknown download size');
    await page.locator('#search').fill('bulunmayan');check(await page.locator('.empty').count()===1,'download search empty');
    await page.locator('#search').fill('');await page.locator('#clear').click();await page.getByRole('button',{name:'Onayla',exact:true}).click();await wait();check((await page.locator('.row').count())===0,'downloads clear records');
    await goto('bookmarks');
    await page.locator('.row').first().getByRole('button',{name:'Adını değiştir',exact:true}).click();await page.getByRole('textbox',{name:'Yeni ad'}).fill('Yeni ad');await page.getByRole('textbox',{name:'Yeni ad'}).press('Escape');check(await page.locator('.edit-form').count()===0,'rename escape');
    await page.locator('.row a').first().dblclick();check(await page.locator('.edit-form').count()===1,'double click rename');
    await page.getByRole('textbox',{name:'Yeni ad'}).fill('Türkçe <img src=x onerror=alert(1)>');await page.getByRole('textbox',{name:'Yeni ad'}).press('Enter');await wait();
    check((await page.locator('.row a').first().textContent()).includes('<img'),'rename escaped');check(await page.locator('.row img').count()===0,'no injection');
    await page.locator('.row').first().getByRole('button',{name:/aşağı taşı/}).click();await wait();check((await latest('bookmarks.move')).from===0 && (await latest('bookmarks.move')).to===1,'keyboard reorder indexes');
    check(await page.evaluate(()=>document.activeElement.matches('.row a')),'reorder retains keyboard focus');
    await page.locator('#search').fill('MDN');await page.locator('.row').first().getByRole('button',{name:/aşağı taşı/}).click();await wait();check((await latest('bookmarks.move')).from===0 && (await latest('bookmarks.move')).to===1,'filtered reorder original indexes');
    await page.locator('#search').fill('');
    await page.locator('.row').first().dragTo(page.locator('.row').last());await wait();check((await latest('bookmarks.move')).from===0 && (await latest('bookmarks.move')).to===3,'drag reorder indexes');
    await page.locator('.row a').first().click({modifiers:['Control']});check((await latest('open')).newTab===true,'ctrl open new tab');
    await page.locator('.row a').first().click({button:'middle'});check((await latest('open')).newTab===true,'middle open new tab');
    await page.locator('.row a').first().click();await page.waitForTimeout(300);check((await latest('open')).newTab===true,'normal bookmark opens new tab');
    await page.locator('.row').first().getByRole('button',{name:'Yer işaretini sil',exact:true}).click();await wait();check(await page.locator('.row').count()===3,'bookmark removal');
    await goto('extensions');
    await page.locator('[role=switch]').first().click();await wait();check(await page.locator('[role=switch]').first().getAttribute('aria-checked')==='false','extension toggle');
    check(await page.evaluate(()=>document.activeElement.getAttribute('role')==='switch'),'extension refresh retains focus');
    await page.locator('[aria-pressed]').first().click();await wait();check(await page.locator('[aria-pressed]').first().getAttribute('aria-pressed')==='false','extension pin');
    await page.getByRole('button',{name:'Uzantıyı kaldır',exact:true}).first().click();await page.getByRole('button',{name:'Vazgeç',exact:true}).click();check(await page.locator('.row').count()===2,'extension remove cancel');
    await page.getByRole('button',{name:'Uzantıyı kaldır',exact:true}).first().click();await page.getByRole('button',{name:'Onayla',exact:true}).click();await wait();check(await page.locator('.row').count()===1,'extension removal');
    await page.locator('#store').click();check((await latest('open')).url==='https://chromewebstore.google.com/' && (await latest('open')).newTab,'extension store protocol');
    await page.locator('#load').click();check(!!await latest('extensions.loadUnpacked'),'unpacked protocol');
    await page.evaluate(()=>receive({type:'extensions.list',supported:false,items:[]}));check(await page.locator('#load').isDisabled() && await page.locator('#unsupported').isVisible(),'unsupported extension state');
    await goto('settings');
    check(await page.locator('[role=switch]').count()>=6,'settings switches');
    await page.locator('#searchEngine').selectOption('duckduckgo');check((await latest('settings.set')).key==='searchEngine' && (await latest('settings.set')).value==='duckduckgo','search setting');
    await page.locator('[data-setting=freezeTabs]').focus();await page.keyboard.press('Space');check((await latest('settings.set')).value===false,'keyboard switch boolean');
    await page.locator('#defaultZoom').selectOption('125');check((await latest('settings.set')).value===125,'zoom numeric');
    await page.locator('#startup').selectOption('newtab');await page.locator('#tracking').selectOption('strict');
    for(const key of ['sleepTabs','hardwareAcceleration','savePasswords','autofill']){await page.locator(`[data-setting=${key}]`).click();const message=await latest('settings.set');check(message.key===key && typeof message.value==='boolean',`${key} boolean setting`);}
    await page.locator('#passwords-manage').click();check((await latest('open')).url==='sqywein://passwords','password manager');
    await page.locator('input[value=history]').uncheck();check(await page.locator('#clear-button').isDisabled(),'no data selected');
    await page.locator('input[value=cookies]').check();await page.locator('#clear-button').click();await page.getByRole('button',{name:'Onayla',exact:true}).click();check(JSON.stringify((await latest('data.clear')).kinds)==='["cookies"]','selected data only');
    check(await page.getByRole('button',{name:'Profili kaldır',exact:true}).first().isDisabled(),'current profile protected');
    await page.locator('#profile-name').fill('Deneme');await page.getByRole('button',{name:'Profil ekle',exact:true}).click();await wait();check(await page.locator('#profiles .row').count()===3,'profile add');
    await page.locator('#profiles .row').last().getByRole('button',{name:'Adını değiştir',exact:true}).click();await page.getByRole('textbox',{name:'Yeni ad'}).fill('Yeni profil');await page.getByRole('textbox',{name:'Yeni ad'}).press('Enter');await wait();check((await latest('profiles.rename')).name==='Yeni profil','profile rename');
    await page.locator('#profiles .row').last().getByRole('button',{name:'Geçiş yap',exact:true}).click();await wait();check(await page.locator('#profiles .row').last().getByRole('button',{name:'Profili kaldır',exact:true}).isDisabled(),'profile switch');
    await page.locator('#profiles .row').first().getByRole('button',{name:'Profili kaldır',exact:true}).click();await page.getByRole('button',{name:'Onayla',exact:true}).click();await wait();check(await page.locator('#profiles .row').count()===2,'profile remove');
    // Functional settings navigation and empty states.
    await page.getByRole('button',{name:'Performans',exact:true}).click();
    check(await page.locator('#settings > section:visible').count()===1,'category isolates performance');
    await page.locator('#settings-search').fill('kamera');
    check(await page.locator('[aria-labelledby=permissions-heading]').isVisible(),'settings search finds camera permissions');
    await page.locator('#settings-search').fill('bulunmayan-ayar-xyz');
    check(await page.locator('#settings-empty').isVisible(),'settings no results');
    await page.getByRole('button',{name:'Tümü',exact:true}).click();
    await page.locator('#neverSleep').fill('meet.google.com, localhost');await page.locator('#neverSleep').press('Tab');
    check((await latest('settings.set')).key==='neverSleep','sleep exclusion protocol');
    await page.locator('#downloadFolder').fill('C:/Downloads');await page.locator('#downloadFolder').press('Tab');
    check((await latest('settings.set')).key==='downloadFolder','download folder protocol');
    await page.locator('#permission-origin').fill('https://example.com/path');
    await page.locator('#permission-kind').selectOption('2');await page.locator('#permission-state').selectOption('2');
    await page.locator('#permission-form button').click();
    check((await latest('permissions.set')).origin==='https://example.com' && (await latest('permissions.set')).state===2,'permission origin and deny state');
    await page.evaluate(()=>receive({type:'permissions.list',items:[{origin:'https://example.com',kind:2,state:2}]}));
    await page.locator('#permission-list button').click();check((await latest('permissions.set')).state===0,'permission reset');
    await goto('bookmarks');
    await page.locator('#bookmark-import').setInputFiles({name:'bookmarks.html',mimeType:'text/html',buffer:Buffer.from('<DL><DT><A HREF="https://example.com">Example</A><DT><A HREF="javascript:alert(1)">Unsafe</A></DL>')});
    await page.waitForTimeout(100);
    check((await latest('bookmarks.import')).items.length===1,'bookmark import rejects script URLs');
    await goto('passwords');
    check(await page.locator('.site-list').count()===1,'password page renders accounts');
    await page.locator('#add-open').click();
    await page.locator('#add-form [name=url]').fill('https://test.example');
    await page.locator('#add-form [name=username]').fill('test-user');
    await page.locator('#add-form [name=password]').fill('test-secret');
    await page.locator('#add-form button[type=submit]').click();
    check((await latest('passwords.add')).username==='test-user','password add protocol');
    await page.emulateMedia({reducedMotion:'reduce'});await goto('downloads');check(await page.locator('.progress span').evaluate(node=>getComputedStyle(node).transitionDuration)==='0s','reduced motion');
    // Exercise a real WebView2-shaped bridge: mock must remain absent.
    const native=await context.newPage();native.on('pageerror',e=>errors.push(String(e)));
    await native.addInitScript(()=>{window.__posted=[];window.chrome=window.chrome || {};window.chrome.webview={postMessage(message){window.__posted.push(message);},addEventListener(type,callback){window.__receive=callback;}};});
    await native.goto(`${base}/history.html`);check((await native.evaluate(()=>window.__posted)).some(m=>m.type==='history.list'),'native bridge initial request');
    await native.evaluate(()=>window.__receive({data:{type:'history.list',items:[{url:'https://example.com',title:'Gerçek köprü',when:'2026-01-01T10:00:00Z',visits:1}]}}));check(await native.locator('.row').count()===1,'native bridge receives');
    await native.locator('.row a').click();check((await native.evaluate(()=>window.__posted.at(-1))).type==='open','native bridge posts');
    await native.goto(`${base}/downloads.html`);
    await native.evaluate(()=>window.__receive({data:{type:'downloads.list',items:[]}}));check(await native.locator('.empty').count()===1,'native empty downloads');
    await native.evaluate(()=>window.__receive({data:{type:'downloads.update',item:{id:'native',name:'dosya.zip',url:'https://example.com',path:'C:/dosya.zip',status:'paused',received:1024,total:2048,when:new Date().toISOString()}}}));
    await native.getByRole('button',{name:'Sürdür',exact:true}).focus();
    await native.evaluate(()=>window.__receive({data:{type:'downloads.update',item:{id:'native',name:'dosya.zip',url:'https://example.com',path:'C:/dosya.zip',status:'paused',received:1536,total:2048,when:document.querySelector('.row').dataset.when || new Date().toISOString()}}}));
    check(await native.evaluate(()=>document.activeElement.tagName==='BUTTON'),'download update retains focus');
    await native.goto(`${base}/extensions.html`);
    await native.evaluate(()=>window.__receive({data:{type:'extensions.list',supported:true,items:[]}}));check(await native.locator('.empty').count()===1,'native empty extensions');
    await native.evaluate(()=>window.__receive({data:{type:'extensions.list',supported:true,items:[{id:'svg',name:'Yerel simge',enabled:true,pinned:false,icon:'data:image/svg+xml,%3Csvg xmlns="http://www.w3.org/2000/svg" width="32" height="32"%3E%3Crect width="32" height="32" fill="%23AF85FC"/%3E%3C/svg%3E'}]}}));
    check(await native.locator('.extension-icon').evaluate(node=>node.complete && node.naturalWidth>0),'data URL extension icon');
    await native.goto(`${base}/bookmarks.html`);await native.evaluate(()=>window.__receive({data:{type:'bookmarks.list',items:[]}}));check(await native.locator('.empty').count()===1,'native empty bookmarks');
    await native.close();
    check(errors.length===0,`browser errors: ${errors.join('\n')}`);check(external.length===0,`external requests: ${external.join('\n')}`);
    console.log(`PASS: ${checks} assertions; all six pages at 1280px and 600px; no browser errors or external requests.`);
    console.log(`Screenshots: ${artifacts}`);
  }finally{await context.close();}
}
main().catch(error=>{console.error(error);process.exitCode=1;}).finally(()=>server.close());
