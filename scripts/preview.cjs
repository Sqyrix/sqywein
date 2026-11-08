// Isolated visible preview; closing its window ends this helper.
const fs=require('node:fs'),path=require('node:path'),{spawn}=require('node:child_process');
const data=path.resolve(__dirname,'../ui/.qa',`preview-${Date.now()}`);fs.mkdirSync(data,{recursive:true});
fs.writeFileSync(path.join(data,'state.json'),JSON.stringify({DefaultPromptSeen:process.env.PREVIEW_ONBOARDING!=='1',CurrentProfile:'default',Settings:{theme:process.env.PREVIEW_THEME||'sqywein',adblock:false,autoUpdate:false,searchSuggestions:false,privacyShield:true}}));
const exe=process.env.SQYWEIN_TEST_EXE||path.resolve(__dirname,'../target/debug/sqywein.exe');
const child=spawn(exe,['sqywein://settings'],{windowsHide:false,stdio:'ignore',env:{...process.env,SQYWEIN_DATA:data,SQYWEIN_LOG:path.join(data,'browser.log'),WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:'--remote-debugging-port=9228 --remote-debugging-address=127.0.0.1'}});
console.log(JSON.stringify({pid:child.pid,data}));child.once('exit',code=>{console.log('Preview closed:',code);process.exitCode=code||0});
