//! Sayfa çevirisi (Safari/Edge gibi, yerinde).
//!
//! Sayfa yüklenince dili algılanır; Türkçe değilse adres alanında çeviri düğmesi
//! çıkar. Basınca sayfadaki metin düğümleri toplanır, Rust tarafında Google
//! Çeviri'ye gönderilir ve dönen metin aynı düğümlere yazılır. Sayfanın CSP'si
//! engel olmaz (istek sayfadan değil tarayıcıdan gider), oturum bozulmaz.
//! Sonradan eklenen içerik (sonsuz kaydırma vb.) de çevrilir. Tekrar basınca
//! orijinal metin geri konur. Sayfa tarayıcıya mesaj gönderemez: bütün
//! alışveriş tarayıcının çalıştırdığı betiklerin dönüş değeriyle yapılır.

use super::*;
use serde_json::Value;

pub(super) const TIMER_TRANSLATE: usize = 7;
pub(super) const LANGUAGES: &[(&str, &str)] = &[
    ("tr", "Türkçe"), ("en", "İngilizce"), ("de", "Almanca"), ("fr", "Fransızca"),
    ("es", "İspanyolca"), ("it", "İtalyanca"), ("pt", "Portekizce"), ("nl", "Felemenkçe"),
    ("ru", "Rusça"), ("uk", "Ukraynaca"), ("ar", "Arapça"), ("fa", "Farsça"),
    ("ja", "Japonca"), ("ko", "Korece"), ("zh-CN", "Çince (Basitleştirilmiş)"), ("zh-TW", "Çince (Geleneksel)"),
    ("pl", "Lehçe"), ("sv", "İsveççe"), ("el", "Yunanca"), ("hi", "Hintçe"),
    ("az", "Azerbaycanca"), ("bg", "Bulgarca"), ("cs", "Çekçe"), ("da", "Danca"),
    ("fi", "Fince"), ("ro", "Rumence"), ("hu", "Macarca"), ("he", "İbranice"),
    ("id", "Endonezce"), ("ms", "Malayca"), ("vi", "Vietnamca"), ("th", "Tayca"),
    ("ka", "Gürcüce"), ("hy", "Ermenice"), ("kk", "Kazakça"), ("uz", "Özbekçe"),
    ("sr", "Sırpça"), ("hr", "Hırvatça"), ("bs", "Boşnakça"), ("sk", "Slovakça"),
    ("sl", "Slovence"), ("et", "Estonca"), ("lv", "Letonca"), ("lt", "Litvanca"),
    ("nb", "Norveççe"), ("is", "İzlandaca"), ("ga", "İrlandaca"), ("cy", "Galce"),
    ("sq", "Arnavutça"), ("mk", "Makedonca"), ("mt", "Maltaca"), ("be", "Belarusça"),
    ("bn", "Bengalce"), ("ur", "Urduca"), ("ta", "Tamilce"), ("te", "Teluguca"),
    ("mr", "Marathice"), ("gu", "Güceratça"), ("kn", "Kannadaca"), ("ml", "Malayalamca"),
    ("pa", "Pencapça"), ("ne", "Nepalce"), ("si", "Seylanca"), ("my", "Birmanca"),
    ("km", "Kmerce"), ("lo", "Laoca"), ("mn", "Moğolca"), ("am", "Amharca"),
    ("sw", "Svahili"), ("af", "Afrikanca"), ("zu", "Zuluca"), ("xh", "Xhosaca"),
    ("eu", "Baskça"), ("ca", "Katalanca"), ("gl", "Galiçyaca"), ("fil", "Filipince"),
];
pub(super) fn normalize_language(locale: &str) -> String {
    let l = locale.to_ascii_lowercase().replace('_', "-");
    if l.starts_with("zh") { return if l.contains("tw") || l.contains("hk") || l.contains("hant") { "zh-TW" } else { "zh-CN" }.into(); }
    if l.starts_with("no") || l.starts_with("nn") { return "nb".into(); }
    let base = l.split('-').next().unwrap_or("");
    if LANGUAGES.iter().any(|(code, _)| *code == base) { base.into() } else { "auto".into() }
}
pub(super) fn system_language() -> String {
    let mut buf = [0u16; 85];
    let len = unsafe {
        use windows::Win32::Globalization::*;
        let ui = LCIDToLocaleName(GetUserDefaultUILanguage() as u32, Some(&mut buf), 0);
        if ui > 0 { ui } else { GetUserDefaultLocaleName(&mut buf) }
    };
    let language = normalize_language(&String::from_utf16_lossy(&buf[..(len.max(1) as usize - 1).min(buf.len())]));
    if language == "auto" { "en".into() } else { language }
}
/// Dil bildirmeyen sayfanın metin örneği Türkçe mi? (Türkçeye özgü harf oranı.)
fn sample_is_turkish(sample: &str) -> bool {
    let letters = sample.chars().filter(|c| c.is_alphabetic()).count();
    let turkish = sample.chars().filter(|c| "ğĞşŞıİçÇöÖüÜ".contains(*c)).count();
    letters >= 40 && turkish * 100 >= letters
}
fn language_name(code: &str) -> &str {
    LANGUAGES.iter().find(|(c, _)| *c == code).map(|(_, n)| *n).unwrap_or("Dili algıla")
}
fn preference_toggle(list: &mut Vec<String>, value: &str) {
    if list.iter().any(|s| s == value) { list.retain(|s| s != value); }
    else if list.len() < 200 { list.push(value.into()); }
}
fn script_for(tab: &Tab, script: &str) -> String {
    format!("(()=>{{try{{const v={script};if(v){{v.g={};v.e={};}}return v;}}catch{{return{{k:'tr',g:{},e:{},error:true}};}}}})()", tab.tr_generation, tab.epoch, tab.tr_generation, tab.epoch)
}

/// Bir istekte gönderilecek en fazla karakter (adres uzunluğu sınırı için).
const BATCH_CHARS: usize = 700;

/// Dil algılama: sayfanın bildirdiği dil ve kısa bir metin örneği.
const DETECT: &str = r#"(()=>{const h=document.documentElement;return {k:'lang',v:(h.lang||'').toLowerCase(),no:h.getAttribute('translate')==='no',s:(document.body?document.body.innerText:'').slice(0,600)}})()"#;

/// Collect at most 80 new nodes; retain at most 10,000 nodes / 500,000 source characters.
const COLLECT: &str = r#"(()=>{const T=window.__sqTr||(window.__sqTr={g:window.__sqTrGeneration,chars:0,limited:false,a:[],n:[],o:[],seen:new WeakSet(),ids:new WeakMap(),dirty:true,obs:null});
if(!T.obs&&document.body){T.obs=new MutationObserver(records=>{for(const r of records){if(r.type==='characterData'){const i=T.ids.get(r.target);if(i!==undefined&&T.a[i]===r.target.nodeValue)continue;if(i!==undefined){T.n[i]=null;T.o[i]=null;T.a[i]=null;T.ids.delete(r.target);T.seen.delete(r.target);}}T.dirty=true;}});T.obs.observe(document.body,{childList:true,characterData:true,subtree:true});}
if(!T.dirty)return {k:'tr',items:[],limited:T.limited};T.dirty=false;
const skip=/^(SCRIPT|STYLE|NOSCRIPT|CODE|PRE|TEXTAREA|INPUT|SELECT|OPTION|SVG|MATH|KBD|SAMP|VAR|TEMPLATE)$/;
if(!document.body)return {k:"tr",items:[]};
for(let i=0;i<T.n.length;i++){if(T.n[i]&&!T.n[i].isConnected){T.n[i]=null;T.o[i]=null;T.a[i]=null;}}
const w=document.createTreeWalker(document.body,NodeFilter.SHOW_TEXT,{acceptNode(n){if(T.seen.has(n))return 2;const p=n.parentElement;
if(!p||skip.test(p.tagName)||p.closest('script,style,noscript,code,pre,textarea,input,select,option,svg,math,kbd,samp,var,template,[contenteditable]:not([contenteditable="false"]),[translate="no"],.notranslate'))return 2;
const t=n.nodeValue;if(t&&t.length>16000){T.limited=true;return 2;}if(!t||t.trim().length<2||!/\p{L}/u.test(t))return 2;return 1;}});
const items=[];let n;while((n=w.nextNode())&&items.length<80&&T.n.length<10000){if(T.chars+n.nodeValue.length>500000){T.limited=true;break;}T.chars+=n.nodeValue.length;T.seen.add(n);const i=T.n.length;T.ids.set(n,i);T.n.push(n);T.o.push(n.nodeValue);items.push([i,n.nodeValue.trim().replace(/\s+/g,' ')]);}
if(T.n.length>=10000)T.limited=true;if(items.length>=80)T.dirty=true;return {k:'tr',items,limited:T.limited};})()"#;

/// Orijinal metni geri koyar.
const RESTORE: &str = r#"(()=>{const T=window.__sqTr;if(!T)return;if(T.obs)T.obs.disconnect();T.n.forEach((n,i)=>{if(n&&n.isConnected&&n.nodeValue===T.a[i])n.nodeValue=T.o[i]});delete window.__sqTr;})()"#;

/// Google Çeviri'nin yanıtından çevrilmiş metni çıkarır.
fn parse_translation(body: &[u8]) -> Option<String> {
    let v: Value = serde_json::from_slice(body).ok()?;
    let parts = v.get(0)?.as_array()?;
    let text: String = parts.iter().filter_map(|p| p.get(0).and_then(Value::as_str)).collect();
    (!text.trim().is_empty()).then_some(text)
}

fn translate_text(text: &str, source: &str, target: &str, cancel: &std::sync::atomic::AtomicBool) -> Option<String> {
    if cancel.load(std::sync::atomic::Ordering::Relaxed) { return None; }
    if text.chars().count() > BATCH_CHARS {
        let chars: Vec<char> = text.chars().collect();
        return chars.chunks(BATCH_CHARS).map(|part| translate_text(&part.iter().collect::<String>(), source, target, cancel)).collect::<Option<Vec<_>>>().map(|parts| parts.join(" "));
    }
    let u = format!("https://translate.googleapis.com/translate_a/single?client=gtx&sl={source}&tl={target}&dt=t&q={}", url::encode(text));
    crate::net::fetch(&u, 2 * 1024 * 1024).and_then(|b| parse_translation(&b))
}

/// Bir grup metni tek istekte çevirir; satır sayısı tutmazsa tek tek çevirir.
fn translate_batch(items: &[(u32, String)], source: &str, target: &str, cancel: &std::sync::atomic::AtomicBool) -> Vec<(u32, String)> {
    let joined = items.iter().map(|(_, t)| t.replace('\n', " ")).collect::<Vec<_>>().join("\n");
    let Some(out) = translate_text(&joined, source, target, cancel) else { return Vec::new(); };
    let lines: Vec<&str> = out.split('\n').collect();
    if lines.len() == items.len() && lines.iter().all(|line| !line.trim().is_empty()) {
        return items.iter().zip(lines).map(|((i, _), t)| (*i, t.trim().to_string())).collect();
    }
    let mut translated = Vec::new();
    for (i, text) in items {
        let Some(out) = translate_text(text, source, target, cancel) else { break; };
        translated.push((*i, out.trim().to_string()));
    }
    translated
}

/// Metinleri karakter sınırına göre gruplar.
fn batches(items: Vec<(u32, String)>) -> Vec<Vec<(u32, String)>> {
    let mut out: Vec<Vec<(u32, String)>> = Vec::new();
    let mut size = 0;
    for item in items {
        let len = item.1.chars().count() + 1;
        if out.is_empty() || size + len > BATCH_CHARS {
            out.push(Vec::new());
            size = 0;
        }
        size += len;
        out.last_mut().unwrap().push(item);
    }
    out
}

impl App {
    /// Sayfa yüklendi: dilini öğren.
    pub(super) fn detect_language(&mut self, i: usize) {
        let tab = &self.tabs[i];
        if url::is_internal(&tab.url) || !(tab.url.starts_with("http://") || tab.url.starts_with("https://")) {
            return;
        }
        if let Some(view) = &tab.view {
            engine::run_script(view, tab.id, &script_for(tab, DETECT));
        }
    }

    /// Sekmeye ait betik sonucu (dil ya da çevrilecek metinler).
    pub(super) fn on_tab_script(&mut self, id: u32, result: &str) {
        let Some(i) = self.index_of(id) else { return };
        let Ok(v) = serde_json::from_str::<Value>(result) else { return };
        if v["k"] == "pip" {
            if i == self.active && v["e"].as_u64() == Some(self.tabs[i].epoch) && v["ok"].as_bool() == Some(false) {
                self.open_surface(super::overlay::Surface::Notice(v["error"].as_str().unwrap_or("Video açılamadı.").into()));
            }
            return;
        }
        if v["k"] == "ocr" {
            if i != self.active || v["e"].as_u64() != Some(self.tabs[i].epoch) || v["request"].as_u64() != Some(self.ocr_request) { return; }
            if let Some(rect) = v["rect"].as_array().filter(|a| a.len() == 4) {
                let mut bounds = [0.0; 4];
                for (j, value) in rect.iter().enumerate() { let Some(n) = value.as_f64() else { return }; bounds[j] = n; }
                if let Some(view) = &self.tabs[i].view { engine::capture_ocr(view, id, self.tabs[i].epoch, self.ocr_request, bounds); }
            }
            return;
        }
        if v["e"].as_u64() != Some(self.tabs[i].epoch) || v["g"].as_u64() != Some(self.tabs[i].tr_generation) { return; }
        match v["k"].as_str() {
            Some("lang") => {
                let lang = v["v"].as_str().unwrap_or("").to_string();
                let language = normalize_language(&lang);
                // Dilini bildirmeyen sayfa: metinde Türkçeye özgü harfler çoksa Türkçe sayılır.
                let target = self.translation_target();
                let unmarked_turkish = language == "auto" && target == "tr" && sample_is_turkish(v["s"].as_str().unwrap_or(""));
                let foreign = !v["no"].as_bool().unwrap_or(false) && language != target && !unmarked_turkish;
                self.tabs[i].page_lang = foreign.then_some(language.clone());
                let st = &self.state.settings;
                let host = url::host(&self.tabs[i].url).to_ascii_lowercase();
                // Ana dil dışındaki sayfa kendiliğinden çevrilir (ayardan kapatılabilir);
                // "asla" listesi, site istisnası ya da kullanıcının bu sayfada kapatması önceliklidir.
                let wanted = st.translation_auto || st.translation_always.contains(&language);
                if foreign && language != "auto" && wanted
                    && !st.translation_never.contains(&language) && !st.translation_sites.contains(&host)
                    && !self.tabs[i].translated && !self.tabs[i].tr_declined && i == self.active {
                    self.start_translation(i);
                }
                self.layout_children();
                self.invalidate();
            }
            Some("tr") => {
                self.tabs[i].tr_collecting = false;
                self.tabs[i].tr_error |= v["error"].as_bool().unwrap_or(false);
                self.tabs[i].tr_limited |= v["limited"].as_bool().unwrap_or(false);
                let items: Vec<(u32, String)> = v["items"]
                    .as_array()
                    .map(|a| a.iter().take(80).filter_map(|it| Some((u32::try_from(it.get(0)?.as_u64()?).ok()?, it.get(1)?.as_str()?.chars().take(16000).collect()))).collect())
                    .unwrap_or_default();
                if items.is_empty() || !self.tabs[i].translated {
                    return;
                }
                let cancel = self.tabs[i].tr_cancel.clone().unwrap_or_default();
                let epoch = self.tabs[i].tr_generation;
                let source = self.tabs[i].page_lang.clone().unwrap_or_else(|| "auto".into());
                let target = self.translation_target();
                let groups = batches(items);
                self.tabs[i].tr_pending += groups.len() as u32;
                self.invalidate();
                std::thread::spawn(move || {
                    crate::net::init_thread();
                    // Bound concurrency; inactive tabs are not polled.
                    let queue = std::sync::Mutex::new(std::collections::VecDeque::from(groups));
                    std::thread::scope(|s| {
                        for _ in 0..2 {
                            s.spawn(|| {
                                crate::net::init_thread();
                                while let Some(group) = queue.lock().ok().and_then(|mut q| q.pop_front()) {
                                    if cancel.load(std::sync::atomic::Ordering::Relaxed) { break; }
                                    let done = translate_batch(&group, &source, &target, &cancel);
                                    let failed = done.len() != group.len();
                                    engine::push_background(engine::Background::Translated(id, epoch, done, failed));
                                }
                            });
                        }
                    });
                });
            }
            _ => {}
        }
    }

    /// Bir grup çeviri geldi: sayfaya yaz.
    pub(super) fn on_translated(&mut self, id: u32, epoch: u64, items: Vec<(u32, String)>, failed: bool) {
        let Some(i) = self.index_of(id) else { return };
        let tab = &mut self.tabs[i];
        if tab.tr_generation != epoch {
            return;
        }
        tab.tr_pending = tab.tr_pending.saturating_sub(1);
        tab.tr_error |= failed || items.is_empty();
        if tab.translated && !items.is_empty() {
            if let Some(view) = &tab.view {
                let data = serde_json::to_string(&items).unwrap_or_default();
                let script = format!(
                    r#"((d)=>{{const T=window.__sqTr;if(!T||T.g!=={epoch})return;for(const[i,t]of d){{const n=T.n[i];if(!n||!n.isConnected||n.nodeValue!==T.o[i])continue;const o=T.o[i];n.nodeValue=o.match(/^\s*/)[0]+t+o.match(/\s*$/)[0];T.a[i]=n.nodeValue;}}}})({data})"#
                );
                engine::execute(view, &script);
            }
        }
        if i == self.active && self.menu.as_ref().is_some_and(|m| m.kind == MenuKind::Translate) {
            // Do not replace an options or language list while it is being used.
            if self.menu.as_ref().is_some_and(|m| m.translation_summary()) {
                let rows = self.translation_items(false);
                if let Some(menu) = &mut self.menu { menu.set_items(rows); }
            }
        }
        self.invalidate();
    }

    fn restore_translation(&mut self, i: usize) {
        let tab = &mut self.tabs[i];
        if let Some(cancel) = tab.tr_cancel.take() { cancel.store(true, std::sync::atomic::Ordering::Relaxed); }
        tab.translated = false; tab.tr_pending = 0; tab.tr_error = false; tab.tr_collecting = false; tab.tr_limited = false;
        tab.tr_generation = tab.tr_generation.wrapping_add(1);
        if let Some(view) = &tab.view { engine::execute(view, RESTORE); }
    }
    fn start_translation(&mut self, i: usize) {
        let tab = &mut self.tabs[i];
        if tab.view.is_none() || url::is_internal(&tab.url) || !(tab.url.starts_with("https://") || tab.url.starts_with("http://")) { return; }
        tab.tr_generation = tab.tr_generation.wrapping_add(1);
        tab.tr_cancel = Some(std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)));
        tab.translated = true; tab.tr_error = false; tab.tr_pending = 0; tab.tr_collecting = true; tab.tr_limited = false;
        if tab.page_lang.is_none() { tab.page_lang = Some("auto".into()); }
        let init = format!("window.__sqTrGeneration={};", tab.tr_generation);
        if let Some(view) = &tab.view {
            engine::execute(view, &init);
            engine::run_script(view, tab.id, &script_for(tab, COLLECT));
        }
        self.sync_translation_timer();
    }
    pub(super) fn toggle_translate(&mut self) {
        if self.tabs[self.active].translated { self.restore_translation(self.active); self.tabs[self.active].tr_declined = true; }
        else { self.start_translation(self.active); }
        self.layout_children(); self.invalidate();
    }
    pub(super) fn apply_translation_rules(&mut self) {
        for i in 0..self.tabs.len() {
            let tab = &self.tabs[i]; let st = &self.state.settings;
            if tab.translated && (tab.page_lang.as_ref().is_some_and(|s| st.translation_never.contains(s)) || st.translation_sites.contains(&url::host(&tab.url).to_ascii_lowercase())) { self.restore_translation(i); }
        }
    }
    pub(super) fn set_translation_target(&mut self, target: String) {
        let was_translated = self.tabs[self.active].translated;
        for i in 0..self.tabs.len() { if self.tabs[i].translated { self.restore_translation(i); } }
        self.state.settings.translation_target = if target.is_empty() { target } else { normalize_language(&target) }; self.dirty = true;
        if was_translated { self.start_translation(self.active); }
        self.notify_pages("settings"); self.layout_children(); self.invalidate();
    }
    pub(super) fn translation_target(&self) -> String {
        let target = normalize_language(&self.state.settings.translation_target);
        if target == "auto" { system_language() } else { target }
    }
    pub(super) fn open_translate(&mut self) {
        if !self.tabs[self.active].translated { self.start_translation(self.active); }
        self.layout_children(); self.invalidate(); self.open_menu(MenuKind::Translate);
    }
    pub(super) fn translation_items(&self, options: bool) -> Vec<menu::Item> {
        use menu::Item;
        let tab = &self.tabs[self.active];
        let source = tab.page_lang.as_deref().unwrap_or("auto");
        let target = self.translation_target();
        let st = &self.state.settings;
        let check = |list: &[String], value: &str| if list.iter().any(|s| s == value) { "✓" } else { "" };
        let mut rows = vec![Item::Translation { source: language_name(source).into(), target: language_name(&target).into(), translated: tab.translated }];
        if options {
            rows.extend([
                Item::act("Başka bir hedef dil seç…", "", MenuCmd::TranslationLanguages(false)),
                Item::act(format!("{} dilini daima çevir", language_name(source)), check(&st.translation_always, source), MenuCmd::TranslationAlways),
                Item::act(format!("{} dilini asla çevirme", language_name(source)), check(&st.translation_never, source), MenuCmd::TranslationNever),
                Item::act("Bu siteyi asla çevirme", check(&st.translation_sites, &url::host(&tab.url).to_ascii_lowercase()), MenuCmd::TranslationSite),
                Item::act("Sayfanın dili farklı…", "", MenuCmd::TranslationLanguages(true)),
                Item::act("Dil ayarlarını aç", "", MenuCmd::TranslationSettings),
            ]);
        } else {
            rows.push(Item::disabled(if tab.tr_error { "Çeviri tamamlanamadı. Yeniden deneyebilirsin." } else if tab.tr_pending > 0 { "Çevriliyor…" } else if tab.tr_limited { "Uzun sayfanın yalnızca bir bölümü çevrildi." } else if tab.translated { "Sayfa çevriliyor; sonradan gelen içerik de çevrilir." } else { "Orijinal sayfa gösteriliyor." }));
            rows.push(Item::act(if tab.tr_error { "Yeniden dene" } else { "Çeviri seçenekleri…" }, "", if tab.tr_error { MenuCmd::Translate } else { MenuCmd::TranslationOptions }));
            rows.push(Item::disabled("Çevrilen sayfa metni Google'a gönderilir."));
        }
        if source == "auto" {
            for row in &mut rows { if let Item::Action { label, cmd, enabled, .. } = row {
                if matches!(cmd, MenuCmd::TranslationAlways | MenuCmd::TranslationNever) { *enabled = false; *label = if *cmd == MenuCmd::TranslationAlways { "Bu dili daima çevir" } else { "Bu dili asla çevirme" }.into(); }
            } }
        }
        rows
    }
    pub(super) fn translation_command(&mut self, cmd: MenuCmd) -> bool {
        use menu::Item;
        let source = self.tabs[self.active].page_lang.clone().unwrap_or_else(|| "auto".into());
        let host = url::host(&self.tabs[self.active].url).to_ascii_lowercase();
        let mut rows = None;
        match cmd {
            MenuCmd::TranslationOptions => rows = Some(self.translation_items(true)),
            MenuCmd::TranslationLanguages(is_source) => {
                let mut items = vec![Item::act("Geri", "", MenuCmd::TranslationOptions)];
                items.push(Item::act(if is_source { "Otomatik algıla" } else { "Bilgisayarın dili" }, "", MenuCmd::TranslationLanguage(is_source, usize::MAX)));
                items.extend(LANGUAGES.iter().enumerate().take(10).map(|(i, (_, name))| Item::act(*name, "", MenuCmd::TranslationLanguage(is_source, i))));
                items.push(Item::act("Diğer diller…", "", MenuCmd::TranslationLanguagePage(is_source, 1)));
                rows = Some(items);
            }
            MenuCmd::TranslationLanguagePage(is_source, page) => {
                let start = page.saturating_mul(10);
                let mut items = vec![Item::act("Geri", "", MenuCmd::TranslationLanguages(is_source))];
                items.extend(LANGUAGES.iter().enumerate().skip(start).take(10).map(|(i, (_, name))| Item::act(*name, "", MenuCmd::TranslationLanguage(is_source, i))));
                if start + 10 < LANGUAGES.len() { items.push(Item::act("Diğer diller…", "", MenuCmd::TranslationLanguagePage(is_source, page + 1))); }
                rows = Some(items);
            }
            MenuCmd::TranslationLanguage(is_source, index) => {
                let value = if index == usize::MAX { "" } else if let Some((code, _)) = LANGUAGES.get(index) { code } else { return true; };
                self.restore_translation(self.active);
                if is_source { self.tabs[self.active].page_lang = Some(if value.is_empty() { "auto" } else { value }.into()); }
                else { self.set_translation_target(value.into()); }
                self.start_translation(self.active); rows = Some(self.translation_items(false));
            }
            MenuCmd::TranslationAlways | MenuCmd::TranslationNever | MenuCmd::TranslationSite => {
                let st = &mut self.state.settings;
                match cmd {
                    MenuCmd::TranslationAlways if source != "auto" => { preference_toggle(&mut st.translation_always, &source); st.translation_never.retain(|s| s != &source); }
                    MenuCmd::TranslationNever if source != "auto" => { preference_toggle(&mut st.translation_never, &source); st.translation_always.retain(|s| s != &source); }
                    MenuCmd::TranslationSite if !host.is_empty() => preference_toggle(&mut st.translation_sites, &host),
                    _ => {},
                }
                let blocked = st.translation_never.contains(&source) || st.translation_sites.contains(&host);
                self.dirty = true;
                if blocked { self.restore_translation(self.active); }
                else if !self.tabs[self.active].translated { self.start_translation(self.active); }
                self.notify_pages("settings"); rows = Some(self.translation_items(true));
            }
            MenuCmd::TranslationSettings => { self.close_menu(); self.new_tab(format!("{}#languages", url::internal("settings")), None); }
            MenuCmd::TranslationClose => { self.close_menu(); self.focus_content(); }
            MenuCmd::TranslationOriginal => { self.restore_translation(self.active); self.tabs[self.active].tr_declined = true; rows = Some(self.translation_items(false)); }
            MenuCmd::Translate if self.menu.as_ref().is_some_and(|m| m.kind == MenuKind::Translate) => {
                if self.tabs[self.active].tr_error { self.restore_translation(self.active); }
                if !self.tabs[self.active].translated { self.start_translation(self.active); }
                rows = Some(self.translation_items(false));
            }
            _ => return false,
        }
        if let (Some(rows), Some(menu)) = (rows, &mut self.menu) { menu.set_items(rows); }
        self.layout_children(); self.invalidate(); true
    }

    /// Çevrilmiş sekmede sonradan gelen içeriği de topla.
    pub(super) fn translate_tick(&mut self) {
        if !self.translation_polling_needed() { self.sync_translation_timer(); return; }
        if let Some(tab) = self.tabs.get_mut(self.active) {
            if tab.translated && tab.tr_pending == 0 && !tab.tr_collecting && !tab.tr_error && tab.mem == Mem::Awake && !tab.loading {
                if let Some(view) = &tab.view { tab.tr_collecting = true; engine::run_script(view, tab.id, &script_for(tab, COLLECT)); }
            }
        }
    }

    fn translation_polling_needed(&self) -> bool {
        self.app_active && !unsafe { IsIconic(self.hwnd) }.as_bool() && self.tabs.get(self.active).is_some_and(|t| t.translated && !t.tr_error && t.mem==Mem::Awake)
    }

    pub(super) fn sync_translation_timer(&self) {
        unsafe {
            if self.translation_polling_needed() { SetTimer(Some(self.hwnd),TIMER_TRANSLATE,if self.energy_saver {3000}else{1500},None); }
            else { let _=KillTimer(Some(self.hwnd),TIMER_TRANSLATE); }
        }
    }

    pub(super) fn translate_visible(&self) -> bool {
        self.tabs.get(self.active).is_some_and(|t| t.page_lang.is_some() || t.translated)
    }

    pub(super) fn paint_translate(&self, r: Rect) {
        let g = &self.gfx;
        let tab = &self.tabs[self.active];
        if tab.translated {
            g.round_alpha(r, 7.0, system_blue(), 0.22);
        }
        self.field_button(r, Hit::Translate);
        if tab.tr_pending > 0 {
            let turn = self.t0.elapsed().as_secs_f32() / 0.9;
            g.spinner(r.cx(), r.cy(), 6.0, turn.fract(), system_blue(), 1.8);
        } else {
            let color = if tab.translated { system_blue() } else if self.hover == Hit::Translate { ink() } else { muted() };
            g.text("文A", r, Font::Small, color, Align::Center);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "live public Google translation endpoint"]
    fn canli_ceviri() {
        crate::net::init_thread();
        let cancel = std::sync::atomic::AtomicBool::new(false);
        let translated = translate_text("Hello world", "en", "tr", &cancel).expect("live translation");
        assert!(!translated.trim().is_empty() && translated != "Hello world");
    }
    #[test]
    fn locale_and_preferences() {
        assert_eq!(normalize_language("tr-TR"), "tr");
        assert_eq!(normalize_language("EN_us"), "en");
        assert_eq!(normalize_language("zh-Hant-HK"), "zh-TW");
        assert_eq!(normalize_language("bad&tl=xx"), "auto");
        let canceled = std::sync::atomic::AtomicBool::new(true);
        assert!(translate_text("do not send", "en", "tr", &canceled).is_none());
        assert!(parse_translation(b"[[]]").is_none());
        assert!(sample_is_turkish("Şu an çalışıyoruz, öğrenciler için güzel bir günü birlikte geçireceğiz"));
        assert!(!sample_is_turkish("The quick brown fox jumps over the lazy dog and keeps running far away"));
        let mut values = Vec::new(); preference_toggle(&mut values, "en");
        assert_eq!(values, vec!["en"]); preference_toggle(&mut values, "en"); assert!(values.is_empty());
    }

    #[test]
    fn yanit_ve_gruplar() {
        let body = br#"[[["Merhaba ","Hello ",null],["dunya","world",null]],null,"en"]"#;
        assert_eq!(parse_translation(body).as_deref(), Some("Merhaba dunya"));
        let items: Vec<(u32, String)> = (0..10).map(|i| (i, "x".repeat(500))).collect();
        let b = batches(items);
        assert!(b.iter().all(|g| g.iter().map(|(_, t)| t.len() + 1).sum::<usize>() <= BATCH_CHARS || g.len() == 1));
        assert_eq!(b.iter().map(Vec::len).sum::<usize>(), 10);
    }
}
