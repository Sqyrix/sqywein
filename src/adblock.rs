//! Reklam ve izleyici engelleyici.
//!
//! - Ağ: Brave'in `adblock` motoru, uBlock Origin'in kullandığı listelerle. Her
//!   istek (iframe ve worker'lar dahil) sayfaya gitmeden kontrol edilir.
//! - Kozmetik: siteye özel gizleme kuralları belge oluşurken, genel kurallar
//!   sayfadaki sınıf/kimlik adlarına göre sonradan eklenir.
//! - YouTube: reklamlar aynı alan adından geldiği için ağ engeli yetmez; kendi
//!   modülümüz oynatıcı yanıtlarındaki reklam alanlarını budar.
//!
//! Derlenmiş motor diske yazılır; açılışta milisaniyeler içinde yüklenir.
//! Listeler haftada bir arka planda yenilenir.

use crate::{net, state};
use adblock::{
    lists::{FilterSet, ParseOptions},
    request::Request,
    resources::PermissionMask,
};
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    path::PathBuf,
    time::{Duration, SystemTime},
};

/// (ad, adres, uBlock'un kendi listesi mi)
const LISTS: &[(&str, &str, bool)] = &[
    ("easylist", "https://easylist.to/easylist/easylist.txt", false),
    ("easyprivacy", "https://easylist.to/easylist/easyprivacy.txt", false),
    ("ubo-filters", "https://ublockorigin.github.io/uAssets/filters/filters.txt", true),
    ("ubo-privacy", "https://ublockorigin.github.io/uAssets/filters/privacy.txt", true),
    ("ubo-unbreak", "https://ublockorigin.github.io/uAssets/filters/unbreak.txt", true),
    ("ubo-quick-fixes", "https://ublockorigin.github.io/uAssets/filters/quick-fixes.txt", true),
    ("ubo-badware", "https://ublockorigin.github.io/uAssets/filters/badware.txt", true),
    // Türk siteleri için bölgesel liste (uBlock da Türkçe kullanıcılara bölgesel liste ekler).
    ("adguard-turkish", "https://filters.adtidy.org/extension/ublock/filters/13.txt", false),
];

/// "Tam" modda eklenen listeler: çerez uyarıları ve benzeri rahatsız edici öğeler.
const ANNOYANCE_LISTS: &[(&str, &str, bool)] = &[
    ("easylist-cookie", "https://secure.fanboy.co.nz/fanboy-cookiemonster.txt", false),
    ("ubo-cookies", "https://ublockorigin.github.io/uAssets/filters/annoyances-cookies.txt", true),
];

/// Filtreleme modları.
pub const OFF: u8 = 0;
/// Yalnızca ağ istekleri (reklam ve izleyici sunucuları).
pub const BASIC: u8 = 1;
/// İstekler + reklam kutularını gizleme + YouTube modülü.
pub const OPTIMAL: u8 = 2;
/// İdeal + çerez uyarıları ve rahatsız edici öğeler.
pub const COMPLETE: u8 = 3;

pub const LEVEL_NAMES: [&str; 4] = ["Kapalı", "Temel", "İdeal", "Tam"];
pub const LEVEL_HINTS: [&str; 4] = [
    "Bu sitede hiçbir şey engellenmez",
    "Reklam ve izleyici istekleri engellenir",
    "Reklamlar, boş kutular ve YouTube",
    "İdeal + çerez uyarıları",
];

pub use adblock::Engine as AdEngine;
pub type Engine = AdEngine;

/// YouTube modülü yalnızca modu "İdeal" ve üstü olan sitelerde çalışır.
/// `default_on`: varsayılan mod İdeal ve üstü mü; `off`/`on`: siteye özel istisnalar.
pub fn youtube_script(default_on: bool, off: &[String], on: &[String]) -> String {
    let list = |v: &[String]| serde_json::to_string(v).unwrap_or_else(|_| "[]".into());
    let (off, on) = (list(off), list(on));
    format!(
        "(()=>{{const h=location.hostname.replace(/^(www|m)\\./,'');\
         const enabled=({on}.includes(h)||({default_on}&&!{off}.includes(h)));window.__sqYt?.setEnabled?.(enabled);if(!enabled)return;{YOUTUBE_SCRIPT}}})();"
    )
}

/// Son güncellemeden bu yana: "bugün", "3 gün önce".
pub fn updated_label() -> String {
    match last_updated().and_then(|t| SystemTime::now().duration_since(t).ok()) {
        None => String::new(),
        Some(d) => match d.as_secs() / 86_400 {
            0 => "bugün".into(),
            1 => "dün".into(),
            n => format!("{n} gün önce"),
        },
    }
}

const UPDATE_EVERY: Duration = Duration::from_secs(7 * 24 * 3600);

thread_local! {
    static ENGINE: RefCell<Option<Engine>> = const { RefCell::new(None) };
    static ANNOY: RefCell<Option<Engine>> = const { RefCell::new(None) };
    static ENABLED: RefCell<bool> = const { RefCell::new(true) };
    static DEFAULT_LEVEL: RefCell<u8> = const { RefCell::new(OPTIMAL) };
    /// Sekme kimliği → sayfanın adresi (üçüncü taraf hesabı için).
    static PAGES: RefCell<HashMap<u32, String>> = RefCell::new(HashMap::new());
    /// Sekme kimliği → bu sayfada engellenen istek sayısı.
    static BLOCKED: RefCell<HashMap<u32, u32>> = RefCell::new(HashMap::new());
    /// Gizlilik raporu için: son okumadan beri engellenenler.
    static TOTAL: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    /// Site → kullanıcının seçtiği mod.
    static SITE_LEVELS: RefCell<HashMap<String, u8>> = RefCell::new(HashMap::new());
}

fn dir() -> PathBuf {
    state::root().join("adblock")
}

fn engine_file() -> PathBuf {
    dir().join("engine.bin")
}

fn annoy_file() -> PathBuf {
    dir().join("annoyances.bin")
}

pub fn set_enabled(on: bool) {
    ENABLED.with(|e| *e.borrow_mut() = on);
}

pub fn set_levels(default: u8, sites: impl IntoIterator<Item = (String, u8)>) {
    DEFAULT_LEVEL.with(|d| *d.borrow_mut() = default.min(COMPLETE));
    SITE_LEVELS.with(|m| *m.borrow_mut() = sites.into_iter().collect());
}

pub fn release() {
    ENGINE.with(|e| *e.borrow_mut() = None);
    ANNOY.with(|e| *e.borrow_mut() = None);
}

pub fn is_ready() -> bool {
    ENGINE.with(|e| e.borrow().is_some())
}

pub fn install(engine: Engine, annoyances: Option<Engine>) {
    ENGINE.with(|e| *e.borrow_mut() = Some(engine));
    ANNOY.with(|e| *e.borrow_mut() = annoyances);
}

/// Sekmenin ana sayfası değişti: sayaç sıfırlanır.
pub fn set_page(tab: u32, url: &str) {
    PAGES.with(|p| p.borrow_mut().insert(tab, url.to_string()));
    BLOCKED.with(|b| b.borrow_mut().remove(&tab));
}

pub fn page(tab: u32) -> String {
    PAGES.with(|p| p.borrow().get(&tab).cloned().unwrap_or_default())
}

pub fn forget(tab: u32) {
    PAGES.with(|p| p.borrow_mut().remove(&tab));
    BLOCKED.with(|b| b.borrow_mut().remove(&tab));
}

/// Son çağrıdan beri engellenen istek sayısı (sıfırlanır).
pub fn take_total() -> u64 {
    TOTAL.with(|t| t.replace(0))
}

pub fn blocked_count(tab: u32) -> u32 {
    BLOCKED.with(|b| b.borrow().get(&tab).copied().unwrap_or(0))
}

fn host_of(url: &str) -> &str {
    crate::url::host(url)
}

/// Sitenin seçili modu (siteye özel seçim yoksa varsayılan).
pub fn site_level(host: &str) -> u8 {
    SITE_LEVELS
        .with(|m| m.borrow().get(host).copied())
        .unwrap_or_else(|| DEFAULT_LEVEL.with(|d| *d.borrow()))
}

/// Bu sayfada geçerli olan mod; engelleyici kapalıysa ya da iç sayfaysa `OFF`.
pub fn level_for(page_url: &str) -> u8 {
    if !ENABLED.with(|e| *e.borrow()) || page_url.is_empty() || crate::url::is_internal(page_url) || !page_url.starts_with("http") {
        return OFF;
    }
    site_level(host_of(page_url))
}

fn check(engine: &RefCell<Option<Engine>>, req: &Request) -> bool {
    engine.borrow().as_ref().is_some_and(|e| e.check_network_request(req).should_block())
}

/// İstek engellenmeli mi? `kind`: adblock istek türü ("script", "image", ...).
pub fn should_block(tab: u32, url: &str, kind: &str) -> bool {
    PAGES.with(|pages| {
    let pages = pages.borrow();
    let page = pages.get(&tab).map(String::as_str).unwrap_or_default();
    let level = level_for(page);
    if level == OFF || url == page || !url.starts_with("http") {
        return false;
    }
    let Ok(req) = Request::new(url, page, kind, "get") else { return false };
    let block = ENGINE.with(|e| check(e, &req)) || (level >= COMPLETE && ANNOY.with(|e| check(e, &req)));
    if block {
        BLOCKED.with(|b| *b.borrow_mut().entry(tab).or_insert(0) += 1);
        TOTAL.with(|t| t.set(t.get() + 1));
        if crate::log::enabled() {
            crate::log::line(&format!("ENGEL #{tab} {kind} {url}"));
        }
    }
    block
    })
}

/// Siteye özel kozmetik kurallar: (belge oluşurken çalışacak betik, genel kurallar istenmeli mi, istisnalar).
pub fn page_resources(url: &str) -> Option<(String, bool, HashSet<String>)> {
    let level = level_for(url);
    if level < OPTIMAL {
        return None;
    }
    let main = ENGINE.with(|e| e.borrow().as_ref().map(|e| e.url_cosmetic_resources(url)))?;
    let mut hide: Vec<String> = main.hide_selectors.into_iter().collect();
    let mut exceptions = main.exceptions;
    if level >= COMPLETE {
        if let Some(extra) = ANNOY.with(|e| e.borrow().as_ref().map(|e| e.url_cosmetic_resources(url))) {
            hide.extend(extra.hide_selectors);
            exceptions.extend(extra.exceptions);
        }
    }
    let mut script = String::new();
    if !hide.is_empty() {
        let css = format!("{}{{display:none!important}}", hide.join(","));
        script.push_str(&style_script(host_of(url), &css));
    }
    Some((script, !main.generichide, exceptions))
}

/// Sayfadaki sınıf ve kimliklere göre genel gizleme CSS'i.
pub fn generic_css(page_url: &str, classes: &[String], ids: &[String], exceptions: &HashSet<String>) -> Option<Vec<String>> {
    let level = level_for(page_url);
    if level < OPTIMAL {
        return None;
    }
    let mut selectors = ENGINE.with(|e| e.borrow().as_ref().map(|e| e.hidden_class_id_selectors(classes, ids, exceptions)))?;
    if level >= COMPLETE {
        if let Some(extra) = ANNOY.with(|e| e.borrow().as_ref().map(|e| e.hidden_class_id_selectors(classes, ids, exceptions))) {
            selectors.extend(extra);
        }
    }
    selectors.sort_unstable(); selectors.dedup();
    (!selectors.is_empty()).then(|| selectors.into_iter().map(|s| format!("{s}{{display:none!important}}")).collect())
}

/// Belge oluşur oluşmaz `<style>` ekleyen betik (yalnızca o sitenin üst çerçevesinde).
fn style_script(host: &str, css: &str) -> String {
    let host = serde_json::to_string(host).unwrap_or_default();
    let css = serde_json::to_string(css).unwrap_or_default();
    format!("(()=>{{if(window.top!==window||location.hostname.replace(/^www\\./,'')!=={host})return;const add=()=>{{let s=window.__sqPageStyle;if(!s?.isConnected){{s=document.createElement('style');(document.head||document.documentElement).appendChild(s);window.__sqPageStyle=s;}}if(s.textContent!=={css})s.textContent={css};}};if(document.documentElement)add();else document.addEventListener('readystatechange',add,{{once:true}});}})();")
}

/// Sonradan eklenen genel CSS için `ExecuteScript` betiği.
pub fn append_css_script(rules: &[String]) -> String {
    let rules = serde_json::to_string(rules).unwrap_or_default();
    format!("(()=>{{let state=window.__sqGenericStyle;if(!state?.node?.isConnected){{const node=document.createElement('style');(document.head||document.documentElement).appendChild(node);state=window.__sqGenericStyle={{node,rules:new Set()}};}}for(const rule of {rules}){{if(state.rules.has(rule))continue;try{{state.node.sheet.insertRule(rule,state.node.sheet.cssRules.length);state.rules.add(rule);}}catch{{}}}}}})();")
}

/// Sayfadaki sınıf/kimlik adlarını toplayıp Sqywein'e bildiren betik (her sayfaya).
pub const COLLECTOR_SCRIPT: &str = r#"(() => {
  if (window.top !== window || !window.chrome || !window.chrome.webview) return;
  let observer = null, enabled = false, walker = null, root = null, scanTimer = 0;
  const roots = new Set();
  const seenC = new Set(), seenI = new Set();
  let pendingC = [], pendingI = [], timer = 0;
  const flush = () => {
    timer = 0;
    if (!pendingC.length && !pendingI.length) return;
    window.chrome.webview.postMessage({ type: 'sq.cosmetic', classes: pendingC, ids: pendingI });
    pendingC = []; pendingI = [];
  };
  const remember = (name, seen, pending) => {
    if (!name || name.length > 200 || seen.has(name)) return;
    if (seen.size >= 16000) seen.delete(seen.values().next().value);
    seen.add(name); pending.push(name);
    if (pending.length >= 2000) { clearTimeout(timer); flush(); }
  };
  const inspect = n => {
    if (n.nodeType !== 1) return;
    remember(n.id, seenI, pendingI);
    for (const c of n.classList) remember(c, seenC, pendingC);
  };
  const step = () => {
    scanTimer = 0;
    const deadline = performance.now() + 4;
    let count = 0;
    while (enabled && count < 256 && performance.now() < deadline) {
      if (!walker) {
        root = roots.values().next().value;
        if (!root) break;
        roots.delete(root);
        if (!root.isConnected) continue;
        walker = document.createTreeWalker(root, NodeFilter.SHOW_ELEMENT);
        inspect(root); count++;
      }
      if (!root.isConnected) { walker = null; root = null; continue; }
      const n = walker.nextNode();
      if (n) { inspect(n); count++; } else { walker = null; root = null; }
    }
    if (!timer && (pendingC.length || pendingI.length)) timer = setTimeout(flush, 300);
    if (enabled && (walker || roots.size)) scanTimer = setTimeout(step, 40);
  };
  const scan = node => {
    if (!enabled || !node.isConnected) return;
    if (roots.size >= 128) { roots.clear(); roots.add(document); }
    else if (!roots.has(document)) roots.add(node);
    if (!scanTimer) scanTimer = setTimeout(step, 0);
  };
  const start = () => {
    if (!enabled || observer) return;
    scan(document);
    observer = new MutationObserver(ms => { for (const m of ms) for (const n of m.addedNodes) if (n.nodeType === 1) scan(n); });
    observer.observe(document.documentElement, { childList: true, subtree: true });
  };
  window.__sqCollect = on => {
    if (enabled === !!on) return;
    enabled = !!on;
    if (enabled) {
      if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', start, {once:true}); else start();
    } else {
      document.removeEventListener('DOMContentLoaded', start);
      observer?.disconnect(); observer = null;
      clearTimeout(timer); clearTimeout(scanTimer); timer = scanTimer = 0;
      roots.clear(); walker = root = null;
      seenC.clear(); seenI.clear(); pendingC = []; pendingI = [];
    }
  };
})();"#;

/// YouTube modülü: oynatıcı yanıtlarındaki reklam alanlarını budar, yine de
/// reklam gelirse sessize alıp atlar ve reklam kutularını gizler.
pub const YOUTUBE_SCRIPT: &str = r#"(() => {
  if (!/(^|\.)youtube\.com$/.test(location.hostname)) return;
  if (window.__sqYt?.setEnabled) { window.__sqYt.setEnabled(true); return; }
  let enabled = true, timer = 0, savedMute = null;
  const KEYS = ['adPlacements', 'playerAds', 'adSlots', 'adBreakHeartbeatParams', 'adBreakParams'];
  const strip = o => {
    if (!enabled || !o || typeof o !== 'object') return o;
    for (const k of KEYS) if (k in o) delete o[k];
    if (o.playerResponse) strip(o.playerResponse);
    return o;
  };
  const parse = JSON.parse;
  JSON.parse = function () { return strip(parse.apply(this, arguments)); };
  let initial = window.ytInitialPlayerResponse;
  try { Object.defineProperty(window, 'ytInitialPlayerResponse', {configurable:true,get:()=>initial,set:v=>{initial=strip(v);}}); strip(initial); } catch {}
  // Preserve Response.url, headers, cloning and streaming instead of constructing
  // a different Response around a rewritten body.
  const responseJson = Response.prototype.json;
  Response.prototype.json = async function () {
    const value = await responseJson.apply(this, arguments);
    return /\/youtubei\/v1\/(player|next|reel)/.test(this.url) ? strip(value) : value;
  };
  const response = Object.getOwnPropertyDescriptor(XMLHttpRequest.prototype,'response');
  if (response?.get && response.configurable) {
    try { Object.defineProperty(XMLHttpRequest.prototype,'response',{...response,get(){const value=response.get.call(this);return /\/youtubei\/v1\/(player|next|reel)/.test(this.responseURL)?strip(value):value;}}); } catch {}
  }
  const css = '#masthead-ad,ytd-ad-slot-renderer,ytd-in-feed-ad-layout-renderer,ytd-banner-promo-renderer,'
    + 'ytd-promoted-sparkles-web-renderer,ytd-display-ad-renderer,ytd-companion-slot-renderer,#player-ads,'
    + 'ytd-statement-banner-renderer,.ytp-ad-overlay-container,ytd-rich-item-renderer:has(ytd-ad-slot-renderer),'
    + 'ytd-reel-video-renderer:has(.ytd-ad-slot-renderer){display:none!important}';
  let style;
  const restoreMute = () => { if(savedMute){if(savedMute.video.muted)savedMute.video.muted=savedMute.muted;savedMute=null;} };
  const tick = () => {
    timer=0; if(!enabled)return;
    if(!style?.isConnected && document.documentElement){style=document.createElement('style');style.id='__sqYtStyle';style.textContent=css;(document.head||document.documentElement).append(style);}
    // Never conceal a modal with CSS: its backdrop, focus trap and scroll lock
    // would survive. Let YouTube close its own overlay; retain visible fallback.
    for(const dialog of document.querySelectorAll('tp-yt-paper-dialog:has(ytd-enforcement-message-view-model)')){
      if(dialog.opened || dialog.hasAttribute('opened') || getComputedStyle(dialog).display!=='none'){
        try{if(typeof dialog.close==='function')dialog.close();}catch{}
        if(getComputedStyle(dialog).display==='none' && (dialog.opened || dialog.hasAttribute('opened')))dialog.style.setProperty('display','block','important');
      }
    }
    const player=document.querySelector('.html5-video-player');
    if(player?.classList.contains('ad-showing')){
      const video=player.querySelector('video');
      if(video){
        if(savedMute?.video!==video){restoreMute();savedMute={video,muted:video.muted};}
        video.muted=true;
        try{if(Number.isFinite(video.duration)&&video.duration>0&&video.duration<600)video.currentTime=video.duration;}catch{}
      }
      for(const button of player.querySelectorAll('.ytp-skip-ad-button,.ytp-ad-skip-button,.ytp-ad-skip-button-modern,.ytp-skip-ad-button-v2'))button.click();
    }else restoreMute();
    timer=setTimeout(tick,document.hidden?1500:500);
  };
  const observer=new MutationObserver(()=>{if(enabled&&!timer)timer=setTimeout(tick,100);});
  const setEnabled = on => {
    enabled=!!on;clearTimeout(timer);timer=0;observer.disconnect();
    if(!enabled){style?.remove();restoreMute();return;}
    observer.observe(document,{childList:true,subtree:true});tick();
  };
  window.__sqYt={setEnabled};
  document.addEventListener('yt-navigate-finish',()=>{if(enabled){clearTimeout(timer);timer=setTimeout(tick,0);}});
  setEnabled(true);
})();"#;

// ---------- Yükleme ve güncelleme ----------

/// Kayıtlı motor dosyasının yaşı (yoksa `None`).
fn engine_age() -> Option<Duration> {
    let modified = std::fs::metadata(engine_file()).ok()?.modified().ok()?;
    SystemTime::now().duration_since(modified).ok()
}

pub fn last_updated() -> Option<SystemTime> {
    std::fs::metadata(engine_file()).ok()?.modified().ok()
}

fn read_engine(path: &std::path::Path) -> Option<Engine> {
    let bytes = std::fs::read(path).ok()?;
    let mut engine = Engine::default();
    engine.deserialize(&bytes).ok()?;
    Some(engine)
}

/// Diskteki derlenmiş motorları okur (arka planda çağrılır): (ana, rahatsız edici öğeler).
pub fn load_cached() -> Option<(Engine, Option<Engine>)> {
    Some((read_engine(&engine_file())?, read_engine(&annoy_file())))
}

pub fn needs_update() -> bool {
    engine_age().is_none_or(|age| age > UPDATE_EVERY) || !annoy_file().exists()
}

/// Listeleri indirip tek motorda derler ve diske yazar. İnemeyen liste için önceki kopya kullanılır.
fn build_set(lists: &[(&str, &str, bool)], out: &std::path::Path) -> Option<Engine> {
    let lists_dir = dir().join("lists");
    let _ = std::fs::create_dir_all(&lists_dir);
    let mut set = FilterSet::new(false);
    let mut any = false;
    for (name, url, trusted) in lists {
        let path = lists_dir.join(format!("{name}.txt"));
        let text = match net::fetch(url, 16 * 1024 * 1024).filter(|b| b.len() > 1000) {
            Some(bytes) => {
                let _ = std::fs::write(&path, &bytes);
                String::from_utf8_lossy(&bytes).into_owned()
            }
            None => match std::fs::read_to_string(&path) {
                Ok(t) => t,
                Err(_) => continue,
            },
        };
        let opts = ParseOptions {
            // uBlock'un kendi listeleri uBlock'ta da "güvenilir" sayılır.
            permissions: if *trusted { PermissionMask::from_bits(0xFF) } else { PermissionMask::default() },
            ..Default::default()
        };
        set.add_filter_list(text, opts);
        any = true;
    }
    if !any {
        return None;
    }
    let engine = Engine::new_with_filter_set(set);
    let temp = out.with_extension("tmp");
    if std::fs::write(&temp, engine.serialize()).is_ok() {
        let _ = std::fs::rename(&temp, out);
    }
    Some(engine)
}

/// Tüm listeleri yeniler (arka planda çağrılır).
pub fn build() -> Option<(Engine, Option<Engine>)> {
    let main = build_set(LISTS, &engine_file())?;
    Some((main, build_set(ANNOYANCE_LISTS, &annoy_file())))
}

/// WebView2 kaynak bağlamı → adblock istek türü.
pub fn request_kind(context: i32, is_document_of_page: bool) -> &'static str {
    use webview2_com::Microsoft::Web::WebView2::Win32::*;
    match webview2_com::Microsoft::Web::WebView2::Win32::COREWEBVIEW2_WEB_RESOURCE_CONTEXT(context) {
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_DOCUMENT if is_document_of_page => "document",
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_DOCUMENT => "subdocument",
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_STYLESHEET => "stylesheet",
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_IMAGE => "image",
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_MEDIA => "media",
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_FONT => "font",
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_SCRIPT => "script",
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_XML_HTTP_REQUEST => "xhr",
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_FETCH => "fetch",
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_WEBSOCKET => "websocket",
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_PING => "ping",
        _ => "other",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_engine() -> Engine {
        let mut set = FilterSet::new(false);
        set.add_filter_list(
            "||doubleclick.net^\n||googlesyndication.com^\n/pagead/*\nexample.com##.ad-banner\n##.sponsored-box\n".into(),
            ParseOptions::default(),
        );
        Engine::new_with_filter_set(set)
    }

    #[test]
    fn ag_engeli() {
        install(test_engine(), None);
        set_enabled(true);
        set_levels(OPTIMAL, []);
        set_page(1, "https://example.com/haber");
        assert!(should_block(1, "https://securepubads.g.doubleclick.net/tag/js/gpt.js", "script"));
        assert!(should_block(1, "https://www.youtube.com/pagead/viewthroughconversion/1", "image"));
        assert!(!should_block(1, "https://example.com/style.css", "stylesheet"));
        assert_eq!(blocked_count(1), 2);
        // "Kapalı" seçilen sitede engel yok; "Temel"de istekler engellenir ama kutular gizlenmez.
        set_levels(OPTIMAL, [("example.com".to_string(), OFF)]);
        assert!(!should_block(1, "https://securepubads.g.doubleclick.net/tag/js/gpt.js", "script"));
        set_levels(OPTIMAL, [("example.com".to_string(), BASIC)]);
        assert!(should_block(1, "https://securepubads.g.doubleclick.net/tag/js/gpt.js", "script"));
        assert!(page_resources("https://example.com/").is_none());
    }

    #[test]
    fn kozmetik() {
        install(test_engine(), None);
        set_enabled(true);
        set_levels(OPTIMAL, []);
        let (script, generic, exceptions) = page_resources("https://example.com/").unwrap();
        assert!(script.contains(".ad-banner"));
        assert!(generic);
        let css = generic_css("https://example.com/", &["sponsored-box".into()], &[], &exceptions).unwrap();
        assert!(css.iter().any(|s| s.contains(".sponsored-box")));
    }

    /// Betikleri dosyaya yazar; `node --check` ile doğrulanır.
    #[test]
    #[ignore]
    fn betikleri_yaz() {
        let dir = std::env::temp_dir().join("sqywein-js");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("youtube.js"), youtube_script(true, &["example.com".into()], &["m.example.org".into()])).unwrap();
        std::fs::write(dir.join("collector.js"), COLLECTOR_SCRIPT).unwrap();
        std::fs::write(dir.join("style.js"), style_script("ornek.com", "a[href*='x']{display:none!important}")).unwrap();
        std::fs::write(dir.join("append.js"), append_css_script(&[".ad{display:none}".into(), ".another{display:none}".into()])).unwrap();
    }

    /// Ağ gerektirir: `cargo test -- --ignored gercek_listeler --nocapture`
    #[test]
    #[ignore]
    fn gercek_listeler() {
        net::init_thread();
        std::env::set_var("SQYWEIN_DATA", std::env::temp_dir().join("sqywein-adblock-test"));
        let started = std::time::Instant::now();
        let (engine, annoy) = build().expect("listeler inemedi");
        println!("derleme: {:?}, boyut: {} KB, çerez listesi: {}", started.elapsed(), engine.serialize().len() / 1024, annoy.is_some());
        install(engine, annoy);
        set_enabled(true);
        set_levels(OPTIMAL, []);
        set_page(7, "https://www.haberturk.com/");
        for (u, k) in [
            ("https://securepubads.g.doubleclick.net/tag/js/gpt.js", "script"),
            ("https://pagead2.googlesyndication.com/pagead/js/adsbygoogle.js", "script"),
            ("https://www.google-analytics.com/analytics.js", "script"),
            ("https://www.youtube.com/api/stats/ads?ver=2", "xhr"),
        ] {
            println!("{u} → {}", should_block(7, u, k));
        }
        let (script, generic, _) = page_resources("https://www.haberturk.com/").unwrap();
        println!("habertürk kozmetik: {} karakter, data-adv: {}, genel: {generic}", script.len(), script.contains("data-adv"));
        let started = std::time::Instant::now();
        let loaded = load_cached().expect("önbellek okunamadı");
        println!("diskten yükleme: {:?}", started.elapsed());
        drop(loaded);
    }
}
