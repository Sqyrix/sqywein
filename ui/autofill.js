(() => {
  'use strict';
  if (!window.chrome?.webview || location.origin === 'https://sqywein.ui' || window.__sqPw) return;
  window.__sqPw = true;
  const bridge = window.chrome.webview;
  let appTheme = /*__SQ_THEME__*/null;
  const applyPopupTheme=()=>{
    if(!popup||!appTheme)return;
    for(const key of ['raised','ink','line','hover','muted','faint']){
      const value=appTheme.colors?.[key];if(/^#[0-9a-f]{6}$/i.test(value||''))popup.style.setProperty('--sq-'+key,value,'important');
    }
    popup.style.setProperty('color-scheme',appTheme.dark===false?'light':'dark','important');
  };
  const documentToken = crypto.randomUUID();
  Object.defineProperty(window, "__sqPwDocument", {value:documentToken});
  const send = message => bridge.postMessage({...message, documentToken});
  const nativeSet = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set;
  let accounts = [], queried = false, timer, active = null, pending = null, popup = null, options = [], selected = 0;
  const fields = new WeakSet();
  const usernames = new WeakSet();
  const submitted = new Map();
  const visible = input => input instanceof HTMLInputElement && input.isConnected && !input.disabled && !input.readOnly && input.getClientRects().length > 0 && getComputedStyle(input).visibility === 'visible';
  function pair(password) {
    const container = password.form || password.closest('section,article,[role="form"],dialog') || password.parentElement?.parentElement || document;
    const candidates = [...container.querySelectorAll('input')].filter(input =>
      visible(input) && ['text','email','tel'].includes(input.type) &&
      (!password.form || input.form === password.form) && (input.compareDocumentPosition(password) & Node.DOCUMENT_POSITION_FOLLOWING));
    return {password, username: candidates.filter(input => /(?:^|\s)(username|email)(?:\s|$)/i.test(input.autocomplete)).pop() || candidates.pop() || null};
  }
  // İki adımlı girişte (önce e-posta, sonra şifre) seçilen hesap ve yazılan ad hatırlanır.
  const memory = {
    get() { try { return JSON.parse(sessionStorage.getItem('__sqPwUser') || 'null'); } catch { return null; } },
    set(v) { try { sessionStorage.setItem('__sqPwUser', JSON.stringify(v)); } catch {} },
  };
  const USER_HINT = /user|email|e-mail|login|account|apple.?id|kullan|posta|hesap/i;
  function loneUsernames() {
    if ([...document.querySelectorAll('input[type="password"]')].some(visible)) return [];
    return [...document.querySelectorAll('input')].filter(input => visible(input) && ['text','email','tel'].includes(input.type) &&
      (/(?:^|\s)(username|email)(?:\s|$)/i.test(input.autocomplete) || input.type === 'email' || USER_HINT.test(input.name + ' ' + input.id + ' ' + (input.getAttribute('aria-label') || ''))));
  }
  function close() {
    popup?.remove(); popup = null; options = [];
    for (const input of active ? [active.username, active.password] : []) {
      input?.removeAttribute('aria-expanded');
    }
  }
  function fill(target, account) {
    close();
    if (!target || !(visible(target.password) || visible(target.username))) return;
    pending = {id:account.id, target};
    memory.set({id:account.id, username:account.username});
    send({type:'pw.fill', id:account.id});
  }
  function show(target, anchor) {
    close(); active = target;
    // Hiçbir şey kendiliğinden doldurulmaz: kutu boşsa hesaplar önerilir, seçilince
    // Sqywein Windows Hello ile doğrulayıp doldurur.
    if (!accounts.length || !visible(anchor) || anchor.value) return;
    const rect = anchor.getBoundingClientRect();
    // Liste alanın altına yerleşir; kapalı gölge kökü site stillerinden ayrılır.
    popup = document.createElement('div');
    popup.style.cssText = `all:initial!important;position:fixed!important;left:${Math.max(4,Math.min(rect.left,innerWidth-240))}px!important;top:${rect.bottom+5}px!important;width:${Math.min(Math.max(rect.width,230),innerWidth-8)}px!important;z-index:2147483647!important;`;
    const root = popup.attachShadow({mode:'closed'});
    applyPopupTheme();
    const style = document.createElement('style');
    style.textContent = ':host{color-scheme:dark}*{box-sizing:border-box}.list{background:var(--sq-raised,#211D2E);color:var(--sq-ink,#F4F1FC);border:1px solid var(--sq-line,#3D3654);border-radius:10px;padding:6px;box-shadow:0 8px 24px #0006;max-height:300px;overflow:auto;font:13px "Segoe UI Variable Text","Segoe UI",sans-serif}button{display:flex;gap:12px;align-items:center;text-align:left;width:100%;border:0;border-radius:6px;padding:9px 10px;background:transparent;color:inherit;font:inherit;cursor:pointer}button[aria-selected=true],button:hover,button:focus-visible{background:var(--sq-hover,#2E2940);outline:none}.icon{color:var(--sq-muted,#B8AED0);font-family:"Segoe Fluent Icons","Segoe MDL2 Assets",sans-serif;font-size:15px;width:18px;text-align:center;flex:none}.text{display:flex;flex-direction:column;min-width:0}.top{display:flex;gap:8px;align-items:baseline;min-width:0}.user{font-weight:600;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}.site{color:var(--sq-faint,#8E84A8);font-size:12px;white-space:nowrap}.dots{color:var(--sq-muted,#B8AED0);letter-spacing:1px;font-size:11px}.sep{height:1px;background:var(--sq-line,#3D3654);margin:6px 4px}.manage{color:var(--sq-muted,#B8AED0)}';
    const list = document.createElement('div'); list.className = 'list'; list.setAttribute('role','listbox'); list.setAttribute('aria-label','Kayıtlı hesaplar');
    selected = 0;
    const el = (tag, cls, text) => { const n = document.createElement(tag); if (cls) n.className = cls; if (text != null) n.textContent = text; return n; };
    const site = location.hostname.replace(/^www\./,'');
    options = accounts.map((account,index) => {
      const button = el('button'); button.type = 'button'; button.setAttribute('role','option'); button.setAttribute('aria-selected',String(index === 0));
      const icon = el('span','icon','\uE774'); icon.setAttribute('aria-hidden','true');
      const top = el('span','top'); top.append(el('span','user',account.username || 'Kullanıcı adı yok'), el('span','site',account.site || site));
      const text = el('span','text'); text.append(top, el('span','dots','••••••••'));
      button.append(icon,text);
      button.addEventListener('pointerdown',event => event.preventDefault());
      button.addEventListener('click',() => fill(target,account));
      button.addEventListener('keydown',event => keyboard(event,target));
      list.append(button); return button;
    });
    list.append(el('div','sep'));
    const manage = el('button','manage'); manage.type = 'button';
    const mi = el('span','icon','\uE8D7'); mi.setAttribute('aria-hidden','true');
    manage.append(mi, el('span',null,'Şifreleri yönet…'));
    manage.addEventListener('pointerdown',event => event.preventDefault());
    manage.addEventListener('click',() => { close(); send({type:'pw.manage'}); });
    list.append(manage);
    root.append(style,list); document.documentElement.append(popup); anchor.setAttribute('aria-expanded','true');
  }
  function keyboard(event,target) {
    if (!popup) return;
    if (event.key === 'Escape') { event.preventDefault(); event.stopPropagation(); close(); return; }
    if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      event.preventDefault(); selected = (selected + (event.key === 'ArrowDown' ? 1 : -1) + options.length) % options.length;
      options.forEach((option,index) => option.setAttribute('aria-selected',String(index === selected))); options[selected].scrollIntoView({block:'nearest'});
    } else if (event.key === 'Enter') { event.preventDefault(); event.stopPropagation(); fill(target,accounts[selected]); }
  }
  function capture(target) {
    if (!target || !visible(target.password) || !target.password.value) return;
    const username = target.username?.value || memory.get()?.username || '', password = target.password.value;
    const key = JSON.stringify([username,password]), time = Date.now();
    if (time - (submitted.get(key) || 0) < 2000) return;
    // Sadece kısa süreli yinelenen gönderimler tutulur.
    for (const [key,when] of submitted) if (time-when >= 2000) submitted.delete(key);
    submitted.set(key,time); send({type:'pw.submitted',username,password});
  }
  function watch(input, target) {
    input.addEventListener('input',() => { if (input.value) close(); else if (document.activeElement === input) show(target(), input); });
    input.addEventListener('mousedown',() => { if (document.activeElement === input && !popup) setTimeout(() => show(target(), input), 0); });
  }
  function scan() {
    const passwords = [...document.querySelectorAll('input[type="password"]')].filter(visible);
    const lone = loneUsernames();
    if ((passwords.length || lone.length) && !queried) { queried = true; send({type:'pw.query'}); }
    for (const username of lone) {
      if (usernames.has(username)) continue;
      usernames.add(username);
      const target = {username, password:null};
      username.addEventListener('focus',() => { active = target; show(target,username); });
      username.addEventListener('keydown',event => keyboard(event,target));
      watch(username, () => target);
      // Sonraki adıma geçerken yazılan ad hatırlanır (şifreyi kaydederken lazım).
      username.addEventListener('change',() => { if (username.value) memory.set({id:memory.get()?.id ?? null, username:username.value}); });
    }

    for (const password of passwords) {
      if (!fields.has(password)) {
        fields.add(password);
        password.addEventListener('focus',() => { active = pair(password); show(active,password); });
        watch(password, () => pair(password));
        password.addEventListener('keydown',event => {
          const target = pair(password); const choosing = !!popup;
          keyboard(event,target); if (event.key === 'Enter' && !choosing) capture(target);
        });
      }
      const username = pair(password).username;
      if (username && !usernames.has(username)) {
        usernames.add(username);
        username.addEventListener('focus',() => { active = pair(password); show(active,username); });
        username.addEventListener('keydown',event => keyboard(event,pair(password)));
        watch(username, () => pair(password));
      }
    }
  }
  const receivePassword = event => {
    const message = event.data;
    if (!message || typeof message !== 'object') return;
    if(message.type==='theme'){appTheme=message;applyPopupTheme();return;}
    if(message.theme){appTheme=message.theme;applyPopupTheme();}
    if (message.type === 'pw.accounts' && Array.isArray(message.accounts)) {
      accounts = message.accounts.filter(a => a && Number.isSafeInteger(a.id) && typeof a.username === 'string');
      close();
      const passwords = [...document.querySelectorAll('input[type="password"]')].filter(visible);
      const target = passwords.find(p => p === document.activeElement || pair(p).username === document.activeElement);
      // İki adımlı girişte önceki adımda seçilen hesap listenin başına alınır.
      const remembered = memory.get();
      if (remembered?.id != null) accounts.sort((a,b) => (b.id === remembered.id) - (a.id === remembered.id));
      if (target) show(pair(target),document.activeElement);
      else { const lone = loneUsernames().find(u => u === document.activeElement); if (lone) { active = {username:lone, password:null}; show(active,lone); } }
    } else if (message.type === 'pw.secret' && pending && message.id === pending.id && typeof message.username === 'string' && typeof message.password === 'string') {
      const target = pending.target; pending = null;
      if (!visible(target.password) && !visible(target.username)) return;
      for (const [input,value] of [[target.username,message.username],[target.password,message.password]]) {
        if (!input || !visible(input)) continue;
        nativeSet.call(input,value); input.dispatchEvent(new Event('input',{bubbles:true})); input.dispatchEvent(new Event('change',{bubbles:true}));
      }
    }
  };
  bridge.addEventListener('message', receivePassword);
  window.addEventListener('sq-password', event => { if(event.data?.documentToken === documentToken) receivePassword(event); });
  document.addEventListener('submit',event => {
    const password = [...event.target.querySelectorAll('input[type="password"]')].find(visible);
    if (password) capture(pair(password));
  },true);
  document.addEventListener('click',event => {
    if (popup && !event.composedPath().includes(popup) && event.target !== active?.username && event.target !== active?.password) close();
    const button = event.target instanceof Element && event.target.closest('button,input[type="submit"],input[type="image"],[role="button"]');
    if (!button || button.disabled || button.closest('[aria-disabled="true"]')) return;
    const submitLike = button.type === 'submit' || button.type === 'image' || /giriş|oturum|devam|login|log in|sign in|submit|continue/i.test(button.textContent || button.value || button.getAttribute('aria-label') || '');
    if (!submitLike) return;
    const container = button.form || button.closest('section,article,[role="form"],dialog') || button.parentElement?.parentElement;
    const password = container && [...container.querySelectorAll('input[type="password"]')].find(visible);
    if (password) capture(pair(password));
  },true);
  document.addEventListener('focusin',event => { if (popup && event.target !== active?.username && event.target !== active?.password && event.target !== popup) close(); });
  window.addEventListener('scroll',close,true); window.addEventListener('resize',close);
  // Feed animations and class/style changes outside inputs do not require a
  // complete login-form scan. Inspect only newly added input subtrees.
  const observer = new MutationObserver(mutations => {
    const relevant = mutations.some(m => m.type === 'attributes'
      ? (m.target instanceof HTMLInputElement || (m.target instanceof Element && !!m.target.querySelector('input[type="password"]')))
      : [...m.addedNodes].some(n => n.nodeType === 1 && (n.matches('input') || n.querySelector('input'))));
    if (active && !active.password?.isConnected && !active.username?.isConnected) { close(); active = null; }
    if (relevant && !timer) timer = setTimeout(() => { timer = null; scan(); },180);
  });
  observer.observe(document,{childList:true,subtree:true,attributes:true,attributeFilter:['type','hidden','style','class','autocomplete']});
  scan();
})();
