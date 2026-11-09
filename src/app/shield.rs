//! Reklam engelleyicinin uygulama tarafı: motorun yüklenmesi, sayfa başına
//! kozmetik kurallar, YouTube modülü, adres çubuğundaki kalkan ve site başına
//! filtreleme modu (Kapalı / Temel / İdeal / Tam).

use super::*;
use crate::adblock;
use std::collections::HashSet;

/// Belge betik yuvaları: 0 = sayfaya özel gizleme, 1 = YouTube modülü.
const SLOT_PAGE: u8 = 0;
const SLOT_YOUTUBE: u8 = 1;

impl App {
    /// Açılışta: kayıtlı motor hemen, eskiyse yenisi arka planda derlenir.
    pub(super) fn start_adblock(&mut self) {
        // Eski sürümdeki "bu sitede kapalı" listesi "Kapalı" moduna taşınır.
        for host in std::mem::take(&mut self.state.adblock_allow) {
            self.state.adblock_levels.insert(host, adblock::OFF);
            self.dirty = true;
        }
        adblock::set_enabled(self.state.settings.adblock);
        self.apply_adblock_levels();
        if !self.state.settings.adblock || self.adblock_loading || adblock::is_ready() { return; }
        self.adblock_loading = true;
        std::thread::spawn(|| {
            let cached = adblock::load_cached();
            let rebuild = cached.is_none() || adblock::needs_update();
            if let Some(engines) = cached {
                engine::push_background(engine::Background::Adblock(Some(Box::new(engines)), false, !rebuild));
            }
            if rebuild {
                crate::net::init_thread();
                let fresh = adblock::build().map(Box::new);
                engine::push_background(engine::Background::Adblock(fresh, true, true));
            }
        });
    }

    /// Ayarlardaki "Şimdi güncelle".
    pub(super) fn update_adblock_lists(&mut self) {
        if self.adblock_loading { return; }
        self.adblock_loading = true;
        std::thread::spawn(|| {
            crate::net::init_thread();
            let fresh = adblock::build().map(Box::new);
            engine::push_background(engine::Background::Adblock(fresh, true, true));
        });
    }

    pub(super) fn on_adblock_engine(&mut self, engines: Option<Box<(adblock::Engine, Option<adblock::Engine>)>>, fresh: bool, done: bool) {
        if done { self.adblock_loading = false; }
        let ok = engines.is_some();
        if let Some(e) = engines.filter(|_| self.state.settings.adblock) {
            let (main, annoy) = *e;
            adblock::install(main, annoy);
            for i in 0..self.tabs.len() {
                if self.tabs[i].mem == Mem::Awake {
                    if let Some(view) = &self.tabs[i].view { engine::execute(view, "window.__sqCollect?.(false);"); }
                    self.apply_page_cosmetics(i);
                }
            }
        }
        if fresh {
            let msg = serde_json::json!({ "type": "adblock.updated", "ok": ok });
            for t in &self.tabs {
                if t.url == url::internal("settings") {
                    if let Some(v) = &t.view {
                        engine::post_json(v, &msg.to_string());
                    }
                }
            }
        }
        self.invalidate();
    }

    /// Modları motora bildirir ve YouTube modülünü yeniler.
    pub(super) fn apply_adblock_levels(&mut self) {
        let sites = self.state.adblock_levels.iter().map(|(h, l)| (h.clone(), *l));
        adblock::set_levels(self.state.settings.adblock_level, sites);
        for tab in &self.tabs {
            if let Some(view) = &tab.view {
                let _ = engine::set_network_interception(view, adblock::level_for(&tab.url) > adblock::OFF);
            }
        }
        self.refresh_youtube_scripts();
    }

    fn youtube_script(&self) -> String {
        let (mut off, mut on) = (Vec::new(), Vec::new());
        for (host, level) in &self.state.adblock_levels {
            if *level >= adblock::OPTIMAL {
                on.push(host.clone());
            } else {
                off.push(host.clone());
            }
        }
        adblock::youtube_script(self.state.settings.adblock_level >= adblock::OPTIMAL, &off, &on)
    }

    /// Yeni görünüm: YouTube modülü kurulur.
    pub(super) fn attach_adblock(&mut self, i: usize) {
        let Some(view) = self.tabs[i].view.clone() else { return };
        if self.state.settings.adblock {
            engine::add_document_script(&view, self.tabs[i].id, SLOT_YOUTUBE, self.tabs[i].view_epoch, &self.youtube_script());
        }
    }

    /// Ana çerçevede gezinme başladı: o sayfanın gizleme kuralları belge oluşmadan kurulur.
    pub(super) fn on_nav_starting_adblock(&mut self, i: usize, uri: &str) {
        let Some(view) = self.tabs[i].view.clone() else { return };
        let id = self.tabs[i].id;
        if let Some(old) = self.tabs[i].ad_scripts[SLOT_PAGE as usize].take() {
            engine::remove_document_script(&view, &old);
        }
        self.tabs[i].ad_exceptions = None;
        if let Some((script, generic, exceptions)) = adblock::page_resources(uri) {
            if !script.is_empty() {
                engine::add_document_script(&view, id, SLOT_PAGE, self.tabs[i].view_epoch, &script);
            }
            self.tabs[i].ad_exceptions = Some((generic, exceptions));
        }
    }

    /// Belge betiğine yetişemeyen kurallar yüklenen sayfaya doğrudan uygulanır.
    pub(super) fn apply_page_cosmetics(&mut self, i: usize) {
        let Some(view) = self.tabs[i].view.clone() else { return };
        if adblock::level_for(&self.tabs[i].url) < adblock::OPTIMAL {
            engine::execute(&view, "window.__sqCollect?.(false);window.__sqPageStyle?.remove();window.__sqGenericStyle?.node?.remove();");
            return;
        }
        let mut collect = false;
        if let Some((script, generic, exceptions)) = adblock::page_resources(&self.tabs[i].url) {
            collect = generic;
            if !script.is_empty() {
                engine::execute(&view, &script);
            }
            self.tabs[i].ad_exceptions = Some((generic, exceptions));
        }
        engine::execute(&view, &format!("window.__sqCollect?.({collect});"));
    }

    pub(super) fn on_script_added(&mut self, tab: u32, slot: u8, generation: u64, script_id: String) {
        if let Some(i) = self.index_of(tab) {
            if self.tabs[i].view_epoch != generation || self.tabs[i].view.is_none() { return; }
            if let Some(old) = self.tabs[i].ad_scripts[slot as usize].take() {
                if old != script_id { engine::remove_document_script(self.tabs[i].view.as_ref().unwrap(), &old); }
            }
            self.tabs[i].ad_scripts[slot as usize] = Some(script_id);
        }
    }

    /// Sayfa sınıf/kimlik adlarını bildirdi: eşleşen genel gizleme kuralları eklenir.
    pub(super) fn on_cosmetic(&mut self, tab: u32, classes: Vec<String>, ids: Vec<String>) {
        let Some(i) = self.index_of(tab) else { return };
        let Some(view) = self.tabs[i].view.clone() else { return };
        let empty = HashSet::new();
        let (generic, exceptions) = match &self.tabs[i].ad_exceptions {
            Some((g, e)) => (*g, e),
            None => (true, &empty),
        };
        if !generic {
            return;
        }
        if let Some(css) = adblock::generic_css(&self.tabs[i].url, &classes, &ids, exceptions) {
            engine::execute(&view, &adblock::append_css_script(&css));
        }
        if i == self.active {
            self.invalidate(); // Sayaç güncellensin.
        }
    }

    /// YouTube modülünü tüm görünümlerde yeniler (ayar ya da site modu değişince).
    fn refresh_youtube_scripts(&mut self) {
        let on = self.state.settings.adblock;
        let script = self.youtube_script();
        for t in &mut self.tabs {
            let Some(view) = t.view.clone() else { continue };
            if let Some(old) = t.ad_scripts[SLOT_YOUTUBE as usize].take() {
                engine::remove_document_script(&view, &old);
            }
            engine::execute(&view, if on { &script } else { "window.__sqYt?.setEnabled?.(false);" });
            if on {
                engine::add_document_script(&view, t.id, SLOT_YOUTUBE, t.view_epoch, &script);
            }
        }
    }

    pub(super) fn set_adblock_enabled(&mut self, on: bool) {
        self.state.settings.adblock = on;
        adblock::set_enabled(on);
        if on { self.start_adblock(); } else { adblock::release(); self.apply_adblock_levels(); }
        self.refresh_youtube_scripts();
        for i in 0..self.tabs.len() { self.apply_page_cosmetics(i); }
        self.dirty = true;
        self.invalidate();
    }

    /// Etkin sitenin modunu değiştirir (varsayılana eşitse istisna silinir) ve sayfayı yeniler.
    pub(super) fn set_site_level(&mut self, level: u8) {
        let host = url::host(&self.tabs[self.active].url).to_string();
        if host.is_empty() || level > adblock::COMPLETE {
            return;
        }
        if level == self.state.settings.adblock_level {
            self.state.adblock_levels.remove(&host);
        } else {
            self.state.adblock_levels.insert(host, level);
        }
        self.apply_adblock_levels();
        self.dirty = true;
        if let Some(v) = self.active_view() {
            unsafe { let _ = v.Reload(); }
        }
        self.invalidate();
    }

    /// Kalkan: sayfa http(s) ise adres çubuğunda görünür.
    pub(super) fn shield_visible(&self) -> bool {
        // Açılışta düzen, henüz sekme yokken de hesaplanır.
        self.tabs.get(self.active).is_some_and(|t| t.url.starts_with("http") && !url::is_internal(&t.url))
    }

    pub(super) fn shield_count(&self) -> u32 {
        self.tabs.get(self.active).map_or(0, |t| adblock::blocked_count(t.id))
    }

    fn active_level(&self) -> u8 {
        self.tabs.get(self.active).map_or(adblock::OFF, |t| adblock::level_for(&t.url))
    }

    pub(super) fn shield_menu_items(&self) -> Vec<Item> {
        let host = url::host(&self.tabs[self.active].url).to_string();
        let level = adblock::site_level(&host);
        let title = if host.is_empty() { "Reklam engelleyici".to_string() } else { host };
        let mut items = Vec::new();
        if self.state.settings.adblock {
            items.push(Item::Slider { title, level });
            let status = if level == adblock::OFF {
                adblock::LEVEL_HINTS[0].to_string()
            } else if !adblock::is_ready() {
                "Filtre listeleri hazırlanıyor…".to_string()
            } else {
                format!("Bu sayfada {} istek engellendi", self.shield_count())
            };
            items.push(Item::disabled(adblock::LEVEL_HINTS[level as usize]));
            if level != adblock::OFF {
                items.push(Item::disabled(status));
            }
        } else {
            items.push(Item::Header(menu::Header {
                title: "Reklam engelleyici kapalı".into(),
                sub: "Ayarlardan açabilirsin".into(),
                avatar: self.logo.clone(),
                color: accent(),
            }));
        }
        items.push(Item::Sep);
        items.push(Item::act("Engelleyici ayarları", "", MenuCmd::Settings));
        items
    }

    pub(super) fn paint_shield(&self, r: Rect) {
        let g = &self.gfx;
        let on = self.active_level() >= adblock::BASIC;
        if self.hover == Hit::Shield || self.menu.as_ref().is_some_and(|m| m.kind == MenuKind::Shield) {
            g.round(r, 6.0, hover());
        }
        let icon = Rect::new(r.x, r.y, 28.0, r.h);
        g.text(ICON_SHIELD, icon, Font::MidIcon, if on { accent() } else { faint() }, Align::Center);
        let n = self.shield_count();
        if on && n > 0 {
            g.text(&n.to_string(), Rect::new(icon.right() - 4.0, r.y, r.w - 24.0, r.h), Font::Small, muted(), Align::Left);
        }
    }

    pub(super) fn shield_width(&self) -> f32 {
        let n = self.shield_count();
        if n > 0 && self.active_level() >= adblock::BASIC {
            28.0 + self.gfx.measure(&n.to_string(), Font::Small) + 6.0
        } else {
            28.0
        }
    }
}
