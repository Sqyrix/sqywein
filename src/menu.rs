//! Açılır menüler (üç çizgi, profil, uzantılar).
//!
//! Menü ayrı bir üst düzey penceredir: web sayfası kendi penceresinde çizildiği
//! için ana pencereye çizilen bir menü sayfanın altında kalırdı. İçeriği
//! uygulama kurar; burası yalnızca çizer ve tıklamayı komuta çevirir.

use crate::{
    gfx::{Align, Font, Gfx, Rect},
    image::Image,
    theme::*,
};
use std::rc::Rc;
use windows::{
    core::*,
    Win32::{
        Foundation::*,
        Graphics::{Dwm::*, Gdi::InvalidateRect},
        System::LibraryLoader::GetModuleHandleW,
        UI::WindowsAndMessaging::*,
    },
};

pub const CLASS: PCWSTR = w!("SqyweinMenu");

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MenuKind {
    Main,
    Profile,
    Extensions,
    Downloads,
    BarOverflow,
    Shield,
    SavePassword,
    TabContext,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MenuCmd {
    NewTab,
    PrivateTab,
    History,
    Downloads,
    Bookmarks,
    Extensions,
    Passwords,
    ZoomIn,
    ZoomOut,
    Print,
    Find,
    DevTools,
    Settings,
    Exit,
    SignIn,
    ManageAccount,
    SwitchProfile(usize),
    AddProfile,
    ManageProfiles,
    OpenExtension(usize),
    PinExtension(usize),
    Store,
    OpenBookmark(usize),
    OpenDownload(u32),
    ShowDownload(u32),
    CancelDownload(u32),
    SetAdblockLevel(u8),
    SavePassword,
    NeverSavePassword,
    DismissPassword,
    TabNewRight,
    TabReload,
    TabDuplicate,
    TabSleep,
    TabSleepOthers,
    TabCloseOthers,
    TabCloseRight,
    TabClose,
}

/// İndirme baloncuğundaki bir satır.
pub struct DownloadRow {
    pub id: u32,
    pub name: String,
    /// "2,3 MB / 10 MB · 1,2 MB/sn · 8 sn kaldı" gibi.
    pub line: String,
    /// Sürüyorsa 0..1.
    pub progress: Option<f32>,
    pub done: bool,
    /// Dosyayı açan programın simgesi.
    pub icon: Option<Rc<Image>>,
}

pub struct Header {
    pub title: String,
    pub sub: String,
    pub avatar: Option<Rc<Image>>,
    pub color: Rgb,
}

pub enum Item {
    Header(Header),
    Action {
        label: String,
        hint: String,
        cmd: MenuCmd,
        enabled: bool,
        /// Sağdaki ipucuna tıklanınca çalışan ikinci komut (ör. uzantıyı sabitle).
        side: Option<MenuCmd>,
        icon: Option<Rc<Image>>,
    },
    Sep,
    Zoom,
    Download(DownloadRow),
    /// Filtreleme modu kaydırıcısı (site adı, seçili mod 0..3).
    Slider { title: String, level: u8 },
}

impl Item {
    pub fn act(label: impl Into<String>, hint: impl Into<String>, cmd: MenuCmd) -> Self {
        Item::Action { label: label.into(), hint: hint.into(), cmd, enabled: true, side: None, icon: None }
    }

    pub fn disabled(label: impl Into<String>) -> Self {
        Item::Action { label: label.into(), hint: String::new(), cmd: MenuCmd::Exit, enabled: false, side: None, icon: None }
    }

    fn height(&self) -> f32 {
        match self {
            Item::Header(_) => 72.0,
            Item::Action { .. } => 34.0,
            Item::Sep => 9.0,
            Item::Zoom => 38.0,
            Item::Download(_) => 62.0,
            Item::Slider { .. } => 136.0,
        }
    }
}

/// Menüdeki bir noktanın karşılığı.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Part {
    Level(u8),
    Row(usize),
    Side(usize),
    Minus,
    Plus,
}

const PAD: f32 = 6.0;
const SIDE_W: f32 = 84.0;

pub struct Menu {
    pub hwnd: HWND,
    pub kind: MenuKind,
    gfx: Gfx,
    items: Vec<Item>,
    hover: Option<Part>,
    /// Yakınlaştırma oranı (1.0 = %100); sayfa yoksa `None`.
    pub zoom: Option<f64>,
    dpi: u32,
    size: (u32, u32),
}

/// Profil rengi "#AF85FC" → 0xAF85FC.
pub fn parse_color(hex: &str) -> Rgb {
    u32::from_str_radix(hex.trim_start_matches('#'), 16).unwrap_or(ACCENT)
}

/// Yuvarlak avatar: fotoğraf varsa o, yoksa renkli daire içinde baş harf.
pub fn avatar(g: &Gfx, r: Rect, photo: Option<&Image>, color: Rgb, name: &str) {
    match photo {
        Some(img) => g.image(img, r, true),
        None => {
            g.circle(r.cx(), r.cy(), r.w / 2.0, color);
            let letter: String = name.chars().next().map(|c| c.to_uppercase().collect()).unwrap_or_default();
            let font = if r.w >= 30.0 { Font::UiBold } else { Font::Small };
            g.text(&letter, r, font, BASE, Align::Center);
        }
    }
}

impl Menu {
    /// `anchor`: menünün sağ üst köşesi (ekran pikseli).
    pub fn open(owner: HWND, kind: MenuKind, anchor: POINT, dpi: u32, items: Vec<Item>, zoom: Option<f64>, hardware: bool) -> Result<Self> {
        let s = dpi as f32 / 96.0;
        let h: f32 = items.iter().map(Item::height).sum::<f32>() + 2.0 * PAD;
        let (wpx, hpx) = ((MENU_WIDTH * s).round() as i32, (h * s).round() as i32);
        unsafe {
            let instance: HINSTANCE = GetModuleHandleW(None)?.into();
            let hwnd = CreateWindowExW(
                WS_EX_TOOLWINDOW,
                CLASS,
                w!(""),
                WS_POPUP,
                anchor.x - wpx,
                anchor.y,
                wpx,
                hpx,
                Some(owner),
                None,
                Some(instance),
                None,
            )?;
            let dark: i32 = 1;
            let _ = DwmSetWindowAttribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE, &dark as *const _ as _, 4);
            let corner = DWMWCP_ROUND;
            let _ = DwmSetWindowAttribute(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE, &corner as *const _ as _, 4);
            let mut gfx = Gfx::new()?;
            gfx.hardware = hardware;
            gfx.set_dpi(dpi);
            Ok(Self { hwnd, kind, gfx, items, hover: None, zoom, dpi, size: (wpx as u32, hpx as u32) })
        }
    }

    fn rows(&self) -> Vec<Rect> {
        let mut y = PAD;
        self.items
            .iter()
            .map(|it| {
                let r = Rect::new(0.0, y, MENU_WIDTH, it.height());
                y += it.height();
                r
            })
            .collect()
    }

    /// Kaydırıcı yolunun başı, sonu ve dikey ortası.
    fn slider_track(r: Rect) -> (f32, f32, f32) {
        (r.x + 40.0, r.right() - 40.0, r.y + 66.0)
    }

    fn zoom_buttons(row: Rect) -> (Rect, Rect, Rect) {
        let plus = Rect::new(row.right() - 14.0 - 28.0, row.y + 5.0, 28.0, row.h - 10.0);
        let value = Rect::new(plus.x - 52.0, plus.y, 52.0, plus.h);
        let minus = Rect::new(value.x - 28.0, plus.y, 28.0, plus.h);
        (minus, value, plus)
    }

    pub fn hit(&self, lparam: LPARAM) -> Option<Part> {
        let s = self.dpi as f32 / 96.0;
        let x = (lparam.0 & 0xFFFF) as i16 as f32 / s;
        let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as f32 / s;
        for (i, (item, r)) in self.items.iter().zip(self.rows()).enumerate() {
            if !r.contains(x, y) {
                continue;
            }
            return match item {
                Item::Action { enabled: true, side, .. } => {
                    if side.is_some() && x >= r.right() - SIDE_W {
                        Some(Part::Side(i))
                    } else {
                        Some(Part::Row(i))
                    }
                }
                Item::Slider { .. } => {
                    let (x0, x1, cy) = Self::slider_track(r);
                    if (y - cy).abs() <= 26.0 && x >= x0 - 24.0 && x <= x1 + 24.0 {
                        let step = (x1 - x0) / 3.0;
                        Some(Part::Level((((x - x0) / step).round()).clamp(0.0, 3.0) as u8))
                    } else {
                        None
                    }
                }
                Item::Download(_) => {
                    if x >= r.right() - 52.0 {
                        Some(Part::Side(i))
                    } else {
                        Some(Part::Row(i))
                    }
                }
                Item::Zoom if self.zoom.is_some() => {
                    let (minus, _, plus) = Self::zoom_buttons(r);
                    if minus.contains(x, y) {
                        Some(Part::Minus)
                    } else if plus.contains(x, y) {
                        Some(Part::Plus)
                    } else {
                        None
                    }
                }
                _ => None,
            };
        }
        None
    }

    /// Fare hareketi; çizim gerekiyorsa `true`.
    pub fn set_hover(&mut self, part: Option<Part>) -> bool {
        let changed = self.hover != part;
        self.hover = part;
        changed
    }

    pub fn command(&self, part: Part) -> Option<MenuCmd> {
        match part {
            Part::Level(n) => Some(MenuCmd::SetAdblockLevel(n)),
            Part::Minus => Some(MenuCmd::ZoomOut),
            Part::Plus => Some(MenuCmd::ZoomIn),
            Part::Row(i) => match self.items.get(i) {
                Some(Item::Action { cmd, enabled: true, .. }) => Some(*cmd),
                Some(Item::Download(d)) if d.done => Some(MenuCmd::OpenDownload(d.id)),
                _ => None,
            },
            Part::Side(i) => match self.items.get(i) {
                Some(Item::Action { side, enabled: true, .. }) => *side,
                Some(Item::Download(d)) if d.done => Some(MenuCmd::ShowDownload(d.id)),
                Some(Item::Download(d)) if d.progress.is_some() => Some(MenuCmd::CancelDownload(d.id)),
                _ => None,
            },
        }
    }

    /// Menü açıkken içeriği değişti (ör. uzantı sabitlendi).
    pub fn set_items(&mut self, items: Vec<Item>) {
        self.items = items;
        // Satır sayısı değiştiyse pencere ve çizim yüzeyi yeni boya göre ayarlanır.
        let s = self.dpi as f32 / 96.0;
        let h: f32 = self.items.iter().map(Item::height).sum::<f32>() + 2.0 * PAD;
        let hpx = (h * s).round() as u32;
        if hpx != self.size.1 {
            self.size.1 = hpx;
            self.gfx.resize(self.size.0, hpx);
            unsafe {
                let _ = SetWindowPos(self.hwnd, None, 0, 0, self.size.0 as i32, hpx as i32, SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE);
            }
        }
        unsafe { let _ = InvalidateRect(Some(self.hwnd), None, false); }
    }

    pub fn paint(&mut self) {
        let (w, h) = self.size;
        if !self.gfx.begin(self.hwnd, w, h) {
            return;
        }
        let g = &self.gfx;
        g.clear(RAISED);
        let full = Rect::new(0.0, 0.0, MENU_WIDTH, h as f32 * 96.0 / self.dpi as f32);
        g.round_outline(full, 8.0, LINE, 1.0);

        for (i, (item, r)) in self.items.iter().zip(self.rows()).enumerate() {
            match item {
                Item::Header(head) => {
                    let card = r.inset(8.0, 4.0);
                    g.round(card, 10.0, HOVER);
                    avatar(g, Rect::new(card.x + 12.0, card.cy() - 20.0, 40.0, 40.0), head.avatar.as_deref(), head.color, &head.title);
                    g.text(&head.title, Rect::new(card.x + 64.0, card.y + 12.0, card.w - 74.0, 20.0), Font::UiBold, INK, Align::Left);
                    g.text(&head.sub, Rect::new(card.x + 64.0, card.y + 33.0, card.w - 74.0, 18.0), Font::Small, MUTED, Align::Left);
                }
                Item::Action { label, hint, enabled, side, icon, .. } => {
                    let row_hover = self.hover == Some(Part::Row(i));
                    let side_hover = self.hover == Some(Part::Side(i));
                    if *enabled && (row_hover || side_hover) {
                        g.round(r.inset(6.0, 1.0), 6.0, HOVER);
                    }
                    let mut x = r.x + 16.0;
                    if let Some(img) = icon {
                        g.image(img, Rect::new(x, r.cy() - 8.0, 16.0, 16.0), false);
                        x += 26.0;
                    }
                    let color = if *enabled { INK } else { FAINT };
                    let reserve = if hint.is_empty() && side.is_none() { 16.0 } else { SIDE_W + 8.0 };
                    g.text(label, Rect::new(x, r.y, r.right() - x - reserve, r.h), Font::Ui, color, Align::Left);
                    let hint_r = Rect::new(r.right() - SIDE_W - 14.0, r.y, SIDE_W, r.h);
                    if side.is_some() && side_hover {
                        g.round(hint_r.inset(0.0, 5.0), 6.0, LINE);
                    }
                    g.text(hint, hint_r.inset(6.0, 0.0), Font::Small, if side.is_some() { MUTED } else { FAINT }, Align::Right);
                }
                Item::Sep => g.fill(Rect::new(r.x + 10.0, r.cy(), r.w - 20.0, 1.0), LINE),
                Item::Slider { title, level } => {
                    g.text(title, Rect::new(r.x + 12.0, r.y + 8.0, r.w - 24.0, 24.0), Font::UiBold, INK, Align::Center);
                    let (x0, x1, cy) = Self::slider_track(r);
                    let step = (x1 - x0) / 3.0;
                    let at = |n: u8| x0 + n as f32 * step;
                    // Yol: seçili moda kadar vurgu renginde, sonrası sönük; aralarda ince boşluklar.
                    for n in 0..3u8 {
                        let seg = Rect::new(at(n) + 1.0, cy - 7.0, step - 2.0, 14.0);
                        let filled = n < *level;
                        g.round_alpha(seg, 4.0, if filled { ACCENT } else { LINE }, if filled { 0.85 } else { 1.0 });
                    }
                    for n in 0..=3u8 {
                        if self.hover == Some(Part::Level(n)) && n != *level {
                            g.round(Rect::new(at(n) - 14.0, cy - 14.0, 28.0, 28.0), 8.0, HOVER);
                        }
                    }
                    let knob = Rect::new(at(*level) - 22.0, cy - 22.0, 44.0, 44.0);
                    g.round(knob, 12.0, ACCENT);
                    g.text("filtreleme modu", Rect::new(r.x, cy + 28.0, r.w, 18.0), Font::Small, MUTED, Align::Center);
                    let name = crate::adblock::LEVEL_NAMES[*level as usize];
                    g.text(name, Rect::new(r.x, cy + 46.0, r.w, 20.0), Font::UiBold, ACCENT, Align::Center);
                }
                Item::Download(d) => {
                    if self.hover == Some(Part::Row(i)) && d.done {
                        g.round(r.inset(6.0, 2.0), 8.0, HOVER);
                    }
                    let icon = Rect::new(r.x + 14.0, r.cy() - 16.0, 32.0, 32.0);
                    match &d.icon {
                        Some(img) => {
                            // Sürerken soluk, bitince tam renk.
                            g.set_opacity(if d.done { 1.0 } else { 0.55 });
                            g.image(img, icon, false);
                            g.set_opacity(1.0);
                        }
                        None => {
                            g.round(icon, 8.0, if d.done { ACCENT } else { HOVER });
                            g.text(ICON_DOC, icon, Font::Icon, if d.done { BASE } else { INK }, Align::Center);
                        }
                    }
                    let tx = icon.right() + 12.0;
                    let tw = r.right() - tx - 58.0;
                    g.text(&d.name, Rect::new(tx, r.y + 9.0, tw, 20.0), Font::UiBold, INK, Align::Left);
                    g.text(&d.line, Rect::new(tx, r.y + 29.0, tw, 18.0), Font::Small, MUTED, Align::Left);
                    if let Some(p) = d.progress {
                        let bar = Rect::new(tx, r.y + 50.0, tw, 3.0);
                        g.round(bar, 1.5, LINE);
                        g.round(Rect::new(bar.x, bar.y, (bar.w * p.clamp(0.0, 1.0)).max(3.0), bar.h), 1.5, ACCENT);
                    }
                    let side = Rect::new(r.right() - 48.0, r.cy() - 16.0, 32.0, 32.0);
                    if self.hover == Some(Part::Side(i)) {
                        g.round(side, 8.0, LINE);
                    }
                    if d.done {
                        g.text(ICON_FOLDER, side, Font::Icon, INK, Align::Center);
                    } else if d.progress.is_some() {
                        g.text(ICON_CLOSE, side, Font::SmallIcon, MUTED, Align::Center);
                    }
                }
                Item::Zoom => {
                    let enabled = self.zoom.is_some();
                    g.text("Yakınlaştır", Rect::new(r.x + 16.0, r.y, 120.0, r.h), Font::Ui, if enabled { INK } else { FAINT }, Align::Left);
                    let (minus, value, plus) = Self::zoom_buttons(r);
                    for (b, part, icon) in [(minus, Part::Minus, ICON_REMOVE), (plus, Part::Plus, ICON_ADD)] {
                        if enabled && self.hover == Some(part) {
                            g.round(b, 6.0, HOVER);
                        }
                        g.text(icon, b, Font::MidIcon, if enabled { INK } else { FAINT }, Align::Center);
                    }
                    let pct = self.zoom.map(|z| format!("%{}", (z * 100.0).round() as i32)).unwrap_or_else(|| "—".into());
                    g.text(&pct, value, Font::Ui, if enabled { INK } else { FAINT }, Align::Center);
                }
            }
        }
        self.gfx.end();
    }
}

/// Bir sonraki/önceki yakınlaştırma basamağı (Chrome ile aynı basamaklar).
pub fn zoom_step(current: f64, up: bool) -> f64 {
    const STEPS: [f64; 16] = [0.25, 0.33, 0.5, 0.67, 0.75, 0.8, 0.9, 1.0, 1.1, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0, 4.0];
    if up {
        STEPS.iter().copied().find(|s| *s > current + 0.001).unwrap_or(current)
    } else {
        STEPS.iter().rev().copied().find(|s| *s < current - 0.001).unwrap_or(current)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yakinlastirma_basamaklari() {
        assert_eq!(zoom_step(1.0, true), 1.1);
        assert_eq!(zoom_step(1.0, false), 0.9);
        assert_eq!(zoom_step(4.0, true), 4.0);
        assert_eq!(zoom_step(1.05, false), 1.0);
    }

    #[test]
    fn renk_cozme() {
        assert_eq!(parse_color("#AF85FC"), 0xAF85FC);
        assert_eq!(parse_color("bozuk"), ACCENT);
    }
}
