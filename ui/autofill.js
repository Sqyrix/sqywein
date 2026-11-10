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
  // Kullanıcı adı olabilecek alan türleri (Chrome gibi: sayı ve telefon da; e-Devlet T.C. alanı type="number").
  const TEXTY = ['text','email','tel','number','url'];
  const ID_HINT = /user|e-?mail|login|logon|account|apple.?id|kullan|posta|hesap|kimlik|\btc\b|tckn|trid|identity|national|telefon|phone|gsm|mobil|\buid\b|m[üu][şs]teri|customer|abone|[öo][ğg]renci|student|sicil|member|[üu]ye/i;
  const NOT_ID = /search|\bara\b|arama|captcha|otp|one.?time|verification|do[ğg]rulama.?kod|\bcode\b|\bkod\b|coupon|kupon|promo|zip|postal|posta.?kodu|amount|tutar|card.?number|kart.?no|cvv|quantity/i;
  // Alanın insan için yazılmış adı: etiket, aria adı, yer tutucu, title, name ve id.
  function describeField(input) {
    const parts = [input.name, input.id, input.placeholder, input.title, input.getAttribute('aria-label') || ''];
    for (const label of input.labels || []) parts.push(label.textContent);
    for (const id of (input.getAttribute('aria-labelledby') || '').split(/\s+/)) { if (id) parts.push(document.getElementById(id)?.textContent || ''); }
    return parts.join(' ').replace(/\s+/g, ' ').slice(0, 400);
  }
  // Kimlik alanı puanı: site işareti (autocomplete) en güçlüsü, sonra tür ve ad.
  function identityScore(input) {
    const ac = input.autocomplete || '';
    if (/(?:^|\s)(one-time-code|cc-|new-password|current-password)/i.test(ac)) return -10;
    const text = describeField(input);
    let score = 0;
    if (/(?:^|\s)(username|email)(?:\s|$)/i.test(ac)) score += 4;
    if (/(?:^|\s)tel(?:\s|$)/i.test(ac)) score += 2;
    if (input.type === 'email') score += 2;
    if (ID_HINT.test(text)) score += 2;
    if (NOT_ID.test(text) || input.type === 'search') score -= 6;
    return score;
  }
  // Açık Shadow DOM'ların içi de taranır (Apple gibi web bileşenli girişler).
  function deepInputs(selector, root = document, out = []) {
    out.push(...root.querySelectorAll(selector));
    for (const el of root.querySelectorAll('*')) {
      if (el.shadowRoot) deepInputs(selector, el.shadowRoot, out);
    }
    return out;
  }
  function deepActive() {
    let el = document.activeElement;
    while (el?.shadowRoot?.activeElement) el = el.shadowRoot.activeElement;
    return el;
  }
  function pair(password) {
    const container = password.form || password.closest('section,article,[role="form"],dialog') || password.parentElement?.parentElement || password.getRootNode();
    const before = input => (!password.form || input.form === password.form) && (input.compareDocumentPosition(password) & Node.DOCUMENT_POSITION_FOLLOWING);
    // Adaylar: şifreden önceki metin benzeri alanlar; ayrıca adı kimlik olan "gizli" (şifre türünde) alanlar.
    const candidates = [...container.querySelectorAll('input')].filter(input => visible(input) && before(input) &&
      (TEXTY.includes(input.type) || (input.type === 'password' && ID_HINT.test(describeField(input)))) && identityScore(input) > -5);
    if (!candidates.length) return {password, username: null};
    // En yüksek puanlı; eşitlikte şifreye en yakın olan (Chrome'un "son metin alanı" kuralı).
    let best = candidates[candidates.length - 1], bestScore = identityScore(best);
    for (const input of candidates) { const sc = identityScore(input); if (sc > bestScore) { best = input; bestScore = sc; } }
    return {password, username: best};
  }
  // İki adımlı girişte (önce e-posta, sonra şifre) seçilen hesap ve yazılan ad hatırlanır.
  const memory = {
    get() { try { return JSON.parse(sessionStorage.getItem('__sqPwUser') || 'null'); } catch { return null; } },
    set(v) { try { sessionStorage.setItem('__sqPwUser', JSON.stringify(v)); } catch {} },
  };
  // Önce kullanıcı adı, sonra şifre istenen girişlerin ilk adımı (Apple, Google, Microsoft…).
  function loneUsernames() {
    const passwords = deepInputs('input[type="password"]').filter(visible);
    // Şifre türündeki bir alan aslında kimlik alanıysa (ör. maskeli T.C.) yine tek adım sayılmaz.
    if (passwords.some(p => !ID_HINT.test(describeField(p)))) return [];
    return deepInputs('input').filter(input => visible(input) && TEXTY.includes(input.type) && identityScore(input) >= 2 && loginContext(input));
  }
  // Tek başına duran e-posta alanı her zaman giriş değildir (bülten, iletişim, ödeme formu).
  // Site "username" demişse ya da çevresi açıkça bir giriş ekranıysa kabul edilir.
  const LOGIN = /log.?in|sign.?in|signin|giri[şs]|oturum|auth|hesab[ıi]n|account|idmsa|apple.?id|kimlik do[ğg]rula/i;
  const NOT_LOGIN = /newsletter|b[üu]lten|subscribe|abone ol|kaydol|sign.?up|register|[üu]ye ol|checkout|[öo]deme|ileti[şs]im|contact/i;
  function loginContext(input) {
    if (/(?:^|\s)(username|webauthn)(?:\s|$)/i.test(input.autocomplete || '')) return true;
    const box = input.form || input.closest('section,article,[role="form"],dialog,main') || input.getRootNode();
    if (box.querySelector?.('textarea,select')) return false;
    const texts = [...(box.querySelectorAll?.('input') || [])].filter(i => visible(i) && TEXTY.includes(i.type));
    if (texts.length > 2) return false;
    const around = (box.textContent || '').slice(0, 800) + ' ' + [...(box.querySelectorAll?.('button,input[type="submit"]') || [])].map(b => b.textContent + ' ' + (b.value || '')).join(' ');
    if (NOT_LOGIN.test(around)) return false;
    return LOGIN.test(around + ' ' + document.title + ' ' + location.pathname + ' ' + location.hostname);
  }
  function close() {
    popup?.remove(); popup = null; options = [];
    for (const input of active ? [active.username, active.password] : []) {
      input?.removeAttribute('aria-expanded');
    }
  }
  // Tek kayıtlı hesap varsa ve alan boşsa yalnızca kullanıcı adı hemen yazılır (Chrome gibi).
  // Şifre asla kendiliğinden doldurulmaz: seçilip Windows Hello ile onaylanınca dolar.
  const prefilled = new WeakSet();
  function prefill(input) {
    if (accounts.length !== 1 || !input || prefilled.has(input) || input.value || !visible(input) || !accounts[0].username) return;
    if (input.type === 'number' && !/^\d+$/.test(accounts[0].username)) return;
    if (input.type === 'email' && !accounts[0].username.includes('@')) return;
    prefilled.add(input);
    nativeSet.call(input, accounts[0].username);
    input.dispatchEvent(new Event('input',{bubbles:true})); input.dispatchEvent(new Event('change',{bubbles:true}));
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
    style.textContent = ':host{color-scheme:dark}*{box-sizing:border-box}.list{background:var(--sq-raised,#192737);color:var(--sq-ink,#F1F6FC);border:1px solid var(--sq-line,#36516B);border-radius:10px;padding:6px;box-shadow:0 8px 24px #0006;max-height:300px;overflow:auto;font:13px "Segoe UI Variable Text","Segoe UI",sans-serif}button{display:flex;gap:12px;align-items:center;text-align:left;width:100%;border:0;border-radius:6px;padding:9px 10px;background:transparent;color:inherit;font:inherit;cursor:pointer}button[aria-selected=true],button:hover,button:focus-visible{background:var(--sq-hover,#24384C);outline:none}.icon{color:var(--sq-muted,#B0C2D4);font-family:"Segoe Fluent Icons","Segoe MDL2 Assets",sans-serif;font-size:15px;width:18px;text-align:center;flex:none}.text{display:flex;flex-direction:column;min-width:0}.top{display:flex;gap:8px;align-items:baseline;min-width:0}.user{font-weight:600;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}.site{color:var(--sq-faint,#9AAFC4);font-size:12px;white-space:nowrap}.dots{color:var(--sq-muted,#B0C2D4);letter-spacing:1px;font-size:11px}.sep{height:1px;background:var(--sq-line,#36516B);margin:6px 4px}.manage{color:var(--sq-muted,#B0C2D4)}';
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
    input.addEventListener('input',() => { if (input.value) close(); else if (deepActive() === input) show(target(), input); });
    input.addEventListener('mousedown',() => { if (deepActive() === input && !popup) setTimeout(() => show(target(), input), 0); });
  }
  function scan() {
    const passwords = deepInputs('input[type="password"]').filter(visible);
    const lone = loneUsernames();
    if ((passwords.length || lone.length) && !queried) { queried = true; send({type:'pw.query'}); }
    for (const username of lone) {
      if (usernames.has(username)) continue;
      usernames.add(username);
      const target = {username, password:null};
      // Sayfa açılırken odaklanmış alan: odak olayı kaçtı, liste hemen gösterilir.
      if (deepActive() === username && accounts.length) { active = target; setTimeout(() => show(target, username), 0); }
      prefill(username);
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
        if (deepActive() === username && accounts.length) { active = pair(password); setTimeout(() => show(pair(password), username), 0); }
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
      const passwords = deepInputs('input[type="password"]').filter(visible);
      const target = passwords.find(p => p === deepActive() || pair(p).username === deepActive());
      // İki adımlı girişte önceki adımda seçilen hesap listenin başına alınır.
      const remembered = memory.get();
      if (remembered?.id != null) accounts.sort((a,b) => (b.id === remembered.id) - (a.id === remembered.id));
      if (target) show(pair(target),deepActive());
      else { const lone = loneUsernames().find(u => u === deepActive()); if (lone) { active = {username:lone, password:null}; show(active,lone); } }
      // Hesaplar geldi: tek adımlı girişte kullanıcı adı alanı tek hesapla doldurulur.
      if (!passwords.length) for (const lone of loneUsernames()) prefill(lone);
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
      : [...m.addedNodes].some(n => n.nodeType === 1 && (n.matches('input') || n.localName.includes('-') || !!n.shadowRoot || !!n.querySelector('input'))));
    if (active && !active.password?.isConnected && !active.username?.isConnected) { close(); active = null; }
    if (relevant && !timer) timer = setTimeout(() => { timer = null; scan(); },180);
  });
  observer.observe(document,{childList:true,subtree:true,attributes:true,attributeFilter:['type','hidden','style','class','autocomplete']});
  // Odaklanan her alan o an değerlendirilir: Shadow DOM, sonradan çizilen formlar.
  document.addEventListener('focusin', event => {
    const el = event.composedPath?.()[0];
    if (el instanceof HTMLInputElement && !fields.has(el) && !usernames.has(el)) setTimeout(scan, 0);
  }, true);
  scan();
  // Web bileşenleri gölge kökünü sonradan kurar: sayfa hazır olunca bir kez daha bakılır.
  if (document.readyState !== 'complete') {
    document.addEventListener('DOMContentLoaded', () => setTimeout(scan, 0), {once:true});
    window.addEventListener('load', () => setTimeout(scan, 300), {once:true});
  }
})();
