//! Ana sayfa (logo, arama, kısayol kutucukları), kısayol ekleme penceresi ve
//! yer işaretleri çubuğu.

use super::*;

pub(super) const BAR_H: f32 = 30.0;
pub(super) const DIALOG_CLASS: PCWSTR = w!("SqyweinDialog");
const TILE_H: f32 = 100.0;
const MAX_TILES: usize = 10;
const DIALOG_W: f32 = 380.0;
const DIALOG_H: f32 = 214.0;
const NAME_ID: i32 = 201;
const URL_ID: i32 = 202;

static LOGO_PNG: &[u8] = include_bytes!("../../Assets/logo.png");

pub(super) struct StartLayout {
    pub logo: Rect,
    pub title: Rect,
    pub search: Rect,
    pub tiles: Vec<Rect>,
    pub add: Option<Rect>,
    /// Favoriler başlığı; altta hava durumu ve döviz.
    pub favorites_title: Rect,
    pub weather: Option<Rect>,
    pub rates: Option<Rect>,
}

impl StartLayout {
    fn shifted(self, dy: f32) -> Self {
        let m = |r: Rect| Rect::new(r.x, r.y + dy, r.w, r.h);
        StartLayout {
            logo: m(self.logo), title: m(self.title), search: m(self.search),
            tiles: self.tiles.into_iter().map(m).collect(), add: self.add.map(m),
            favorites_title: m(self.favorites_title), weather: self.weather.map(m), rates: self.rates.map(m),
        }
    }
}

/// Bir site favorilere kendiliğinden eklenmeden önce en az kaç kez ziyaret edilmeli.
const FAVORITE_VISITS: i64 = 10;
const SECTION_W: f32 = 720.0;
const TILE_COLS: usize = 6;
/// Yan yana iki karemsi kart.
const CARD_H: f32 = 290.0;

/// Kısayol ekleme penceresi: iki metin kutusu ve iki düğme.
pub(super) struct Dialog {
    pub(super) hwnd: HWND,
    name: HWND,
    url: HWND,
    gfx: Gfx,
    hover: u8,
}

#[derive(Clone, Copy, PartialEq)]
enum DialogHit {
    None,
    Cancel,
    Save,
}

fn dialog_buttons() -> (Rect, Rect) {
    let save = Rect::new(DIALOG_W - 20.0 - 96.0, DIALOG_H - 20.0 - 34.0, 96.0, 34.0);
    let cancel = Rect::new(save.x - 10.0 - 96.0, save.y, 96.0, 34.0);
    (cancel, save)
}

fn dialog_fields() -> (Rect, Rect) {
    (Rect::new(20.0, 56.0, DIALOG_W - 40.0, 36.0), Rect::new(20.0, 104.0, DIALOG_W - 40.0, 36.0))
}

impl App {
    pub(super) fn load_logo() -> Option<Rc<Image>> {
        crate::image::from_bytes(LOGO_PNG, 128).map(Rc::new)
    }

    // ---------- Yer işaretleri çubuğu ----------

    pub(super) fn bar_visible(&self) -> bool {
        !self.chrome_hidden() && self.state.settings.show_bookmarks_bar && !self.state.favorites.is_empty()
    }

    fn bar_label(b: &state::Bookmark) -> String {
        if b.title.is_empty() { url::host(&b.url).to_string() } else { b.title.clone() }
    }

    /// Çubuktaki öğeler; sığmayanlar "»" menüsüne gider.
    pub(super) fn bar_layout(&self, y: f32, width: f32) -> (Vec<Rect>, Option<Rect>) {
        let mut items = Vec::new();
        let mut x = 10.0;
        let limit = width - 44.0;
        for b in &self.state.favorites {
            let text = self.gfx.measure(&Self::bar_label(b), Font::Small).min(150.0);
            let w = 10.0 + 16.0 + 6.0 + text + 10.0;
            if x + w > limit {
                return (items, Some(Rect::new(width - 38.0, y + 3.0, 30.0, BAR_H - 6.0)));
            }
            items.push(Rect::new(x, y + 3.0, w, BAR_H - 6.0));
            x += w + 2.0;
        }
        (items, None)
    }

    pub(super) fn paint_bar(&self, l: &Layout) {
        let Some(bar) = l.bar else { return };
        let g = &self.gfx;
        for (i, (r, b)) in l.bar_items.iter().zip(&self.state.favorites).enumerate() {
            if self.hover == Hit::BarItem(i) {
                g.round(*r, 6.0, hover());
            }
            let icon = Rect::new(r.x + 10.0, r.cy() - 8.0, 16.0, 16.0);
            match self.icon_for(&b.url) {
                Some(img) => g.image(&img, icon, false),
                None => menu::avatar(g, icon, None, line(), &Self::bar_label(b)),
            }
            g.text(&Self::bar_label(b), Rect::new(icon.right() + 6.0, r.y, r.right() - icon.right() - 14.0, r.h), Font::Small, ink(), Align::Left);
        }
        if let Some(more) = l.bar_more {
            if self.hover == Hit::BarMore {
                g.round(more, 6.0, hover());
            }
            g.text("»", more, Font::Ui, muted(), Align::Center);
        }
        let _ = bar;
    }

    pub(super) fn bar_overflow_items(&self) -> Vec<Item> {
        let shown = self.layout().bar_items.len();
        self.state
            .favorites
            .iter()
            .enumerate()
            .skip(shown)
            .map(|(i, b)| Item::Action {
                label: Self::bar_label(b),
                hint: String::new(),
                cmd: MenuCmd::OpenBookmark(i),
                enabled: true,
                side: None,
                icon: self.icon_for(&b.url),
            })
            .collect()
    }

    pub(super) fn open_bookmark(&mut self, i: usize, new_tab: bool) {
        let Some(u) = self.state.favorites.get(i).map(|b| b.url.clone()) else { return };
        if new_tab {
            self.new_tab(u, None);
        } else {
            self.navigate(&u);
        }
    }

    // ---------- Ana sayfa ----------

    /// Sık ziyaret edilen siteler (favorilerde olmayanlar), en çok ziyaretten aza.
    /// Sık ziyaret edilen siteler: ziyaretler siteye göre toplanır; en az
    /// `FAVORITE_VISITS` kez girilmiş, favorilerde olmayan ve silinmemiş olanlar.
    pub(super) fn frequent_sites(&self) -> Vec<(String, String)> {
        let mut skip: Vec<String> = self.state.shortcuts.iter().map(|b| url::host(&b.url).trim_start_matches("www.").to_string()).collect();
        skip.extend(self.state.shortcuts_removed.iter().cloned());
        let mut totals: Vec<(String, String, i64)> = Vec::new();
        for e in self.history.recent("", 5000) {
            if !e.url.starts_with("http") { continue; }
            let raw = url::host(&e.url);
            let host = raw.trim_start_matches("www.").to_string();
            if host.is_empty() || skip.contains(&host) { continue; }
            match totals.iter_mut().find(|(h, _, _)| *h == host) {
                Some(t) => t.2 += i64::from(e.visits.max(1)),
                None => totals.push((host, format!("https://{raw}/"), i64::from(e.visits.max(1)))),
            }
        }
        totals.retain(|t| t.2 >= FAVORITE_VISITS);
        totals.sort_by_key(|t| std::cmp::Reverse(t.2));
        totals.into_iter().take(MAX_TILES).map(|(h, u, _)| (h, u)).collect()
    }

    /// Sık ziyaret edilen siteler favorilere kendiliğinden kaydedilir (boş yer kadar).
    pub(super) fn auto_favorites(&mut self) {
        if self.tabs.get(self.active).is_some_and(|t| t.private) { return; }
        let room = MAX_TILES.saturating_sub(self.state.shortcuts.len());
        if room == 0 { return; }
        let add: Vec<(String, String)> = self.frequent_sites().into_iter().take(room).collect();
        if add.is_empty() { return; }
        for (host, u) in add {
            self.state.shortcuts.push(state::Bookmark { title: host, url: u });
        }
        self.dirty = true;
        self.invalidate();
    }

    /// Engelleyicinin sayacını günlük kayda ekler (ara ara çağrılır).
    pub(super) fn fold_privacy_stats(&mut self) {
        let n = crate::adblock::take_total();
        if n == 0 { return; }
        let today = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() / 86400);
        let days = &mut self.state.privacy_days;
        match days.iter_mut().find(|(d, _)| *d == today) {
            Some((_, c)) => *c += n,
            None => days.push((today, n)),
        }
        days.retain(|(d, _)| today.saturating_sub(*d) < 7);
        self.dirty = true;
    }

    // ---------- Ana sayfa (Safari tarzı) ----------

    pub(super) fn start_layout(&self, content: Rect) -> StartLayout {
        let private = self.tabs.get(self.active).is_some_and(|t| t.private);
        let cx = content.cx();
        let w = (content.w - 64.0).clamp(240.0, SECTION_W);
        let x0 = cx - w / 2.0;
        if private {
            // Gizli sekme: sade, ortalanmış.
            let top = (content.cy() - 150.0).max(content.y + 24.0);
            let logo = Rect::new(cx - 36.0, top, 72.0, 72.0);
            let title = Rect::new(content.x, logo.bottom() + 6.0, content.w, 40.0);
            let search_w = (content.w - 64.0).clamp(120.0, 600.0);
            let search = Rect::new(cx - search_w / 2.0, title.bottom() + 34.0, search_w, 54.0);
            return StartLayout { logo, title, search, tiles: vec![], add: None, favorites_title: Rect::new(0.0, 0.0, 0.0, 0.0), weather: None, rates: None };
        }
        let mut y = content.y + 36.0;
        let logo=Rect::new(cx,y,0.0,0.0);
        let title=logo;
        let search=logo;

        // Favoriler: eşit aralıklı kare kutucuklar.
        let tile_w = w / TILE_COLS as f32;
        let grid = |start_y: f32, count: usize| -> Vec<Rect> {
            (0..count).map(|k| Rect::new(x0 + (k % TILE_COLS) as f32 * tile_w, start_y + (k / TILE_COLS) as f32 * TILE_H, tile_w, TILE_H)).collect()
        };
        let favorites_title = Rect::new(x0, y, w, 22.0);
        y += 28.0;
        let n = self.state.shortcuts.len().min(MAX_TILES);
        let count = n + usize::from(n < MAX_TILES);
        let mut tiles = grid(y, count);
        let add = (n < MAX_TILES).then(|| tiles.pop()).flatten();
        y += count.div_ceil(TILE_COLS) as f32 * TILE_H + 18.0;
        let room = |y: f32, h: f32| y + h <= content.bottom() - 16.0;

        // Altta yan yana iki karemsi cam kart: hava durumu ve döviz.
        let (weather, rates) = if self.state.settings.show_widgets && room(y, CARD_H) {
            let gap = 16.0;
            let cw = (w - gap) / 2.0;
            (Some(Rect::new(x0, y, cw, CARD_H)), Some(Rect::new(x0 + cw + gap, y, cw, CARD_H)))
        } else { (None, None) };

        let layout = StartLayout { logo, title, search, tiles, add, favorites_title, weather, rates };
        // Blok dikeyde ortalanır (biraz yukarıda, göz hizasında); sığmıyorsa üstten başlar.
        let bottom = layout.rates.or(layout.weather).map(|r| r.bottom())
            .or_else(|| layout.tiles.last().map(|r| r.bottom()))
            .or(layout.add.map(|r| r.bottom()))
            .unwrap_or(search.bottom());
        let free = content.bottom() - bottom - 24.0;
        if free > 0.0 { layout.shifted(free * 0.42) } else { layout }
    }

    fn section_title(&self, r: Rect, text: &str) {
        self.gfx.text(text, r, Font::UiBold, ink(), Align::Left);
    }

    pub(super) fn paint_start(&self, l: &Layout) {
        let g = &self.gfx;
        let private = self.tabs[self.active].private;
        let s = self.start_layout(l.content);
        let r = s.search;
        let focused = unsafe { GetFocus() } == self.start_edit;
        let radius = r.h / 2.0;

        if private {
            g.glow(l.content.cx(), s.logo.cy() + 20.0, 280.0, 120.0, accent(), 0.16);
            if let Some(img) = &self.logo { g.image(img, s.logo, false); }
            g.text("Gizli sekme", s.title, Font::Big, ink(), Align::Center);
        }

        if private {
        if focused {
            g.round_outline_alpha(r.inset(-4.0, -4.0), radius + 4.0, accent(), 4.0, 0.10);
        }
        g.round(r, radius, search());
        g.round_outline(r, radius, if focused { accent() } else { line() }, if focused { 1.5 } else { 1.0 });
        g.text(ICON_SEARCH, Rect::new(r.x + 14.0, r.y, 26.0, r.h), Font::Icon, if focused { accent() } else { muted() }, Align::Center);
        }

        if private {
            g.text("Geçmiş ve çerezler bu sekme kapanınca silinir.", Rect::new(l.content.x, r.bottom() + 24.0, l.content.w, 20.0), Font::Small, faint(), Align::Center);
            return;
        }

        let edit=Self::customize_rect(l.content);
        g.round(edit,13.0,if self.hover==Hit::Customize {hover()}else{search()});
        g.text("Düzenle",edit,Font::Small,ink(),Align::Center);
        // Favoriler
        self.section_title(s.favorites_title, "Favoriler");
        for (i, (t, b)) in s.tiles.iter().zip(&self.state.shortcuts).enumerate() {
            let hovered = matches!(self.hover, Hit::Tile(h) | Hit::TileRemove(h) if h == i);
            self.paint_tile(*t, &Self::bar_label(b), self.icon_for(&b.url).as_deref(), hovered, false);
            if hovered {
                let x = Self::tile_remove(*t);
                g.circle(x.cx(), x.cy(), 10.0, if self.hover == Hit::TileRemove(i) { danger() } else { line() });
                g.text(ICON_CLOSE, x, Font::SmallIcon, ink(), Align::Center);
            }
        }
        if let Some(add) = s.add {
            self.paint_tile(add, "Ekle", None, self.hover == Hit::TileAdd, true);
        }

        if let Some(card) = s.weather { self.paint_weather(card); }
        if let Some(card) = s.rates { self.paint_rates(card); }
    }

    /// Liquid Glass yüzeyi: altta gölge, üstte ışık yansıması, ince parlak kenar.
    fn glass(&self, r: Rect, radius: f32, tint: Option<(u32, u32)>) {
        let g = &self.gfx;
        g.glow(r.cx(), r.bottom() - 6.0, r.w * 0.48, 22.0, 0x000000, if is_dark() { 0.40 } else { 0.14 });
        match tint {
            Some((top, bottom)) => g.round_gradient(r, radius, top, bottom, true),
            None if is_dark() => g.round_gradient_alpha(r, radius, (0xFFFFFF, 0.10), (0xFFFFFF, 0.04), false),
            None => g.round_gradient_alpha(r, radius, (0xFFFFFF, 0.75), (0xFFFFFF, 0.45), false),
        }
        // Üst yarıda yumuşak ışık (camın kavisi).
        let sheen = Rect::new(r.x, r.y, r.w, r.h * 0.55);
        g.round_gradient_alpha(sheen, radius, (0xFFFFFF, if tint.is_some() { 0.22 } else { 0.10 }), (0xFFFFFF, 0.0), false);
        g.round_outline_alpha(r, radius, 0xFFFFFF, 1.2, if tint.is_some() || is_dark() { 0.30 } else { 0.9 });
        g.round_outline_alpha(r.inset(1.5, 1.5), radius - 1.5, 0xFFFFFF, 1.0, 0.07);
        if tint.is_none() && !is_dark() { g.round_outline_alpha(r, radius, 0x000000, 1.0, 0.06); }
    }

    /// Pixel / macOS tarzı hava durumu: kartın kendisi gökyüzü.
    fn paint_weather(&self, r: Rect) {
        use crate::widgets::{describe, sky, WeatherState};
        let g = &self.gfx;
        let pad = 24.0;
        let white = 0xFFFFFF;
        let Some(WeatherState::Ready(w)) = &self.weather else {
            self.glass(r, 24.0, None);
            let sub = match &self.weather {
                Some(WeatherState::NoLocation) => "Konumun alınamadı. Tıkla, Ayarlar'dan şehrini yaz.",
                Some(WeatherState::Failed) => "Şu an alınamadı; birazdan yeniden denenecek.",
                _ => "Yükleniyor…",
            };
            g.text("🌤️", Rect::new(r.x + pad, r.cy() - 30.0, 60.0, 60.0), Font::Emoji, ink(), Align::Center);
            g.text("Hava durumu", Rect::new(r.x + pad + 76.0, r.cy() - 22.0, r.w - 140.0, 22.0), Font::UiBold, ink(), Align::Left);
            g.text(sub, Rect::new(r.x + pad + 76.0, r.cy(), r.w - 140.0, 20.0), Font::Small, muted(), Align::Left);
            return;
        };
        self.glass(r, 24.0, Some(sky(w.code, w.day)));
        // Havaya göre süs: güneş, yıldızlar, yağmur, kar.
        let seed = |k: usize| ((k as f32 * 12.9898).sin() * 43758.547).fract().abs();
        match (w.code, w.day) {
            (0 | 1, true) => {
                g.glow(r.right() - 84.0, r.y + 54.0, 150.0, 110.0, 0xFFE7A0, 0.55);
                g.glow(r.right() - 84.0, r.y + 54.0, 54.0, 54.0, 0xFFF6D8, 0.6);
            }
            (0..=3, false) => {
                for k in 0..34 {
                    let (x, y) = (r.x + 18.0 + seed(k) * (r.w - 36.0), r.y + 12.0 + seed(k + 50) * (r.h * 0.5));
                    g.circle(x, y, 0.6 + seed(k + 99) * 1.0, white);
                }
            }
            (51..=67 | 80..=82 | 95..=99, _) => {
                for k in 0..40 {
                    let (x, y) = (r.x + seed(k) * r.w, r.y + seed(k + 40) * (r.h - 18.0));
                    g.polyline(&[(x, y), (x - 4.0, y + 12.0)], white, 0.20, 1.4);
                }
            }
            (71..=77 | 85 | 86, _) => {
                for k in 0..44 {
                    g.circle(r.x + seed(k) * r.w, r.y + seed(k + 70) * r.h, 1.0 + seed(k + 5) * 1.8, white);
                }
            }
            _ => {}
        }
        let (text, emoji) = describe(w.code, w.day);
        let place = if w.place.is_empty() { "Bulunduğun yer".to_string() } else { w.place.clone() };
        // Üst: yer, dev sıcaklık, durum; sağda büyük simge.
        g.text(&place, Rect::new(r.x + pad, r.y + 20.0, r.w - pad * 2.0 - 70.0, 22.0), Font::UiBold, white, Align::Left);
        g.text(&format!("{:.0}°", w.temp), Rect::new(r.x + pad - 3.0, r.y + 40.0, 180.0, 72.0), Font::Giant, white, Align::Left);
        g.text(emoji, Rect::new(r.right() - pad - 70.0, r.y + 16.0, 70.0, 66.0), Font::Emoji, white, Align::Center);
        g.text(text, Rect::new(r.x + pad, r.y + 114.0, r.w - pad * 2.0, 20.0), Font::UiBold, white, Align::Left);
        // Alt: önümüzdeki günler, dikey buzlu cam hücrelerde.
        let n = w.days.len().max(1) as f32;
        let gap = 6.0;
        let cw = (r.w - pad * 2.0 - gap * (n - 1.0)) / n;
        for (k, (day, code, hi, lo)) in w.days.iter().enumerate() {
            let c = Rect::new(r.x + pad + k as f32 * (cw + gap), r.y + 150.0, cw, 116.0);
            g.round_alpha(c, 14.0, white, 0.13);
            g.round_outline_alpha(c, 14.0, white, 1.0, 0.18);
            g.text(day, Rect::new(c.x, c.y + 8.0, c.w, 18.0), Font::Small, white, Align::Center);
            g.text(describe(*code, true).1, Rect::new(c.x, c.y + 30.0, c.w, 28.0), Font::EmojiSmall, white, Align::Center);
            g.text(&format!("{hi:.0}°"), Rect::new(c.x, c.y + 64.0, c.w, 20.0), Font::UiBold, white, Align::Center);
            g.text(&format!("{lo:.0}°"), Rect::new(c.x, c.y + 86.0, c.w, 18.0), Font::Small, white, Align::Center);
        }
    }

    /// Döviz: karemsi cam kart; her kur bir satır (rozet, değer, değişim, küçük grafik).
    fn paint_rates(&self, r: Rect) {
        let g = &self.gfx;
        self.glass(r, 24.0, None);
        let pad = 22.0;
        g.text("Döviz", Rect::new(r.x + pad, r.y + 20.0, r.w - pad * 2.0, 22.0), Font::UiBold, ink(), Align::Left);
        let Some(rates) = &self.rates else {
            g.text("Kurlar yükleniyor…", r, Font::Small, muted(), Align::Center);
            return;
        };
        let date = rates.date.split('-').rev().collect::<Vec<_>>().join(".");
        g.text(&date, Rect::new(r.x + pad, r.y + 20.0, r.w - pad * 2.0, 22.0), Font::Small, faint(), Align::Right);
        for (k, rate) in rates.items.iter().enumerate() {
            let row = Rect::new(r.x + pad, r.y + 58.0 + k as f32 * 74.0, r.w - pad * 2.0, 66.0);
            if k > 0 { g.fill_alpha(Rect::new(row.x, row.y - 5.0, row.w, 1.0), line(), 0.6); }
            let (symbol, badge, name) = match rate.code.as_str() { "USD" => ("$", 0x34C759, "Dolar"), "EUR" => ("€", 0x0A84FF, "Euro"), _ => ("£", 0x0A84FF, "Sterlin") };
            g.circle(row.x + 17.0, row.y + 22.0, 17.0, badge);
            g.text(symbol, Rect::new(row.x, row.y + 5.0, 34.0, 34.0), Font::UiBold, 0xFFFFFF, Align::Center);
            g.text(name, Rect::new(row.x + 44.0, row.y + 4.0, 90.0, 20.0), Font::UiBold, ink(), Align::Left);
            let up = rate.change >= 0.0;
            let color = if rate.change.abs() < 0.005 { faint() } else if up { 0x34C759 } else { 0xFF453A };
            g.text(&format!("{} {:.2}%", if up { "▲" } else { "▼" }, rate.change.abs()).replace('.', ","), Rect::new(row.x + 44.0, row.y + 24.0, 90.0, 18.0), Font::Small, color, Align::Left);
            g.text(&format!("{:.2} ₺", rate.value).replace('.', ","), Rect::new(row.right() - 100.0, row.y + 2.0, 100.0, 24.0), Font::UiBold, ink(), Align::Right);
            let chart = Rect::new(row.right() - 100.0, row.y + 28.0, 100.0, 22.0);
            if rate.series.len() >= 2 {
                let (lo, hi) = rate.series.iter().fold((f64::MAX, f64::MIN), |(a, b), v| (a.min(*v), b.max(*v)));
                let span = (hi - lo).max(1e-9);
                let pts: Vec<(f32, f32)> = rate.series.iter().enumerate().map(|(i, v)| {
                    (chart.x + chart.w * i as f32 / (rate.series.len() - 1) as f32, chart.bottom() - chart.h * ((v - lo) / span) as f32)
                }).collect();
                g.polyline(&pts, color, 0.95, 1.8);
            }
        }
    }

    fn tile_remove(t: Rect) -> Rect {
        Rect::new(t.cx() + 22.0, t.y - 2.0, 20.0, 20.0)
    }

    fn paint_tile(&self, t: Rect, label: &str, icon: Option<&Image>, hovered: bool, add: bool) {
        let g = &self.gfx;
        let sq = Rect::new(t.cx() - 32.0, t.y + 4.0, 64.0, 64.0);
        if is_dark() { g.round_alpha(sq, 16.0, 0xFFFFFF, if hovered { 0.16 } else { 0.08 }); } else { g.round_alpha(sq, 16.0, 0x000000, if hovered { 0.09 } else { 0.045 }); }
        g.round_outline_alpha(sq, 16.0, line(), 1.0, if hovered { 0.9 } else { 0.5 });
        let inner = sq.centered(30.0, 30.0);
        if add {
            g.text(ICON_ADD, inner, Font::Icon, muted(), Align::Center);
        } else {
            match icon {
                Some(img) => g.image(img, inner, false),
                None => menu::avatar(g, inner, None, accent(), label),
            }
        }
        g.text(label, Rect::new(t.x + 4.0, sq.bottom() + 6.0, t.w - 8.0, 18.0), Font::Small, if hovered { ink() } else { muted() }, Align::Center);
    }

    /// Ana sayfadaki bir noktanın karşılığı.
    pub(super) fn customize_rect(content:Rect)->Rect{Rect::new(content.right()-94.0,content.bottom()-42.0,76.0,26.0)}
    pub(super) fn start_hit(&self, content: Rect, x: f32, y: f32) -> Hit {
        if self.tabs[self.active].private {
            return Hit::None;
        }
        if Self::customize_rect(content).contains(x,y){return Hit::Customize;}
        let s = self.start_layout(content);
        for (i, t) in s.tiles.iter().enumerate().take(self.state.shortcuts.len()) {
            if Self::tile_remove(*t).contains(x, y) {
                return Hit::TileRemove(i);
            }
            if t.inset(4.0, 2.0).contains(x, y) {
                return Hit::Tile(i);
            }
        }
        if s.add.is_some_and(|a| a.inset(4.0, 2.0).contains(x, y)) {
            return Hit::TileAdd;
        }
        if s.weather.is_some_and(|r| r.contains(x, y)) && matches!(self.weather, Some(crate::widgets::WeatherState::NoLocation)) {
            return Hit::Weather;
        }
        Hit::None
    }

    pub(super) fn open_shortcut(&mut self, i: usize, new_tab: bool) {
        let Some(u) = self.state.shortcuts.get(i).map(|b| b.url.clone()) else { return };
        if new_tab {
            self.new_tab(u, None);
        } else {
            self.navigate(&u);
        }
    }

    pub(super) fn remove_shortcut(&mut self, i: usize) {
        if i < self.state.shortcuts.len() {
            let removed = self.state.shortcuts.remove(i);
            let host = url::host(&removed.url).trim_start_matches("www.").to_string();
            if !host.is_empty() && !self.state.shortcuts_removed.contains(&host) {
                self.state.shortcuts_removed.push(host);
                if self.state.shortcuts_removed.len() > 200 { self.state.shortcuts_removed.remove(0); }
            }
            self.dirty = true;
            self.hover = Hit::None;
            self.layout_children();
            self.invalidate();
        }
    }

    // ---------- Kısayol ekleme penceresi ----------

    pub(super) fn open_dialog(&mut self) {
        if self.dialog.is_some() {
            return;
        }
        let s = self.scale();
        let (w, h) = ((DIALOG_W * s) as i32, (DIALOG_H * s) as i32);
        let mut rc = RECT::default();
        unsafe { let _ = GetWindowRect(self.hwnd, &mut rc); }
        let (x, y) = ((rc.left + rc.right - w) / 2, (rc.top + rc.bottom - h) / 2);
        let Ok(instance) = (unsafe { GetModuleHandleW(None) }) else { return };
        let instance: HINSTANCE = instance.into();
        unsafe {
            let Ok(hwnd) = CreateWindowExW(WS_EX_TOOLWINDOW, DIALOG_CLASS, w!("Kısayol ekle"), WS_POPUP, x, y, w, h, Some(self.hwnd), None, Some(instance), None) else {
                return;
            };
            let dark: i32 = i32::from(is_dark());
            let _ = DwmSetWindowAttribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE, &dark as *const _ as _, 4);
            let corner = DWMWCP_ROUND;
            let _ = DwmSetWindowAttribute(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE, &corner as *const _ as _, 4);
            let (Ok(name), Ok(url)) = (create_edit(hwnd, instance, NAME_ID, "Ad"), create_edit(hwnd, instance, URL_ID, "Adres (ör. youtube.com)")) else {
                let _ = DestroyWindow(hwnd);
                return;
            };
            for e in [name, url] {
                SendMessageW(e, WM_SETFONT, Some(WPARAM(self.edit_font.0 as usize)), Some(LPARAM(1)));
            }
            let (f1, f2) = dialog_fields();
            for (e, f) in [(name, f1), (url, f2)] {
                let line = (13.5 * 1.45 * s).round() as i32;
                let _ = SetWindowPos(e, None, ((f.x + 12.0) * s) as i32, (f.cy() * s) as i32 - line / 2, ((f.w - 24.0) * s) as i32, line, SWP_NOZORDER);
            }
            let Ok(mut gfx) = Gfx::new() else {
                let _ = DestroyWindow(hwnd);
                return;
            };
            gfx.set_dpi(self.dpi);
            self.dialog = Some(Dialog { hwnd, name, url, gfx, hover: 0 });
            defer(move || {
                let _ = ShowWindow(hwnd, SW_SHOW);
                let _ = SetFocus(Some(name));
            });
        }
    }

    pub(super) fn close_dialog(&mut self) {
        if let Some(d) = self.dialog.take() {
            let hwnd = d.hwnd;
            defer(move || unsafe { let _ = DestroyWindow(hwnd); });
        }
    }

    fn save_dialog(&mut self) {
        let Some(d) = &self.dialog else { return };
        let (name, address) = (Self::text_of(d.name).trim().to_string(), Self::text_of(d.url));
        let Some(target) = url::to_url_with(&address, self.state.settings.search_url()) else {
            let url = d.url;
            defer(move || unsafe { let _ = SetFocus(Some(url)); });
            return;
        };
        let title = if name.is_empty() { url::host(&target).to_string() } else { name };
        self.state.shortcuts.push(state::Bookmark { title, url: target });
        self.dirty = true;
        self.close_dialog();
        self.invalidate();
    }

    /// Penceredeki metin kutularına gelen Enter, Esc ve Tab.
    pub(super) fn dialog_key(&mut self, hwnd: HWND, vk: u32) -> bool {
        let Some(d) = &self.dialog else { return false };
        if hwnd != d.name && hwnd != d.url {
            return false;
        }
        match vk {
            0x0D => self.save_dialog(),
            0x1B => self.close_dialog(),
            0x09 => {
                let next = if hwnd == d.name { d.url } else { d.name };
                defer(move || unsafe { let _ = SetFocus(Some(next)); });
            }
            _ => return false,
        }
        true
    }

    fn dialog_hit(&self, lparam: LPARAM) -> DialogHit {
        let s = self.scale();
        let (x, y) = ((lparam.0 & 0xFFFF) as i16 as f32 / s, ((lparam.0 >> 16) & 0xFFFF) as i16 as f32 / s);
        let (cancel, save) = dialog_buttons();
        if cancel.contains(x, y) {
            DialogHit::Cancel
        } else if save.contains(x, y) {
            DialogHit::Save
        } else {
            DialogHit::None
        }
    }

    pub(super) fn dialog_message(&mut self, hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> Option<LRESULT> {
        if self.dialog.as_ref().map(|d| d.hwnd) != Some(hwnd) {
            return None;
        }
        match msg {
            WM_PAINT => {
                let mut ps = PAINTSTRUCT::default();
                unsafe { BeginPaint(hwnd, &mut ps) };
                self.paint_dialog();
                unsafe { let _ = EndPaint(hwnd, &ps); }
                Some(LRESULT(0))
            }
            WM_CTLCOLOREDIT => {
                let hdc = HDC(wparam.0 as _);
                unsafe {
                    SetTextColor(hdc, COLORREF(colorref(ink())));
                    SetBkColor(hdc, COLORREF(colorref(search())));
                }
                Some(LRESULT(edit_brushes().1 .0 as isize))
            }
            WM_MOUSEMOVE => {
                let h = match self.dialog_hit(lparam) {
                    DialogHit::None => 0,
                    DialogHit::Cancel => 1,
                    DialogHit::Save => 2,
                };
                if let Some(d) = &mut self.dialog {
                    if d.hover != h {
                        d.hover = h;
                        unsafe { let _ = InvalidateRect(Some(hwnd), None, false); }
                    }
                }
                Some(LRESULT(0))
            }
            WM_LBUTTONUP => {
                match self.dialog_hit(lparam) {
                    DialogHit::Cancel => self.close_dialog(),
                    DialogHit::Save => self.save_dialog(),
                    DialogHit::None => {}
                }
                Some(LRESULT(0))
            }
            WM_ACTIVATE if (wparam.0 & 0xFFFF) as u32 == WA_INACTIVE => {
                // Pencere dışına tıklanınca vazgeçilmiş sayılır.
                let child = HWND(lparam.0 as _);
                if !(self.dialog.as_ref().is_some_and(|d| child == d.name || child == d.url)) {
                    self.close_dialog();
                }
                Some(LRESULT(0))
            }
            WM_COMMAND => {
                unsafe { let _ = InvalidateRect(Some(hwnd), None, false); }
                Some(LRESULT(0))
            }
            _ => None,
        }
    }

    fn paint_dialog(&mut self) {
        let s = self.scale();
        let Some(d) = &mut self.dialog else { return };
        let (w, h) = ((DIALOG_W * s) as u32, (DIALOG_H * s) as u32);
        if !d.gfx.begin(d.hwnd, w, h) {
            return;
        }
        let g = &d.gfx;
        g.clear(raised());
        g.round_outline(Rect::new(0.0, 0.0, DIALOG_W, DIALOG_H), 8.0, line(), 1.0);
        g.text("Kısayol ekle", Rect::new(20.0, 14.0, DIALOG_W - 40.0, 28.0), Font::UiBold, ink(), Align::Left);
        let focus = unsafe { GetFocus() };
        let (f1, f2) = dialog_fields();
        for (f, e) in [(f1, d.name), (f2, d.url)] {
            g.round(f, 10.0, search());
            g.round_outline(f, 10.0, if focus == e { accent() } else { line() }, if focus == e { 1.5 } else { 1.0 });
        }
        let (cancel, save) = dialog_buttons();
        g.round(cancel, 10.0, if d.hover == 1 { line() } else { hover() });
        g.text("İptal", cancel, Font::Ui, ink(), Align::Center);
        g.round(save, 10.0, if d.hover == 2 { ink() } else { accent() });
        g.text("Ekle", save, Font::Ui, base(), Align::Center);
        d.gfx.end();
    }
}
