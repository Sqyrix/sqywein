//! İç sayfalar (geçmiş, indirmeler, yer işaretleri, ayarlar, uzantılar) ile
//! mesajlaşma ve indirmelerin takibi.
//!
//! Sayfalar `{type, ...}` biçiminde JSON gönderir; yanıt aynı `type` ile döner.

use super::*;
use serde_json::{json, Value};
use windows::Win32::UI::Shell::ShellExecuteW;

/// Sayfadaki açık iç sayfa adı: "https://sqywein.ui/history.html" → "history".
fn page_of(url: &str) -> Option<&str> {
    let rest = url.strip_prefix(url::UI_ORIGIN)?;
    Some(rest.split(['?', '#']).next().unwrap_or(rest).trim_end_matches(".html"))
}

fn shell_open(target: &str, args: Option<&str>) {
    let (file, params) = (wide(target), args.map(wide));
    unsafe {
        ShellExecuteW(None, w!("open"), &file, params.as_ref().map_or(PCWSTR::null(), |p| PCWSTR(p.as_ptr())), PCWSTR::null(), SW_SHOWNORMAL);
    }
}

impl App {
    pub(super) fn on_web_message(&mut self, tab_id: u32, json: &str) {
        let Ok(msg) = serde_json::from_str::<Value>(json) else { return };
        if msg["type"].as_str().is_some_and(|s| s.starts_with("overlay.")) { self.surface_message(tab_id, &msg); return; }
        let kind = msg["type"].as_str().unwrap_or("").to_string();
        let s = |k: &str| msg[k].as_str().unwrap_or("").to_string();
        let reply = match kind.as_str() {
            "browser.action" => {
                match msg["action"].as_str().unwrap_or("") {
                    "spotlight" => self.open_surface(super::overlay::Surface::Spotlight),
                    "tabs" => self.open_surface(super::overlay::Surface::Tabs),
                    _ => {},
                }
                None
            }
            "open" => {
                let target = s("url");
                if let Some(u) = url::to_url_with(&target, self.state.settings.search_url()) {
                    if msg["newTab"].as_bool().unwrap_or(false) {
                        self.new_tab(u, None);
                    } else if let Some(i) = self.index_of(tab_id) {
                        self.activate(i);
                        self.navigate(&u);
                    }
                }
                None
            }
            k if k.starts_with("passwords.") => self.on_passwords_page(tab_id, k, &msg),
            // Geçmiş
            "history.list" => Some(self.history_json(&s("query"), msg["limit"].as_u64().unwrap_or(500) as usize)),
            "history.remove" => {
                self.history.remove(&s("url"));
                Some(self.history_json("", 500))
            }
            "history.clear" => {
                self.history.clear();
                if let Some(view) = self.any_view() {
                    engine::clear_data(&view, &["history".into()], self.state.current_profile.clone(), None);
                }
                Some(self.history_json("", 500))
            }
            // Yer işaretleri
            "permissions.list" => { if let Some(v) = self.any_view() { engine::list_permissions(&v, tab_id); } None }
            "permissions.set" => {
                let origin = s("origin"); let kind = msg["kind"].as_i64().unwrap_or(-1) as i32; let state = msg["state"].as_i64().unwrap_or(-1) as i32;
                if (origin.starts_with("https://") || origin.starts_with("http://")) && [1,2,3,4,6].contains(&kind) && (0..=2).contains(&state) {
                    if let Some(v) = self.any_view() { engine::set_permission(&v, tab_id, &origin, kind, state); }
                }
                None
            }
            "system.defaultBrowser" => {
                match crate::integration::open_default_settings() {
                    Ok(()) => Some(json!({"type":"system.result","ok":true})),
                    Err(e) => Some(json!({"type":"system.result","ok":false,"error":e.to_string()})),
                }
            }
            "settings.reset" => {
                self.state.settings = state::Settings::default();
                set_accent(&self.state.settings.accent);
                set_motion(self.state.settings.animations);
                set_theme(&self.state.settings.theme);
                self.refresh_theme();
                self.set_adblock_enabled(self.state.settings.adblock);
                self.apply_adblock_levels();
                self.dirty = true;
                self.layout_children();
                Some(self.settings_json())
            }
            "bookmarks.import" => {
                let mut added = 0;
                if let Some(items) = msg["items"].as_array() {
                    for item in items.iter().take(5000) {
                        let Some(u) = item["url"].as_str().filter(|s| s.len() <= 8192 && (s.starts_with("https://") || s.starts_with("http://"))) else { continue };
                        if self.state.favorites.iter().any(|b| b.url == u) { continue; }
                        let title = item["title"].as_str().unwrap_or(u).chars().take(500).collect();
                        self.state.favorites.push(state::Bookmark { title, url: u.to_string() }); added += 1;
                    }
                }
                self.dirty = true; self.invalidate(); self.notify_pages("bookmarks");
                Some(json!({"type":"bookmarks.import","added":added}))
            }
            "bookmarks.list" => Some(self.bookmarks_json()),
            "bookmarks.remove" => {
                let u = s("url");
                self.state.favorites.retain(|b| b.url != u);
                self.dirty = true;
                self.invalidate();
                Some(self.bookmarks_json())
            }
            "bookmarks.rename" => {
                let (u, t) = (s("url"), s("title"));
                if let Some(b) = self.state.favorites.iter_mut().find(|b| b.url == u) {
                    b.title = t;
                    self.dirty = true;
                }
                Some(self.bookmarks_json())
            }
            "bookmarks.move" => {
                let (from, to) = (msg["from"].as_u64().unwrap_or(0) as usize, msg["to"].as_u64().unwrap_or(0) as usize);
                let n = self.state.favorites.len();
                if from < n && to < n {
                    let b = self.state.favorites.remove(from);
                    self.state.favorites.insert(to, b);
                    self.dirty = true;
                }
                Some(self.bookmarks_json())
            }
            // İndirmeler
            "downloads.preview" => { if let Some(id) = msg["id"].as_u64().and_then(|id| u32::try_from(id).ok()) { self.open_surface(super::overlay::Surface::Download(id)); } None }
            "downloads.list" => Some(self.downloads_json()),
            "downloads.open" | "downloads.show" | "downloads.cancel" | "downloads.pause" | "downloads.resume" | "downloads.remove" => {
                let id = msg["id"].as_u64().unwrap_or(0) as u32;
                self.download_action(&kind, id);
                Some(self.downloads_json())
            }
            "downloads.clear" => {
                self.state.download_list.retain(|d| d.status == "progress" || d.status == "paused");
                self.dirty = true;
                Some(self.downloads_json())
            }
            "downloads.openFolder" => {
                let folder = self
                    .any_view()
                    .and_then(|v| engine::profile(&v))
                    .map(|p| engine::string(|out| unsafe { p.DefaultDownloadFolderPath(out) }))
                    .unwrap_or_default();
                if !folder.is_empty() {
                    shell_open(&folder, None);
                }
                None
            }
            // Uzantılar
            "extensions.list" => {
                self.refresh_extensions();
                Some(self.extensions_json())
            }
            "extensions.update" => { self.update_extension(&s("id")); None }
            "extensions.toggle" => {
                self.enable_extension(&s("id"), msg["enabled"].as_bool().unwrap_or(true));
                None
            }
            "extensions.remove" => {
                self.remove_extension(&s("id"));
                None
            }
            "extensions.pin" => {
                self.set_pinned(&s("id"), msg["pinned"].as_bool().unwrap_or(false));
                Some(self.extensions_json())
            }
            "extensions.loadUnpacked" => {
                self.load_unpacked();
                None
            }
            // Ayarlar
            "sq.rest.result" => {
                if let (Some(epoch), Some(discard), Some(safe)) = (msg["epoch"].as_u64(), msg["discard"].as_bool(), msg["safe"].as_bool()) { engine::push(Event::SleepChecked(tab_id, epoch, discard, safe)); }
                None
            }
            "motion.get" => Some(json!({"type":"motion","enabled":motion_enabled()})),
            "theme.get" => Some(crate::theme::message()),
            "settings.get" => Some(self.settings_json()),
            "update.check" => {
                self.check_updates();
                None
            }
            "update.apply" => {
                self.apply_update();
                None
            }
            "adblock.update" => {
                self.update_adblock_lists();
                None
            }
            "settings.set" => {
                if s("key") == "downloadFolder" && !msg["value"].as_str().unwrap_or("").is_empty() && !std::path::Path::new(msg["value"].as_str().unwrap_or("")).is_dir() {
                    let reply = json!({"type":"settings.error","error":"Klasör bulunamadı. Var olan bir klasörün tam yolunu yaz."});
                    if let Some(v) = self.index_of(tab_id).and_then(|i| self.tabs[i].view.as_ref()) { engine::post_json(v, &reply.to_string()); }
                    return;
                }
                self.set_setting(&s("key"), &msg["value"]);
                None
            }
            "data.clear" => {
                let kinds: Vec<String> = msg["kinds"].as_array().map(|a| a.iter().filter_map(|k| k.as_str().map(str::to_string)).collect()).unwrap_or_default();
                if kinds.iter().any(|k| k == "history") {
                    self.history.clear();
                }
                if let Some(view) = self.any_view() {
                    if kinds.iter().any(|k| k == "cookies") { self.clear_saved_cookies(); }
                    self.clearing_data.insert(self.state.current_profile.clone());
                    engine::clear_data(&view, &kinds, self.state.current_profile.clone(), Some(tab_id));
                }
                None
            }
            "profiles.switch" => {
                if self.installing {
                    if let Some(v) = self.index_of(tab_id).and_then(|i| self.tabs[i].view.as_ref()) { engine::post_json(v, &json!({"type":"settings.error","error":"Uzantı kurulumu bitince profil değiştirebilirsin."}).to_string()); }
                    return;
                }
                if let Some(i) = self.state.profiles.iter().position(|p| p.id == s("id")) {
                    self.switch_profile(i);
                }
                None
            }
            "profiles.rename" => {
                self.rename_profile(&s("id"), &s("name"));
                Some(self.settings_json())
            }
            "profiles.remove" => {
                self.remove_profile(&s("id"));
                Some(self.settings_json())
            }
            "profiles.add" => {
                self.add_profile(Some(s("name")));
                None
            }
            _ => None,
        };
        if let (Some(reply), Some(view)) = (reply, (if tab_id == POPUP_VIEW { self.surface_view() } else { self.index_of(tab_id).and_then(|i| self.tabs[i].view.clone()) })) {
            engine::post_json(&view, &reply.to_string());
        }
    }

    /// Açık iç sayfaları tazeler (ör. yıldıza basınca yer işaretleri sayfası).
    pub(super) fn notify_pages(&self, page: &str) {
        let data = match page {
            "bookmarks" => self.bookmarks_json(),
            "settings" => self.settings_json(),
            "extensions" => self.extensions_json(),
            "downloads" => self.downloads_json(),
            _ => return,
        };
        for t in &self.tabs {
            if page_of(&t.url) == Some(page) {
                if let Some(v) = &t.view {
                    engine::post_json(v, &data.to_string());
                }
            }
        }
    }

    fn history_json(&self, query: &str, limit: usize) -> Value {
        let items: Vec<Value> = self
            .history
            .recent(query, limit)
            .into_iter()
            .map(|e| json!({ "url": e.url, "title": e.title, "when": e.last_visit, "visits": e.visits }))
            .collect();
        json!({ "type": "history.list", "items": items })
    }

    fn bookmarks_json(&self) -> Value {
        let items: Vec<Value> = self.state.favorites.iter().map(|b| json!({ "title": b.title, "url": b.url })).collect();
        json!({ "type": "bookmarks.list", "items": items })
    }

    fn settings_json(&self) -> Value {
        let engine = self.env.as_ref().map(|e| engine::string(|p| unsafe { e.BrowserVersionString(p) })).unwrap_or_default();
        json!({
            "type": "settings.get",
            "settings": self.state.settings,
            "defaultBrowser": crate::integration::is_default_browser(),
            "translationLanguages": super::translate::LANGUAGES,
            "systemLanguage": super::translate::system_language(),
            "recoveredSession":self.recovered_session,
            "energySaver":self.energy_saver,
            "motion": motion_enabled(),
            "profiles": self.profiles_json(),
            "about": { "version": env!("CARGO_PKG_VERSION"), "engine": format!("WebView2 {engine}") },
            "adblock": { "updated": crate::adblock::updated_label() },
            "update": self.update_json(),
        })
    }

    fn set_setting(&mut self, key: &str, value: &Value) {
        let st = &mut self.state.settings;
        let b = value.as_bool().unwrap_or(false);
        let text = value.as_str().unwrap_or("").to_string();
        match key {
            "translationAuto" => st.translation_auto = b,
            "showWidgets" => { st.show_widgets = b; self.invalidate(); }
            "weatherCity" => { st.weather_city = text.chars().take(80).collect(); self.state.widgets.location = None; self.state.widgets.weather = None; self.weather = None; self.weather_retry = 0; self.refresh_widgets(); }
            "translationTarget" => {
                if !text.is_empty() && super::translate::normalize_language(&text) == "auto" { return; }
                self.set_translation_target(text); return;
            }
            "translationAlways" | "translationNever" => {
                let values: Vec<String> = text.split(',').map(str::trim).map(super::translate::normalize_language).filter(|s| s != "auto").take(200).collect();
                if key == "translationAlways" { st.translation_never.retain(|s| !values.contains(s)); st.translation_always = values; }
                else { st.translation_always.retain(|s| !values.contains(s)); st.translation_never = values; }
            }
            "translationSites" => st.translation_sites = text.split(',').map(str::trim).filter(|s| !s.is_empty()).map(|s| url::host(s).to_ascii_lowercase()).take(200).collect(),
            "neverSleep" => st.never_sleep = text.split(',').map(str::trim).filter(|s| !s.is_empty()).map(|s| url::host(s).to_ascii_lowercase()).take(200).collect(),
            "downloadFolder" => {
                if !text.is_empty() && !std::path::Path::new(&text).is_dir() { return; }
                st.download_folder = text.clone();
                for v in self.tabs.iter().filter_map(|t| t.view.as_ref()) {
                    if text.is_empty() { engine::set_download_folder(v); } else if let Some(p) = engine::profile(v) { unsafe { let _ = p.SetDefaultDownloadFolderPath(&HSTRING::from(&text)); } }
                }
            }
            "searchEngine" if ["google", "bing", "duckduckgo", "yandex", "brave"].contains(&text.as_str()) => st.search_engine = text,
            "startup" => st.startup = text,
            "animations" => {
                st.animations = b; set_motion(b); self.animate();
                let json = json!({"type":"motion","enabled":motion_enabled()}).to_string();
                for t in &self.tabs { if url::is_internal(&t.url) { if let Some(v) = &t.view { engine::post_json(v, &json); } } }
            },
            "freezeTabs" => st.freeze_tabs = b,
            "glass" => { st.glass = b; self.update_glass(); }
            "theme" if ["light","dark","sqywein","system"].contains(&text.as_str()) => {
                st.theme = text; set_theme(&st.theme); self.refresh_theme();
            }
            "sleepTabs" => st.sleep_tabs = b,
            "hardwareAcceleration" => st.hardware_acceleration = b, // Yeniden başlatınca geçerli.
            "searchSuggestions" => st.search_suggestions = b,
            "autoUpdate" => st.auto_update = b,
            "accent" => {
                st.accent = "blue".into();
                set_accent(&st.accent);
                self.refresh_theme();
            }
            "adblock" => {
                self.set_adblock_enabled(b);
                return;
            }
            "adblockLevel" => {
                st.adblock_level = value.as_u64().or_else(|| text.parse().ok()).unwrap_or(2).min(3) as u8;
                self.apply_adblock_levels();
                self.dirty = true;
                return;
            }
            "showBookmarksBar" => {
                st.show_bookmarks_bar = b;
                self.layout_children();
            }
            "privacyShield" => {
                // Çerez kuralı yeniden başlatınca, izleyici engeli hemen geçerli olur.
                st.privacy_shield = b;
                let level = if b { "strict".to_string() } else { st.tracking.clone() };
                if let Some(v) = self.any_view() {
                    engine::apply_tracking(&v, &level);
                }
            }
            "defaultZoom" => {
                st.default_zoom = value.as_u64().or_else(|| text.parse().ok()).unwrap_or(0) as u32;
                // Kendi yakınlaştırması kayıtlı olmayan sekmeler yeni varsayılana geçer.
                for i in 0..self.tabs.len() {
                    self.tabs[i].zoom_host.clear();
                    self.apply_zoom(i);
                }
            }
            "savePasswords" | "autofill" => {
                if key == "savePasswords" {
                    st.save_passwords = b;
                } else {
                    st.autofill = b;
                }
                let af = st.autofill;
                for v in self.tabs.iter().filter_map(|t| t.view.as_ref()) {
                    if let Ok(s4) = unsafe { v.Settings() }.and_then(|s| s.cast::<ICoreWebView2Settings4>()) {
                        unsafe {
                            let _ = s4.SetIsPasswordAutosaveEnabled(false);
                            let _ = s4.SetIsGeneralAutofillEnabled(af);
                        }
                    }
                }
            }
            "tracking" => {
                st.tracking = text.clone();
                if let Some(v) = self.any_view() {
                    engine::apply_tracking(&v, &text);
                }
            }
            _ => return,
        }
        if key.starts_with("translation") { self.apply_translation_rules(); }
        self.dirty = true;
        self.invalidate();
    }

    // ---------- İndirmeler ----------

    pub(super) fn download_record(&self, id: u32) -> Option<&state::DownloadRecord> {
        self.state.download_list.iter().find(|d| d.id == id)
    }

    /// Süren indirmelerin toplam ilerlemesi (0..1); süren yoksa `None`.
    pub(super) fn download_progress(&self) -> Option<f32> {
        let active: Vec<_> = self.state.download_list.iter().filter(|d| d.status == "progress").collect();
        if active.is_empty() {
            return None;
        }
        let (got, total): (u64, u64) = active.iter().fold((0, 0), |(g, t), d| (g + d.received, t + d.total));
        Some(if total > 0 { got as f32 / total as f32 } else { 0.0 })
    }

    pub(super) fn on_download_started(&mut self, tab: u32, op: ICoreWebView2DownloadOperation) {
        let id = self.next_download;
        self.download_owner.insert(id, tab);
        self.next_download += 1;
        let path = engine::string(|p| unsafe { op.ResultFilePath(p) });
        let link = engine::string(|p| unsafe { op.Uri(p) });
        let name = std::path::Path::new(&path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "indirme".into());
        let (status, received, total) = engine::download_progress(&op);
        self.state.download_list.insert(
            0,
            state::DownloadRecord { private: self.index_of(tab).is_some_and(|i| self.tabs[i].private), id, name, path, url: link, status: status.into(), received, total, when: crate::history::now_iso() },
        );
        self.state.download_list.truncate(200);
        engine::watch_download(&op, id);
        self.downloads.push((id, op));
        self.show_downloads = true;
        self.dirty = true;
        self.dl_speed.insert(id, (received, Instant::now(), 0.0));
        self.layout_children();
        self.animate();
        self.push_download(id);
        crate::log::line(&format!("İNDİRME #{tab} {}", self.download_record(id).map(|d| d.url.clone()).unwrap_or_default()));
        // Yalnızca indirme için açılan boş sekme kapanır (görünümü indirme bitene kadar yaşar).
        self.close_download_only_tab(tab);
        // Chrome gibi: indirme başlayınca baloncuk kendiliğinden açılır.
        self.open_menu(MenuKind::Downloads);
        self.invalidate();
    }

    /// İndirilen dosyanın simgesi: o türü açan programın simgesi. Programın kendi simgesi
    /// olan türler (.exe, .msi, .lnk) dosyanın kendisinden, diğerleri uzantıdan alınır.
    pub(super) fn file_icon(&self, d: &state::DownloadRecord) -> Option<(Rc<Image>, String)> {
        let path = std::path::Path::new(&d.path);
        let ext = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
        let exists = d.status == "done" && path.exists();
        let own_icon = matches!(ext.as_str(), "exe" | "msi" | "lnk" | "ico");
        let key = if own_icon && exists { d.path.clone() } else { format!(".{ext}") };
        if let Some(hit) = self.file_icons.borrow_mut().get(&key) {
            return hit.clone();
        }
        let source = if own_icon && exists { d.path.clone() } else { format!("dosya.{ext}") };
        let icon = crate::image::file_icon(&source, own_icon && exists).map(|img| {
            let url = crate::image::to_data_url(&img);
            (Rc::new(img), url)
        });
        // Süren bir .exe indirmesinin geçici simgesi saklanmaz; bitince gerçeği alınır.
        if exists || !own_icon {
            self.file_icons.borrow_mut().insert(key, icon.clone());
        }
        icon
    }

    /// Baloncuktaki son indirmeler.
    pub(super) fn download_items(&self) -> Vec<Item> {
        let mut items: Vec<Item> = self
            .state
            .download_list
            .iter()
            .take(5)
            .map(|d| {
                let speed = self.dl_speed.get(&d.id).map_or(0.0, |s| s.2);
                Item::Download(menu::DownloadRow {
                    id: d.id,
                    name: d.name.clone(),
                    line: download_line(d, speed),
                    progress: (d.status == "progress").then(|| if d.total > 0 { d.received as f32 / d.total as f32 } else { 0.0 }),
                    done: d.status == "done",
                    icon: self.file_icon(d).map(|(img, _)| img),
                })
            })
            .collect();
        if items.is_empty() {
            items.push(Item::disabled("Henüz indirme yok"));
        }
        items.push(Item::Sep);
        items.push(Item::act("Tüm indirmeler", "Ctrl+J", MenuCmd::Downloads));
        items
    }

    pub(super) fn on_download_changed(&mut self, id: u32) {
        let Some(op) = self.downloads.iter().find(|(d, _)| *d == id).map(|(_, op)| op.clone()) else { return };
        let (status, received, total) = engine::download_progress(&op);
        // Hız: ani dalgalanmalar yumuşatılır.
        let now = Instant::now();
        let entry = self.dl_speed.entry(id).or_insert((received, now, 0.0));
        let dt = now.duration_since(entry.1).as_secs_f64();
        if dt >= 0.4 {
            let inst = received.saturating_sub(entry.0) as f64 / dt;
            entry.2 = if entry.2 == 0.0 { inst } else { entry.2 * 0.7 + inst * 0.3 };
            entry.0 = received;
            entry.1 = now;
        }
        let was = self.download_record(id).map(|d| d.status.clone()).unwrap_or_default();
        if let Some(rec) = self.state.download_list.iter_mut().find(|d| d.id == id) {
            rec.status = status.into();
            rec.received = received;
            rec.total = total;
        }
        if matches!(status, "done" | "failed" | "canceled") {
            self.downloads.retain(|(d, _)| *d != id);
            self.release_lingering();
            self.dirty = true;
            if status == "done" && was != "done" {
                self.download_pulse = Some(Instant::now());
                self.animate();
            }
        }
        if self.menu.as_ref().is_some_and(|m| m.kind == MenuKind::Downloads) {
            let items = self.download_items();
            if let Some(m) = &mut self.menu {
                m.set_items(items);
            }
        }
        self.push_download(id);
        self.invalidate();
    }

    fn push_download(&self, id: u32) {
        let Some(rec) = self.download_record(id) else { return };
        let msg = json!({ "type": "downloads.update", "item": self.download_json_item(rec) });
        for t in &self.tabs {
            if page_of(&t.url) == Some("downloads") {
                if let Some(v) = &t.view {
                    engine::post_json(v, &msg.to_string());
                }
            }
        }
    }

    fn downloads_json(&self) -> Value {
        let items: Vec<Value> = self.state.download_list.iter().map(|d| self.download_json_item(d)).collect();
        json!({ "type": "downloads.list", "items": items })
    }

    pub(super) fn download_action(&mut self, kind: &str, id: u32) {
        let op = self.downloads.iter().find(|(d, _)| *d == id).map(|(_, op)| op.clone());
        let path = self.download_record(id).map(|d| d.path.clone()).unwrap_or_default();
        unsafe {
            match kind {
                "downloads.open" => shell_open(&path, None),
                "downloads.show" => shell_open("explorer.exe", Some(&format!("/select,\"{path}\""))),
                "downloads.cancel" => {
                    if let Some(op) = op {
                        let _ = op.Cancel();
                    }
                }
                "downloads.pause" => {
                    if let Some(op) = op {
                        let _ = op.Pause();
                    }
                }
                "downloads.resume" => {
                    if let Some(op) = op {
                        let _ = op.Resume();
                    }
                }
                "downloads.remove" => {
                    self.state.download_list.retain(|d| d.id != id);
                    self.dirty = true;
                }
                _ => {}
            }
        }
    }
}

fn size(bytes: u64) -> String {
    let b = bytes as f64;
    if b >= 1_073_741_824.0 {
        format!("{:.1} GB", b / 1_073_741_824.0)
    } else if b >= 1_048_576.0 {
        format!("{:.1} MB", b / 1_048_576.0)
    } else {
        format!("{:.0} KB", (b / 1024.0).max(1.0))
    }
    .replace('.', ",")
}

fn duration(secs: f64) -> String {
    let s = secs.round() as u64;
    match s {
        0..=59 => format!("{s} sn kaldı"),
        60..=3599 => format!("{} dk kaldı", s.div_ceil(60)),
        _ => format!("{} sa {} dk kaldı", s / 3600, (s % 3600) / 60),
    }
}

/// Baloncuktaki durum satırı.
fn download_line(d: &state::DownloadRecord, speed: f64) -> String {
    match d.status.as_str() {
        "done" => format!("{} · Tamamlandı", size(d.total.max(d.received))),
        "failed" => "İndirilemedi".into(),
        "canceled" => "İptal edildi".into(),
        "paused" => format!("Duraklatıldı · {} / {}", size(d.received), size(d.total)),
        _ => {
            let mut line = if d.total > 0 { format!("{} / {}", size(d.received), size(d.total)) } else { size(d.received) };
            if speed > 1.0 {
                line.push_str(&format!(" · {}/sn", size(speed as u64)));
                if d.total > d.received {
                    line.push_str(&format!(" · {}", duration((d.total - d.received) as f64 / speed)));
                }
            }
            line
        }
    }
}

impl App {
    fn download_json_item(&self, d: &state::DownloadRecord) -> Value {
        let icon = self.file_icon(d).map(|(_, url)| url).unwrap_or_default();
        json!({
            "id": d.id, "name": d.name, "path": d.path, "url": d.url, "status": d.status,
            "received": d.received, "total": d.total, "when": d.when, "icon": icon,
        })
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn indirme_satiri() {
        let mut d = crate::state::DownloadRecord { status: "progress".into(), received: 2 * 1_048_576, total: 10 * 1_048_576, ..Default::default() };
        assert_eq!(super::download_line(&d, 1_048_576.0), "2,0 MB / 10,0 MB · 1,0 MB/sn · 8 sn kaldı");
        d.status = "done".into();
        assert_eq!(super::download_line(&d, 0.0), "10,0 MB · Tamamlandı");
    }

    #[test]
    fn sayfa_adi() {
        assert_eq!(super::page_of("https://sqywein.ui/history.html"), Some("history"));
        assert_eq!(super::page_of("https://sqywein.ui/settings.html#profiles"), Some("settings"));
        assert_eq!(super::page_of("https://example.com/"), None);
    }
}
