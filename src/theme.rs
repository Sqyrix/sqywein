//! Sqywein renkleri ve ölçüleri. Tasarım fazında genişletilecek tek kaynak burası.

pub type Rgb = u32;

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Palette { pub base: Rgb, pub raised: Rgb, pub hover: Rgb, pub line: Rgb, pub ink: Rgb, pub muted: Rgb, pub faint: Rgb, pub field: Rgb, pub danger: Rgb }
const SQYWEIN: Palette = Palette {base:0x14121C,raised:0x211D2E,hover:0x2E2940,line:0x3D3654,ink:0xF4F1FC,muted:0xB8AED0,faint:0x8E84A8,field:0x2A2540,danger:0xFF857C};
const DARK: Palette = Palette {base:0x1C1C1E,raised:0x2C2C2E,hover:0x3A3A3C,line:0x48484A,ink:0xF5F5F7,muted:0xAEAEB2,faint:0x98989D,field:0x3A3A3C,danger:0xFF6961};
const LIGHT: Palette = Palette {base:0xF5F5F7,raised:0xFFFFFF,hover:0xE5E5EA,line:0xD1D1D6,ink:0x1D1D1F,muted:0x515154,faint:0x6E6E73,field:0xE8E8ED,danger:0xB42318};
thread_local! {
    static HIGH_CONTRAST: std::cell::Cell<bool> = const {std::cell::Cell::new(false)};
    static PALETTE: std::cell::Cell<Palette> = const {std::cell::Cell::new(SQYWEIN)};
    static THEME: std::cell::Cell<&'static str> = const {std::cell::Cell::new("sqywein")};
}
pub fn palette() -> Palette { PALETTE.with(|p|p.get()) }
pub fn base() -> Rgb {palette().base}
pub fn raised() -> Rgb {palette().raised}
pub fn hover() -> Rgb {palette().hover}
pub fn line() -> Rgb {palette().line}
pub fn ink() -> Rgb {palette().ink}
pub fn muted() -> Rgb {palette().muted}
pub fn faint() -> Rgb {palette().faint}
pub fn search() -> Rgb {palette().field}
pub fn danger() -> Rgb {palette().danger}
pub fn is_dark() -> bool {let c=base(); ((c>>16)&255)*299+((c>>8)&255)*587+(c&255)*114 < 128000}
pub fn high_contrast()->bool {HIGH_CONTRAST.with(|h|h.get())}
fn contrast_palette()->Option<Palette>{
    use windows::Win32::{UI::{Accessibility::{HIGHCONTRASTW,HCF_HIGHCONTRASTON},WindowsAndMessaging::*},Graphics::Gdi::*};
    let mut hc=HIGHCONTRASTW{cbSize:std::mem::size_of::<HIGHCONTRASTW>() as u32,..Default::default()};
    if unsafe{SystemParametersInfoW(SPI_GETHIGHCONTRAST,hc.cbSize,Some(&mut hc as *mut _ as _),SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0))}.is_err() || (hc.dwFlags & HCF_HIGHCONTRASTON).0==0 {return None}
    let color=|index|{let c=unsafe{GetSysColor(index)};colorref(c)};
    let bg=color(COLOR_WINDOW);let fg=color(COLOR_WINDOWTEXT);
    Some(Palette{base:bg,raised:bg,hover:color(COLOR_BTNFACE),line:fg,ink:fg,muted:fg,faint:color(COLOR_GRAYTEXT),field:bg,danger:fg})
}
pub fn system_is_light() -> bool {
    use windows::{core::*,Win32::System::Registry::*};
    let mut value=1u32;let mut size=4u32;
    unsafe { let _ = RegGetValueW(HKEY_CURRENT_USER,w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),w!("AppsUseLightTheme"),RRF_RT_REG_DWORD,None,Some(&mut value as *mut _ as _),Some(&mut size)); }
    value!=0
}
fn resolve(name:&str, system_light:bool) -> Palette {
    match name {"light"=>LIGHT,"dark"=>DARK,"system"=>if system_light{LIGHT}else{DARK},_=>SQYWEIN}
}
pub fn set_theme(name: &str) -> bool {
    let name=match name {"light"=>"light","dark"=>"dark","system"=>"system",_=>"sqywein"};
    let hc=contrast_palette();HIGH_CONTRAST.with(|h|h.set(hc.is_some()));
    let palette=hc.unwrap_or_else(||resolve(name,name=="system"&&system_is_light()));
    THEME.with(|t|t.replace(name)!=name)|PALETTE.with(|p|p.replace(palette)!=palette)
}
pub fn message() -> serde_json::Value {
    let p=palette();
    serde_json::json!({"type":"theme","name":THEME.with(|t|t.get()),"dark":is_dark(),"accent":hex(accent()),"colors":{
        "base":hex(p.base),"raised":hex(p.raised),"hover":hex(p.hover),"line":hex(p.line),"ink":hex(p.ink),"muted":hex(p.muted),"faint":hex(p.faint),"field":hex(p.field),"danger":hex(p.danger)}})
}
pub fn css() -> String {
    let msg=message();let mut css=format!(":root{{color-scheme:{};",if is_dark(){"dark"}else{"light"});
    if let Some(colors)=msg["colors"].as_object(){for(k,v)in colors{if let Some(v)=v.as_str(){css.push_str(&format!("--{k}:{v};"));}}}
    css.push_str(&format!("--accent:{};}}",hex(accent())));css
}
/// Sqywein'in varsayılan vurgu rengi (mor).
pub const DEFAULT_ACCENT: Rgb = 0xAF85FC;

/// Seçilebilir vurgu renkleri: (ayar adı, görünen ad, renk).
pub const ACCENTS: [(&str, &str, Rgb); 6] = [
    ("purple", "Mor", 0xAF85FC),
    ("blue", "Mavi", 0x0A84FF),
    ("green", "Yeşil", 0x5FD39A),
    ("orange", "Turuncu", 0xFFA45C),
    ("pink", "Pembe", 0xF28AC9),
    ("teal", "Turkuaz", 0x4FD1D9),
];

thread_local! {
    static CURRENT_ACCENT: std::cell::Cell<Rgb> = const { std::cell::Cell::new(DEFAULT_ACCENT) };
}

thread_local! { static MOTION: std::cell::Cell<bool> = const { std::cell::Cell::new(true) }; }
pub fn motion_enabled() -> bool { MOTION.with(|m| m.get()) }
pub fn set_motion(enabled: bool) {
    use windows::{core::BOOL, Win32::UI::WindowsAndMessaging::*};
    let mut system = BOOL(1);
    unsafe { let _ = SystemParametersInfoW(SPI_GETCLIENTAREAANIMATION, 0, Some(&mut system as *mut _ as _), SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0)); }
    MOTION.with(|m| m.set(enabled && system.as_bool()));
}

/// Kullanıcının seçtiği vurgu rengi; arayüzün her yerinde bu kullanılır.
pub fn accent() -> Rgb {
    let raw=CURRENT_ACCENT.with(|c| c.get());
    if is_dark(){return raw;}
    match raw {0x0A84FF=>0x0066CC,0x5FD39A=>0x137A48,0xFFA45C=>0xAA4E00,0xF28AC9=>0xAA2D75,0x4FD1D9=>0x007878,_=>0x6D36C9}
}

/// Ayar adından ("blue" vb.) vurgu rengini uygular; bilinmeyen ad mor olur.
pub fn set_accent(name: &str) {
    let rgb = ACCENTS.iter().find(|(n, ..)| *n == name).map_or(DEFAULT_ACCENT, |(_, _, c)| *c);
    CURRENT_ACCENT.with(|c| c.set(rgb));
}

/// "#RRGGBB" biçimi (iç sayfaların CSS'i için).
pub fn hex(rgb: Rgb) -> String {
    format!("#{rgb:06X}")
}

/// Win32 COLORREF biçimi (0x00BBGGRR).
pub const fn colorref(rgb: Rgb) -> u32 {
    ((rgb & 0xFF) << 16) | (rgb & 0xFF00) | ((rgb >> 16) & 0xFF)
}

// Ölçüler DIP cinsinden (96 DPI'da piksel).
// Safari'nin klasik düzeni: üstte araç çubuğu (ortada adres), altında sekmeler.
pub const TOOL_ROW: f32 = 52.0;
pub const TAB_ROW: f32 = 34.0;
pub const CHROME: f32 = TOOL_ROW + TAB_ROW;
pub const GUTTER: f32 = 0.0;
pub const CAPTION_BTN: f32 = 46.0;
pub const CAPTION_H: f32 = TOOL_ROW;
pub const TAB_MIN: f32 = 44.0;
pub const RESIZE_EDGE: f32 = 6.0;
/// Odak modunda üstte kalan, fareyi yakalayan şerit.
pub const FOCUS_STRIP: f32 = 5.0;
pub const MENU_WIDTH: f32 = 272.0;

// Animasyon süreleri (ms).
pub const TAB_OPEN_MS: f32 = 190.0;
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
pub const ICON_SHARE: &str = "\u{E72D}";
pub const ICON_PRIVATE: &str = "\u{E727}";
pub const ICON_HOME: &str = "\u{E80F}";

// ---------- Cam (Acrylic) ----------

thread_local! { static GLASS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }
/// Pencerenin arkasında Windows'un bulanık cam efekti etkin mi?
pub fn glass() -> bool { GLASS.with(|g| g.get()) }
pub fn set_glass(on: bool) { GLASS.with(|g| g.set(on)); }
/// Windows'ta "Saydamlık efektleri" açık mı (Ayarlar > Kişiselleştirme > Renkler)?
pub fn system_transparency() -> bool {
    use windows::{core::*,Win32::System::Registry::*};
    let mut value=1u32;let mut size=4u32;
    unsafe { let _ = RegGetValueW(HKEY_CURRENT_USER,w!(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize"),w!("EnableTransparency"),RRF_RT_REG_DWORD,None,Some(&mut value as *mut _ as _),Some(&mut size)); }
    value!=0
}
/// Camın üstüne sürülen renk ve opaklığı: cam kapalıyken tam opak.
fn tint(rgb: Rgb, alpha: f32) -> (Rgb, f32) { (rgb, if glass() { alpha } else { 1.0 }) }
/// Araç çubuğu: en açık ve en saydam katman.
pub fn toolbar_tint() -> (Rgb, f32) { tint(if is_dark() { raised() } else { 0xF6F6F8 }, if is_dark() { 0.58 } else { 0.62 }) }
/// Sekme şeridi: biraz daha koyu, böylece etkin sekme öne çıkar.
pub fn tabbar_tint() -> (Rgb, f32) { tint(if is_dark() { base() } else { 0xE4E4E9 }, if is_dark() { 0.66 } else { 0.72 }) }
/// Etkin sekme.
pub fn tab_active() -> (Rgb, f32) { tint(if is_dark() { hover() } else { raised() }, if is_dark() { 0.80 } else { 0.92 }) }
/// Menüler ve öneri listesi: buzlu cam.
pub fn menu_tint() -> (Rgb, f32) { tint(raised(), if is_dark() { 0.70 } else { 0.74 }) }
/// Başlangıç sayfasının buzlu zemini.
pub fn page_tint() -> (Rgb, f32) { tint(base(), if is_dark() { 0.72 } else { 0.70 }) }
/// Sistem mavisi: yükleme çizgisi gibi vurgu renginden bağımsız durum göstergeleri.
pub fn system_blue() -> Rgb { if is_dark() { 0x0A84FF } else { 0x007AFF } }
/// Adres alanı: içindeki metin kutusuyla aynı renkte, opak.
pub fn address_field() -> Rgb { palette().field }

/// Yavaşlayarak biten geçiş: 0..1 → 0..1.
pub fn ease_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn luminance(c:Rgb)->f64 {
        let linear=|v:u32|{let s=v as f64/255.0;if s<=0.04045{s/12.92}else{((s+0.055)/1.055).powf(2.4)}};
        0.2126*linear(c>>16)+0.7152*linear((c>>8)&255)+0.0722*linear(c&255)
    }
    fn contrast(a:Rgb,b:Rgb)->f64 {let(a,b)=(luminance(a),luminance(b));(a.max(b)+0.05)/(a.min(b)+0.05)}
    #[test]
    fn themes_have_readable_text_and_system_only_resolves_neutral_palettes() {
        assert_eq!(resolve("system",true).base,LIGHT.base);
        assert_eq!(resolve("system",false).base,DARK.base);
        assert_eq!(resolve("sqywein",true).base,SQYWEIN.base);
        for name in ["light","dark","sqywein"] {
            set_theme(name);let p=palette();
            for fg in [p.ink,p.muted,p.faint] {for bg in [p.base,p.raised]{assert!(contrast(fg,bg)>=4.5,"{name} {fg:x} {bg:x}");}}
            for (a,_,_) in ACCENTS {set_accent(a);assert!(contrast(base(),accent())>=4.5,"{name} {a}");}
        }
        set_accent("purple");set_theme("sqywein");
    }
}
