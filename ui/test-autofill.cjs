// Otomatik doldurmanın farklı giriş ekranlarında doğru alanı seçtiğini sınar.
// Run: PLAYWRIGHT_MODULE=<playwright-core yolu> node ui/test-autofill.cjs
// Sayfalar yerel sunucudan gelir; ağa çıkılmaz. Sitelerin yapısı örneklenir, kendileri açılmaz.
const fs = require('node:fs'), path = require('node:path'), http = require('node:http'), assert = require('node:assert/strict');
const {chromium} = require(process.env.PLAYWRIGHT_MODULE || 'playwright');
const autofill = fs.readFileSync(path.join(__dirname, 'autofill.js'), 'utf8');

const pages = {
  // T.C. kimlik alanı type="number"; şifre ayrı.
  '/giris/edevlet': `<title>Giriş</title><form><label for="tridField">T.C. Kimlik No</label><input id="tridField" name="tridField" type="number">
    <label for="egpField">Şifre</label><input id="egpField" name="egpField" type="password"><button>Giriş Yap</button></form>`,
  // Üstte arama kutusu, altta e-postalı giriş: arama kutusu asla seçilmez.
  '/login/search': `<title>Store</title><header><input type="search" name="q" placeholder="Ara"><input name="query" placeholder="Search products"></header>
    <form><input id="email" type="email" name="email" placeholder="E-posta"><input id="pw" type="password"><button>Sign in</button></form>`,
  // Telefon numarasıyla giriş.
  '/login/phone': `<title>Giriş</title><form><input id="phone" type="tel" name="phone" placeholder="Telefon numarası"><input id="pw" type="password"><button>Giriş</button></form>`,
  // Kullanıcı adı ile şifre arasında "beni hatırla" ve doğrulama kodu alanı.
  '/login/extra': `<title>Sign in</title><form><input id="user" name="login"><input id="otp" name="otp" placeholder="Doğrulama kodu" inputmode="numeric"><input id="pw" type="password"><button>Sign in</button></form>`,
  // İki adımlı giriş, web bileşeni (açık Shadow DOM) içinde: önce yalnızca hesap adı.
  '/signin/shadow': `<title>Sign In</title><main><auth-box></auth-box></main><script>
    customElements.define('auth-box', class extends HTMLElement { connectedCallback(){ const r=this.attachShadow({mode:'open'});
    r.innerHTML='<form><input id="account" type="text" placeholder="Email or Phone Number" aria-label="Account name"><button>Continue</button></form>'; } });</script>`,
  // Bülten formu: e-posta alanı var ama giriş değil, doldurulmaz.
  '/blog/post': `<title>Blog</title><article><p>Yazı</p></article><footer><form><p>Bültenimize abone olun</p><input id="news" type="email" name="email" placeholder="E-posta"><button>Subscribe</button></form></footer>`,
  // İletişim formu: textarea olan form giriş sayılmaz.
  '/contact': `<title>Account help</title><form><input id="mail" type="email" name="email"><input name="name"><textarea></textarea><button>Gönder</button></form>`,
  // Ödeme sayfasında kart alanı kullanıcı adı sanılmaz.
  '/checkout': `<title>Login</title><form><input id="card" name="cardnumber" autocomplete="cc-number" placeholder="Kart no"><input id="user" type="email" name="email"><input id="pw" type="password"><button>Log in</button></form>`,
  // Form etiketi olmayan, sonradan çizilen giriş (tek sayfa uygulamaları).
  '/app': `<title>App</title><div id="root"></div><script>setTimeout(()=>{document.getElementById('root').innerHTML='<div class="card"><div><input id="u" placeholder="Kullanıcı adı"></div><div><input id="pw" type="password" placeholder="Parola"></div><div role="button">Oturum aç</div></div>'},300)</script>`,
};

let checks = 0;
const check = (v, label) => { assert.ok(v, label); checks++; console.log('  ok', label); };
const server = http.createServer((req, res) => {
  const body = pages[new URL(req.url, 'http://x').pathname];
  res.writeHead(body ? 200 : 404, {'Content-Type': 'text/html; charset=utf-8'});
  res.end(body ? `<!doctype html><meta charset="utf-8"><body>${body}</body>` : '');
});

(async () => {
  await new Promise(r => server.listen(0, '127.0.0.1', r));
  const base = `http://127.0.0.1:${server.address().port}`;
  const executablePath = [process.env.BROWSER_EXECUTABLE, 'C:/Program Files/Google/Chrome/Application/chrome.exe', 'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe'].filter(Boolean).find(fs.existsSync);
  const browser = await chromium.launch({headless: true, executablePath});
  const context = await browser.newContext();
  await context.route('**/*', route => route.request().url().startsWith(base) ? route.continue() : route.abort());
  await context.addInitScript(() => { window.__messages = []; window.__listeners = []; window.chrome = {webview: {postMessage: m => window.__messages.push(m), addEventListener: (t, cb) => window.__listeners.push(cb)}}; });
  await context.addInitScript(autofill);
  const deliver = (page, data) => page.evaluate(d => window.__listeners.forEach(cb => cb({data: d})), data);
  // Şifre alanında hesabı seç, onaydan sonra gelen bilgiyle hangi alanın dolduğuna bak.
  async function pick(page, password, username = 'kullanici@example.com') {
    await page.locator(password).focus();
    await deliver(page, {type: 'pw.accounts', accounts: [{id: 1, username}, {id: 2, username: 'diger'}]});
    await page.locator(password).press('Enter');
    const req = await page.evaluate(() => window.__messages.find(m => m.type === 'pw.fill'));
    check(req && req.id === 1, `${page.url().replace(base, '')}: hesap seçimi onaya gönderildi`);
    await deliver(page, {type: 'pw.secret', id: 1, username, password: 'gizli'});
    check(await page.locator(password).inputValue() === 'gizli', `${page.url().replace(base, '')}: şifre onaydan sonra doldu`);
  }
  const open = async p => { const page = await context.newPage(); await page.goto(base + p); await page.waitForTimeout(250); return page; };
  try {
    let page = await open('/giris/edevlet');
    await pick(page, '#egpField', '12345678901');
    check(await page.locator('#tridField').inputValue() === '12345678901', 'e-Devlet: T.C. (type=number) alanı kullanıcı adı olarak seçildi');

    page = await open('/login/search');
    await pick(page, '#pw');
    check(await page.locator('#email').inputValue() === 'kullanici@example.com', 'arama kutusu atlandı, e-posta dolduruldu');
    check(await page.locator('input[name=q]').inputValue() === '' && await page.locator('input[name=query]').inputValue() === '', 'arama kutuları boş kaldı');

    page = await open('/login/phone');
    await pick(page, '#pw', '5551112233');
    check(await page.locator('#phone').inputValue() === '5551112233', 'telefonla giriş: tel alanı seçildi');

    page = await open('/login/extra');
    await pick(page, '#pw', 'ahmet');
    check(await page.locator('#user').inputValue() === 'ahmet' && await page.locator('#otp').inputValue() === '', 'doğrulama kodu alanı atlandı');

    page = await open('/checkout');
    await pick(page, '#pw');
    check(await page.locator('#card').inputValue() === '', 'kart numarası alanına kullanıcı adı yazılmadı');

    page = await open('/app');
    await page.waitForSelector('#pw'); await page.waitForTimeout(300);
    await pick(page, '#pw', 'zeynep');
    check(await page.locator('#u').inputValue() === 'zeynep', 'sonradan çizilen, form etiketsiz giriş eşleşti');

    page = await open('/signin/shadow');
    await page.waitForTimeout(200);
    check(await page.evaluate(() => window.__messages.some(m => m.type === 'pw.query')), 'Shadow DOM içindeki iki adımlı giriş bulundu');
    await deliver(page, {type: 'pw.accounts', accounts: [{id: 5, username: 'ben@example.com'}]});
    check(await page.locator('auth-box').evaluate(h => h.shadowRoot.querySelector('#account').value) === 'ben@example.com', 'tek hesapta kullanıcı adı hemen yazıldı (şifre değil)');

    page = await open('/blog/post');
    await deliver(page, {type: 'pw.accounts', accounts: [{id: 5, username: 'ben@example.com'}]});
    check(await page.locator('#news').inputValue() === '', 'bülten formuna e-posta yazılmadı');
    check(!(await page.evaluate(() => window.__messages.some(m => m.type === 'pw.query'))), 'giriş olmayan sayfada kasa sorgulanmadı');

    page = await open('/contact');
    await deliver(page, {type: 'pw.accounts', accounts: [{id: 5, username: 'ben@example.com'}]});
    check(await page.locator('#mail').inputValue() === '', 'iletişim formuna e-posta yazılmadı');

    console.log(`PASS: ${checks} autofill assertions`);
  } finally { await browser.close(); server.close(); }
})().catch(e => { console.error(e); process.exitCode = 1; server.close(); });
