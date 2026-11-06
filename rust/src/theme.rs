//! Sqywein renkleri ve ölçüleri. Tasarım fazında genişletilecek tek kaynak burası.

pub type Rgb = u32;

pub const BASE: Rgb = 0x14121C;
pub const RAISED: Rgb = 0x211D2E;
pub const HOVER: Rgb = 0x2E2940;
pub const LINE: Rgb = 0x3D3654;
pub const INK: Rgb = 0xF4F1FC;
pub const MUTED: Rgb = 0xB8AED0;
pub const FAINT: Rgb = 0x8E84A8;
pub const ACCENT: Rgb = 0xAF85FC;
pub const DANGER: Rgb = 0xC42B1C;
/// Ana sayfadaki arama kutusu: kartın bir ton üstü.
pub const SEARCH: Rgb = 0x2A2540;

/// Win32 COLORREF biçimi (0x00BBGGRR).
pub const fn colorref(rgb: Rgb) -> u32 {
    ((rgb & 0xFF) << 16) | (rgb & 0xFF00) | ((rgb >> 16) & 0xFF)
}

// Ölçüler DIP cinsinden (96 DPI'da piksel).
pub const TAB_ROW: f32 = 40.0;
pub const TOOL_ROW: f32 = 44.0;
pub const CHROME: f32 = TAB_ROW + TOOL_ROW;
pub const GUTTER: f32 = 6.0;
pub const CAPTION_BTN: f32 = 46.0;
pub const TAB_MAX: f32 = 220.0;
pub const TAB_MIN: f32 = 44.0;
pub const RESIZE_EDGE: f32 = 6.0;
pub const MENU_WIDTH: f32 = 272.0;

// Animasyon süreleri (ms).
pub const TAB_OPEN_MS: f32 = 170.0;
pub const TAB_CLOSE_MS: f32 = 150.0;

// Segoe Fluent Icons glifleri.
pub const ICON_BACK: &str = "\u{E72B}";
pub const ICON_FORWARD: &str = "\u{E72A}";
pub const ICON_RELOAD: &str = "\u{E72C}";
pub const ICON_STOP: &str = "\u{E711}";
pub const ICON_ADD: &str = "\u{E710}";
pub const ICON_REMOVE: &str = "\u{E738}";
pub const ICON_CLOSE: &str = "\u{E8BB}";
pub const ICON_MIN: &str = "\u{E921}";
pub const ICON_MAX: &str = "\u{E922}";
pub const ICON_RESTORE: &str = "\u{E923}";
pub const ICON_LOCK: &str = "\u{E72E}";
pub const ICON_SEARCH: &str = "\u{E721}";
pub const ICON_MENU: &str = "\u{E700}";
pub const ICON_STAR: &str = "\u{E734}";
pub const ICON_STAR_FILLED: &str = "\u{E735}";
pub const ICON_PUZZLE: &str = "\u{EA86}";
pub const ICON_DOWNLOAD: &str = "\u{E896}";
pub const ICON_SHIELD: &str = "\u{EA18}";
pub const ICON_GLOBE: &str = "\u{E774}";
pub const ICON_DOC: &str = "\u{E8A5}";
pub const ICON_FOLDER: &str = "\u{E8B7}";

/// Yavaşlayarak biten geçiş: 0..1 → 0..1.
pub fn ease_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}
