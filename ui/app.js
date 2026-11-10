'use strict';
const bridge = window.chrome && window.chrome.webview;
const handlers = new Map();
on('motion', d => document.documentElement.classList.toggle('reduce-motion', d.enabled === false));
setTimeout(() => send('motion.get'), 0);
// Vurgu rengi ayarlardan değişince sayfa yeniden yüklenmeden güncellenir.
on('theme',d=>{
  const root=document.documentElement;
  if(['light','dark','sqywein','system'].includes(d.name))root.dataset.theme=d.name;
  if(typeof d.dark==='boolean')root.style.colorScheme=d.dark?'dark':'light';
  for(const key of ['base','raised','hover','line','ink','muted','faint','field','danger']){
    const value=d.colors?.[key];if(typeof value==='string'&&/^#[0-9A-F]{6}$/i.test(value))root.style.setProperty('--'+key,value);
  }
  if(typeof d.accent==='string'&&/^#[0-9A-F]{6}$/i.test(d.accent))root.style.setProperty('--accent',d.accent);
});
setTimeout(()=>send('theme.get'),0);
matchMedia('(prefers-color-scheme: dark)').addEventListener('change',()=>{if(document.documentElement.dataset.theme==='system')send('theme.get');});
function on(type, handler) {
  if (!handlers.has(type)) handlers.set(type, new Set());
  handlers.get(type).add(handler);
  return () => handlers.get(type).delete(handler);
}
function receive(message) {
  if (!message || typeof message !== 'object' || typeof message.type !== 'string') return;
  for (const handler of handlers.get(message.type) || []) handler(message);
}
function send(type, payload = {}) { (bridge || mockBridge).postMessage({...payload, type}); }
if (bridge) bridge.addEventListener('message', event => receive(event.data));
function el(tag, attrs = {}, ...children) {
  const node = document.createElement(tag);
  for (const [key, value] of Object.entries(attrs)) {
    if (key.startsWith('on')) node.addEventListener(key.slice(2).toLowerCase(), value);
    else if (key === 'class') node.className = value;
    else if (key === 'text') node.textContent = value;
    else if (value !== false && value != null) node.setAttribute(key, value === true ? '' : String(value));
  }
  node.append(...children.filter(child => child != null));
  return node;
}
const icons = {password:'\uE8D7',eye:'\uE7B3',copy:'\uE8C8',edit:'\uE70F',search:'\uE721',delete:'\uE74D',folder:'\uE8B7',open:'\uE8A7',close:'\uE8BB',star:'\uE734',pin:'\uE718',puzzle:'\uEA86',settings:'\uE713',history:'\uE81C',download:'\uE896'};
function icon(name) { return el('span', {class:'icon','aria-hidden':'true'}, icons[name] || ''); }
function action(label, fn, glyph, extra = {}) {
  return el('button', {type:'button',title:label,'aria-label':label,class:glyph ? 'icon-button quiet' : '',onclick:fn,...extra}, glyph ? icon(glyph) : label);
}
function host(url) { try { return new URL(url).hostname; } catch { return url || ''; } }
function safeURL(url) { try { const parsed = new URL(url); return ['https:','http:','file:','about:','edge:'].includes(parsed.protocol) ? parsed.href : '#'; } catch { return '#'; } }
function openLink(title, url, className = 'title ellipsis') {
  const link = el('a',{href:safeURL(url),class:className,title:url},title || host(url));
  const open = event => { if (event.button !== 0 && event.button !== 1) return; event.preventDefault(); if(safeURL(url)==='#')return;send('open',{url,newTab:event.button === 1 || event.ctrlKey || event.metaKey}); };
  link.addEventListener('click',open); link.addEventListener('auxclick',open);
  return link;
}
function badge(title) { return el('span',{class:'badge','aria-hidden':'true'},Array.from((title || '?').trim())[0]?.toLocaleUpperCase('tr-TR') || '?'); }
function empty(node, title, detail) { node.replaceChildren(el('div',{class:'empty'},el('strong',{},title),detail)); }
function debounce(fn, delay = 150) { let timer; return (...args) => {clearTimeout(timer);timer=setTimeout(() => fn(...args),delay);}; }
function bytes(value) { const n = Math.max(0,Number(value) || 0); return n >= 1073741824 ? `${(n / 1073741824).toLocaleString('tr-TR',{maximumFractionDigits:1})} GB` : n >= 1048576 ? `${(n / 1048576).toLocaleString('tr-TR',{maximumFractionDigits:1})} MB` : `${(n / 1024).toLocaleString('tr-TR',{maximumFractionDigits:0})} KB`; }
function dateValue(value) { const date = new Date(value); return Number.isNaN(+date) ? null : date; }
function dayLabel(value) { const date = dateValue(value); if (!date) return 'Tarihi bilinmiyor'; const today = new Date(); const yesterday = new Date(); yesterday.setDate(today.getDate()-1); if (date.toDateString() === today.toDateString()) return 'Bugün'; if (date.toDateString() === yesterday.toDateString()) return 'Dün'; return date.toLocaleDateString('tr-TR',{day:'numeric',month:'long',weekday:'long',...(date.getFullYear() !== today.getFullYear() ? {year:'numeric'} : {})}); }
let noticeTimer;
function notify(message) { const node = document.getElementById('notice'); node.textContent=message;node.hidden=false;clearTimeout(noticeTimer);noticeTimer=setTimeout(() => {node.hidden=true;},3500); }
function confirmInline(anchor, question, callback) {
  const existing = document.getElementById('confirmation'); if (existing) existing.remove();
  const panel=el('div',{class:'confirm',id:'confirmation',role:'group','aria-label':question});
  const close=() => {panel.remove();if (anchor.isConnected) anchor.focus();};
  panel.append(el('span',{},question),action('Vazgeç',close),action('Onayla',() => {close();callback();},null,{class:'danger'}));
  panel.addEventListener('keydown',event => {if (event.key === 'Escape') close();});
  const row = anchor.closest('.row'); if (row) row.after(panel); else anchor.closest('.toolbar,.page-actions,.card')?.after(panel);
  panel.querySelector('button').focus();
}
function inlineEdit(container, initial, save, cancel = () => {}) {
  if(container.querySelector('.edit-form'))return;
  const origin=document.activeElement;const previous=[...container.childNodes];const input=el('input',{type:'text',value:initial,'aria-label':'Yeni ad',maxlength:'200',required:true});
  const refocus=() => {if(origin?.isConnected && origin!==document.body)origin.focus();else container.closest('.row')?.querySelector('a,button')?.focus();};
  const restore=() => {container.replaceChildren(...previous);cancel();refocus();};
  const form=el('form',{class:'edit-form',onsubmit:event => {event.preventDefault();const value=input.value.trim();if (!value) {input.focus();return;}container.replaceChildren(...previous);refocus();save(value);}},input,el('button',{type:'submit'},'Kaydet'),action('Vazgeç',restore));
  input.addEventListener('keydown',event => {if(event.key==='Escape'){event.preventDefault();restore();}});
  container.replaceChildren(form);input.focus();input.select();
}
// Restore the same control after a native list refresh, or the nearest remaining row.
function rememberFocus(container) {
  const active=document.activeElement;if(!container.contains(active))return () => {};
  const rows=[...container.querySelectorAll('.row')];const row=active.closest('.row');if(!row)return () => {};
  const index=rows.indexOf(row);const key=row.dataset.key || row.dataset.url || row.dataset.id;
  const controls=[...row.querySelectorAll('a,button,input')];const controlIndex=controls.indexOf(active);
  const label=active.getAttribute('aria-label');
  return () => {const nextRows=[...container.querySelectorAll('.row')];const next=nextRows.find(node => key && (node.dataset.key || node.dataset.url || node.dataset.id)===key) || nextRows[Math.min(index,nextRows.length-1)];
    if(next){const candidates=[...next.querySelectorAll('a,button,input')].filter(node => !node.disabled);const match=(label && candidates.find(node => node.getAttribute('aria-label')===label)) || candidates[Math.min(controlIndex,candidates.length-1)] || candidates[0];match?.focus();}
    else {const search=document.getElementById('search');if(search)search.focus();}
  };
}
function initPage(page) {
  const entries=[['history','Geçmiş','history'],['downloads','İndirmeler','download'],['bookmarks','Yer işaretleri','star'],['passwords','Şifreler','password'],['extensions','Uzantılar','puzzle'],['settings','Ayarlar','settings']];
  const nav=el('nav',{'aria-label':'İç sayfalar'});
  for (const [id,title,glyph] of entries) nav.append(el('a',{href:`${id}.html`,...(id===page ? {'aria-current':'page'} : {})},icon(glyph),title));
  document.body.prepend(el('a',{href:'#main',class:'sr-only skip-link'},'İçeriğe geç'),el('aside',{class:'sidebar'},el('div',{class:'brand'},'Sqywein',el('span',{},'.')),nav));
  document.body.append(el('div',{id:'notice',class:'notice',role:'status','aria-live':'polite',hidden:true}));
  document.querySelectorAll('.search').forEach(label => label.prepend(icon('search')));
}
// Preview runs entirely in memory and is never installed inside WebView2.
let mockBridge;
if (!bridge) {
  const ago = (days, hours = 0) => {const d=new Date();d.setDate(d.getDate()-days);d.setHours(d.getHours()-hours);return d.toISOString();};
  const state={
    passwords:[{id:1,origin:'https://accounts.google.com',username:'ornek@example.com',password:'ornek-parola',note:'',lastUsed:ago(1)},{id:2,origin:'https://github.com',username:'ornek',password:'ikinci-ornek',note:'',lastUsed:ago(3)}],
    history:[{url:'https://www.rust-lang.org/learn',title:'Rust öğrenmeye başlayın',when:ago(0),visits:3},{url:'https://developer.mozilla.org/tr/docs/Web',title:'Web teknolojileri — MDN',when:ago(0,1),visits:8},{url:'https://tr.wikipedia.org/wiki/İstanbul',title:'İstanbul — Vikipedi',when:ago(1),visits:2},{url:'https://www.gutenberg.org/',title:'Project Gutenberg: ücretsiz kitaplar',when:ago(3),visits:1}],
    downloads:[{id:'d1',name:'sqywein-kilavuz.pdf',path:'C:\\Users\\Örnek\\Downloads\\sqywein-kilavuz.pdf',url:'https://example.com/kilavuz.pdf',status:'done',received:2516582,total:2516582,when:ago(0)},{id:'d2',name:'tatil-fotograflari.zip',path:'C:\\Users\\Örnek\\Downloads\\tatil-fotograflari.zip',url:'https://example.com/photos.zip',status:'progress',received:15728640,total:52428800,when:ago(0,1)},{id:'d3',name:'notlar.txt',path:'C:\\Users\\Örnek\\Downloads\\notlar.txt',url:'https://example.com/notes.txt',status:'failed',received:0,total:0,when:ago(1)}],
    bookmarks:[{title:'Rust programlama dili',url:'https://www.rust-lang.org/'},{title:'MDN Web Docs',url:'https://developer.mozilla.org/'},{title:'Vikipedi',url:'https://tr.wikipedia.org/'},{title:'Project Gutenberg',url:'https://www.gutenberg.org/'}],
    extensions:[{id:'e1',name:'uBlock Origin',enabled:true,pinned:true,icon:''},{id:'e2',name:'Dark Reader',enabled:false,pinned:false,icon:''}],
    settings:{theme:'sqywein',accent:'blue',searchEngine:'google',startup:'restore',freezeTabs:true,sleepTabs:false,hardwareAcceleration:true,defaultZoom:0,savePasswords:true,autofill:true,tracking:'balanced'},
    profiles:[{id:'p1',name:'Kişisel',email:'',color:'#0A84FF',current:true},{id:'p2',name:'İş',email:'ornek@example.com',color:'#8BBCC8',current:false}]
  };
  let historyQuery='';let historyLimit=500;let sequence=2;
  const reply=(type,payload) => receive(structuredClone({type,...payload}));
  const historyList=() => reply('history.list',{items:state.history.filter(item => `${item.title} ${item.url}`.toLocaleLowerCase('tr-TR').includes(historyQuery.toLocaleLowerCase('tr-TR'))).slice(0,historyLimit)});
  const settingsGet=() => reply('settings.get',{settings:state.settings,profiles:state.profiles,about:{version:'0.1.0 (önizleme)',engine:'WebView2 · önizleme verileri'}});
  const themeGet=()=>{
    const name=state.settings.theme||'sqywein';const light=name==='light'||(name==='system'&&!matchMedia('(prefers-color-scheme: dark)').matches);
    const keys=['base','raised','hover','line','ink','muted','faint','field','danger'];
    const values=light?['F5F6F8','FFFFFF','E9EBEF','C4C8D0','1C2027','4F5968','596473','E8EBF2','B42318']:name==='sqywein'?['101923','192737','24384C','36516B','F1F6FC','B0C2D4','9AAFC4','21344A','FF857C']:['151515','222222','303030','454545','F2F2F2','C0C0C0','9A9A9A','2B2B2B','FF857C'];
    const accent=light?'0066CC':'0A84FF';
    reply('theme',{name,dark:!light,accent:'#'+accent,colors:Object.fromEntries(keys.map((k,i)=>[k,'#'+values[i]]))});
  };
  let passwordQuery='';let passwordSequence=2;
  const passwordList=() => reply('passwords.list',{items:state.passwords.filter(i => `${i.origin} ${i.username}`.toLocaleLowerCase('tr-TR').includes(passwordQuery.toLocaleLowerCase('tr-TR'))).sort((a,b) => host(a.origin).localeCompare(host(b.origin)) || a.username.localeCompare(b.username)).map(({id,origin,username,lastUsed}) => ({id,origin,username,lastUsed}))});
  const passwordPut=(url,username,password,note) => {
    let origin;try{const parsed=new URL(url);if(!['https:','http:','android:'].includes(parsed.protocol))return 'skipped';origin=parsed.protocol==='android:'?`android://${parsed.host}`:parsed.origin;}catch{return 'skipped';}
    if(!password)return 'skipped';
    const old=state.passwords.find(i => i.origin===origin && i.username===username);
    if(old){if(old.password===password && (note===undefined || old.note===note))return 'skipped';old.password=password;if(note!==undefined)old.note=note;return 'updated';}
    state.passwords.push({id:++passwordSequence,origin,username,password,note:note || '',lastUsed:''});return 'added';
  };
  const parsePasswordCSV=text => {
    const rows=[];let row=[],field='',quoted=false,closed=false,bad=false,started=false;
    text=text.replace(/^\uFEFF/,'');
    for(let i=0;i<text.length;i++){const c=text[i];started=true;if(quoted){if(c==='"'){if(text[i+1]==='"'){field+='"';i++;}else{quoted=false;closed=true;}}else field+=c;}
      else if(c==='"' && !field && !closed)quoted=true;
      else if(c===','){row.push(field);field='';closed=false;}
      else if(c==='\r' || c==='\n'){if(c==='\r' && text[i+1]==='\n')i++;row.push(field);rows.push(bad?null:row);row=[];field='';closed=false;bad=false;started=false;}
      else {if(closed || c==='"')bad=true;field+=c;}}
    if(started){row.push(field);rows.push(bad || quoted?null:row);}return rows;
  };
  mockBridge={postMessage(message){setTimeout(() => {
    const {type}=message;
    switch(type){
      case 'passwords.list':passwordQuery=message.query || '';passwordList();break;
      case 'passwords.reveal':{const item=state.passwords.find(i => i.id===message.id);if(item)reply(type,{id:item.id,password:item.password});break;}
      case 'passwords.add':passwordPut(message.url,message.username,message.password);passwordList();break;
      case 'passwords.update':{const item=state.passwords.find(i => i.id===message.id);if(item && message.password && !state.passwords.some(i => i.id!==item.id && i.origin===item.origin && i.username===message.username)){item.username=message.username;item.password=message.password;}passwordList();break;}
      case 'passwords.remove':state.passwords=state.passwords.filter(i => i.id!==message.id);passwordList();break;
      case 'passwords.import':{const rows=parsePasswordCSV(message.csv || '');const header=rows.shift() || [];const index=name => header.findIndex(h => h.trim().toLowerCase()===name);const u=index('url'),n=index('username'),p=index('password'),note=index('note');const counts={added:0,updated:0,skipped:0};for(const row of rows){const result=row && u>=0 && n>=0 && p>=0?passwordPut(row[u] || '',row[n] || '',row[p] || '',note>=0?row[note] || '':undefined):'skipped';counts[result]++;}reply(type,counts);passwordList();break;}
      case 'passwords.export':{const quote=value => /[",\r\n]/.test(value)?`"${value.replaceAll('"','""')}"`:value;reply(type,{csv:'name,url,username,password,note\r\n'+state.passwords.map(i => [host(i.origin),i.origin,i.username,i.password,i.note].map(quote).join(',')+'\r\n').join('')});break;}
      case 'pw.query':reply('pw.accounts',{accounts:state.passwords.map(({id,username}) => ({id,username}))});break;
      case 'pw.fill':{const item=state.passwords.find(i => i.id===message.id);if(item)reply('pw.secret',{id:item.id,username:item.username,password:item.password});break;}
      case 'pw.submitted':notify('Önizleme: şifre kaydetme isteği alındı.');break;
      case 'open':notify(`Önizleme: ${message.newTab ? 'Yeni sekme' : 'Aç'} · ${message.url}`);break;
      case 'history.list':historyQuery=message.query || '';historyLimit=message.limit || 500;historyList();break;
      case 'history.remove':state.history=state.history.filter(i => i.url!==message.url);historyList();break;
      case 'history.clear':state.history=[];historyList();break;
      case 'downloads.list':reply(type,{items:state.downloads});break;
      case 'downloads.clear':state.downloads=state.downloads.filter(i => ['progress','paused'].includes(i.status));reply('downloads.list',{items:state.downloads});break;
      case 'downloads.remove':state.downloads=state.downloads.filter(i => i.id!==message.id);reply('downloads.list',{items:state.downloads});break;
      case 'downloads.pause':case 'downloads.resume':case 'downloads.cancel':{const item=state.downloads.find(i => i.id===message.id);if(item){item.status=type.endsWith('pause')?'paused':type.endsWith('resume')?'progress':'canceled';reply('downloads.update',{item});}break;}
      case 'downloads.open':case 'downloads.show':case 'downloads.openFolder':notify('Önizleme: dosya işlemi tarayıcıda uygulanmaz.');break;
      case 'bookmarks.list':reply(type,{items:state.bookmarks});break;
      case 'bookmarks.remove':state.bookmarks=state.bookmarks.filter(i => i.url!==message.url);reply('bookmarks.list',{items:state.bookmarks});break;
      case 'bookmarks.rename':{const item=state.bookmarks.find(i => i.url===message.url);if(item)item.title=message.title;reply('bookmarks.list',{items:state.bookmarks});break;}
      case 'bookmarks.move':{const [item]=state.bookmarks.splice(message.from,1);if(item)state.bookmarks.splice(message.to,0,item);reply('bookmarks.list',{items:state.bookmarks});break;}
      case 'extensions.list':reply(type,{supported:true,items:state.extensions});break;
      case 'extensions.remove':state.extensions=state.extensions.filter(i => i.id!==message.id);reply('extensions.list',{supported:true,items:state.extensions});break;
      case 'extensions.toggle':case 'extensions.pin':{const item=state.extensions.find(i => i.id===message.id);if(item){if(type.endsWith('toggle'))item.enabled=message.enabled;else item.pinned=message.pinned;}reply('extensions.list',{supported:true,items:state.extensions});break;}
      case 'extensions.loadUnpacked':notify('Önizleme: klasör seçici yalnızca Sqywein içinde açılır.');break;
      case 'settings.get':settingsGet();break;
      case 'theme.get':themeGet();break;
      case 'settings.set':state.settings[message.key]=message.value;if(['theme','accent'].includes(message.key))themeGet();break;
      case 'data.clear':if(message.kinds.includes('history'))state.history=[];reply(type,{kinds:message.kinds});break;
      case 'profiles.add':state.profiles.push({id:`p${++sequence}`,name:message.name,email:'',color:'#0A84FF',current:false});settingsGet();break;
      case 'profiles.rename':{const profile=state.profiles.find(i => i.id===message.id);if(profile)profile.name=message.name;settingsGet();break;}
      case 'profiles.remove':state.profiles=state.profiles.filter(i => i.id!==message.id || i.current);settingsGet();break;
      case 'profiles.switch':state.profiles.forEach(i => {i.current=i.id===message.id;});settingsGet();break;
    }
  },35);}};
  setInterval(() => {for(const item of state.downloads){if(item.status==='progress'){item.received=Math.min(item.total,item.received+1048576);if(item.received===item.total)item.status='done';reply('downloads.update',{item});}}},1500);
}

// Keep utility shortcuts available in internal pages and browser-based previews too.
document.addEventListener('keydown',event=>{
 if(!event.ctrlKey||event.altKey||event.metaKey)return;
 const action=(!event.shiftKey&&event.code==='Space')?'spotlight':(event.shiftKey&&event.code==='KeyA')?'tabs':null;
 if(action){event.preventDefault();send('browser.action',{action});}
});
