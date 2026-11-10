const fs=require('node:fs'),path=require('node:path'),net=require('node:net'),assert=require('node:assert/strict');
const {spawn,execFile}=require('node:child_process'),{promisify}=require('node:util'),run=promisify(execFile);
const {chromium}=require(process.env.PLAYWRIGHT_MODULE||'playwright');
const exe=process.env.SQYWEIN_TEST_EXE,data=path.join(__dirname,'.qa','menus-'+Date.now());fs.mkdirSync(data,{recursive:true});
fs.writeFileSync(path.join(data,'state.json'),JSON.stringify({DefaultPromptSeen:true,SessionClean:null,ReadingList:null,CurrentProfile:'default',Settings:{theme:'light',adblock:false,autoUpdate:false,searchSuggestions:false,translationAuto:false,showWidgets:false,animations:false}}));
const sleep=ms=>new Promise(r=>setTimeout(r,ms));let checks=0;const check=(v,m)=>{assert.ok(v,m);checks++;};
let child,browser;
const qa=async(action,name='')=>(await run('powershell.exe',['-NoProfile','-File',path.resolve(__dirname,'../scripts/qa-native.ps1'),'-BrowserProcessId',String(child.pid),'-Action',action,'-Name',name],{windowsHide:true,timeout:20000})).stdout;
(async()=>{
const port=await new Promise(r=>{const s=net.createServer();s.listen(0,'127.0.0.1',()=>{const p=s.address().port;s.close(()=>r(p));});});
child=spawn(exe,['sqywein://settings','--wait-pid=0'],{windowsHide:true,env:{...process.env,SQYWEIN_DATA:data,WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:`--remote-debugging-port=${port} --remote-debugging-address=127.0.0.1`},stdio:'ignore'});
try{
for(let i=0;i<200;i++){try{browser=await chromium.connectOverCDP('http://127.0.0.1:'+port);break;}catch{}await sleep(100);}assert.ok(browser);const context=browser.contexts()[0];let settings;
for(let i=0;i<100;i++){settings=context.pages().find(p=>p.url().includes('settings.html'));if(settings)break;await sleep(100);}await settings.locator('#settings:not([hidden])').waitFor();await qa('show');
const nativeNames=JSON.parse(await qa('names'));check(nativeNames.includes('Kenar çubuğu'),'sidebar accessible');check(nativeNames.includes('Sekmelere genel bakış'),'overview accessible');
await qa('invoke','Tarayıcı menüsü');let menuNames=JSON.parse(await qa('menu-names'));
for(const label of ['Yeni sekme','Yeni pencere','Yeni gizli pencere','Şifreler ve otomatik doldurma','Geçmiş','İndirmeler','Yer işaretleri ve listeler','Uzantılar','Tarama verilerini sil…','Tam ekran','Yazdır…','Kaydet ve paylaş','Bul ve düzenle','Diğer araçlar','Ayarlar','Yardım','Çıkış'])check(menuNames.includes(label),'main menu: '+label);
await qa('menu-invoke','Yer işaretleri ve listeler');menuNames=JSON.parse(await qa('menu-names'));check(menuNames.includes('Yeni pencere')&&menuNames.includes('Yer işaretlerini göster'),'submenu keeps parent open');
await qa('menu-invoke','Okuma Listesi (0)');menuNames=JSON.parse(await qa('menu-names'));check(menuNames.includes('Okuma Listesi boş'),'empty reading submenu opens without panic');
await qa('menu-invoke','Yer işaretleri çubuğunu gizle').catch(async()=>qa('menu-invoke','Yer işaretleri çubuğunu göster'));
await qa('invoke','Tarayıcı menüsü');await qa('menu-invoke','Diğer araçlar');menuNames=JSON.parse(await qa('menu-names'));check(menuNames.includes('Görev yöneticisi'),'tools submenu');
const panelReady=context.waitForEvent('page');await qa('menu-invoke','Görev yöneticisi');const tasks=await panelReady;await tasks.waitForURL('**/overlay.html#tasks');await tasks.getByRole('heading',{name:'Bu penceredeki sekmeler'}).waitFor();check((await tasks.locator('#results').textContent()).includes('Sqywein arayüzü'),'real process metrics rendered');await Promise.all([tasks.waitForEvent('close'),tasks.locator('#close').click()]);
await settings.locator('h1:visible').first().click({button:'right'});menuNames=JSON.parse(await qa('menu-names'));const reload=menuNames.find(name=>/^(Yenile|Reload)$/.test(name));check(reload,'runtime right click commands use the custom native menu');const reloaded=settings.waitForEvent('domcontentloaded');await qa('menu-invoke',reload);await reloaded;await settings.locator('#settings:not([hidden])').waitFor();check(await settings.title(),'context command executes through runtime deferral');
await qa('invoke','Kenar çubuğu');check(JSON.parse(await qa('names')).includes('Kenar çubuğu: Ayarlar'),'sidebar lists actual tabs');await qa('invoke','Kenar çubuğu');
const gridReady=context.waitForEvent('page');await qa('invoke','Sekmelere genel bakış');const grid=await gridReady;await grid.waitForURL('**/overlay.html#tabs');await grid.locator('.tile').first().waitFor();check(await grid.locator('.close-tab').count()===1,'each tab has independent close control');check(await grid.getByRole('button',{name:'Yeni sekme',exact:true}).count()===1,'new tab tile available');await Promise.all([grid.waitForEvent('close'),grid.getByRole('button',{name:'Yeni sekme',exact:true}).click()]);check(JSON.parse(await qa('names')).includes('Başlangıç sayfasını düzenle'),'new tab tile creates native start page');
await qa('close');for(let i=0;i<100&&child.exitCode===null;i++)await sleep(100);check(child.exitCode===0,'graceful exit');
const repair=await run(exe,['--repair-session'],{env:{...process.env,SQYWEIN_DATA:data},windowsHide:true});check(!repair.stderr,'legacy null session repairs on next load');
console.log(`PASS: ${checks} native menu, sidebar, overview and legacy session checks. ${data}`);
}finally{if(browser)await browser.close().catch(()=>{});if(child.exitCode===null)child.kill();}
})().catch(e=>{console.error(e);process.exitCode=1;});
