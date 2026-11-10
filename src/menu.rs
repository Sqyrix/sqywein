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
        Graphics::{Dwm::*, Gdi::{ClientToScreen, InvalidateRect, GetMonitorInfoW, MonitorFromPoint, MonitorFromWindow, MONITOR_DEFAULTTONEAREST, MONITORINFO}},
        System::LibraryLoader::GetModuleHandleW,
        UI::{WindowsAndMessaging::*,Input::KeyboardAndMouse::SetFocus},
    },
};

pub const CLASS: PCWSTR = w!("SqyweinMenu");

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MenuKind {
    Category(Category),
    WebContext,
    Permission,
    Main,
    Profile,
    Extensions,
    Downloads,
    BarOverflow,
    Shield,
    SavePassword,
    TabContext,
    Share,
    Translate,
}

#[derive(Clone,Copy,PartialEq,Eq,Debug)]
pub enum Category { Context(i32), Main, Edit, History, Bookmarks, Reading, Help, Autofill, SaveShare, Tools }

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MenuCmd {
    Context(i32),
    Category(Category),
    NewWindow,
    PrivateWindow,
    FullScreen,
    Edit(u8),
    SavePage,
    ClearData,
    ToggleBookmarksBar,
    BookmarkAll,
    ReopenTab,
    OpenMenuLink(usize),
    OpenReading(usize),
    RemoveReading(usize),
    BookmarkPage,
    OpenLocation,
    CloseTab,
    Back,
    Forward,
    Home,
    Reload,
    ViewSource,
    Minimize,
    Maximize,
    CheckUpdates,
    AddReading,
    Reader,
    About,
    Help,
    ReportIssue,
    TaskManager,
    PermissionAllow,
    PermissionDeny,
    Overview,
    PreviewDownload(u32),
    TranslationOptions,
    TranslationLanguages(bool),
    TranslationLanguagePage(bool, usize),
    TranslationLanguage(bool, usize),
    TranslationAlways,
    TranslationNever,
    TranslationSite,
    TranslationSettings,
    TranslationOriginal,
    TranslationClose,
    Translate,
    OpenShare,
    CopyLink,
    ShareWindows,
    EmailLink,
    FocusMode,
    PictureInPicture,
    NewTab,
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
    SitePermissions,
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
    TabPin,
    TabMute,
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
    Translation { source: String, target: String, translated: bool },
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
    /// Sayfa adresinin QR kodu (kenar, satır satır koyu hücreler).
    Qr { size: usize, cells: Vec<bool> },
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
            Item::Translation { .. } => 48.0,
            Item::Header(_) => 72.0,
            Item::Action { .. } => 34.0,
            Item::Sep => 9.0,
            Item::Zoom => 38.0,
            Item::Download(_) => 62.0,
            Item::Slider { .. } => 136.0,
            Item::Qr { .. } => 214.0,
        }
    }
}

/// Menüdeki bir noktanın karşılığı.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Part {
    FullScreen,
    TranslationSource,
    TranslationClose,
    TranslationTarget,
    TranslationOptions,
    Level(u8),
    Row(usize),
    Side(usize),
    Minus,
    Plus,
}

const PAD: f32 = 6.0;
const SIDE_W: f32 = 84.0;

pub struct Menu {
    accessibility: Option<crate::accessibility::Tree>,
    pub hwnd: HWND,
    pub kind: MenuKind,
    gfx: Gfx,
    items: Vec<Item>,
    hover: Option<Part>,
    /// Yakınlaştırma oranı (1.0 = %100); sayfa yoksa `None`.
    pub zoom: Option<f64>,
    dpi: u32,
    size: (u32, u32),
    /// Açılış animasyonu: başladığı an ve hedef konum (ekran pikseli).
    fade: Option<(std::time::Instant, POINT)>,
    alpha: u8,
    exit_alpha: u8,
    exiting: bool,
    /// Kaydırıcı sürüklenirken düğmenin x konumu (DIP).
    drag: Option<f32>,
    hold: Option<Part>,
    scroll: f32,
}

const FADE_MS: f32 = 160.0;
const FADE_TIMER: usize = 1;

/// Profil rengi "#AF85FC" → 0xAF85FC.
pub fn parse_color(hex: &str) -> Rgb {
    u32::from_str_radix(hex.trim_start_matches('#'), 16).unwrap_or(accent())
}

/// Yuvarlak avatar: fotoğraf varsa o, yoksa renkli daire içinde baş harf.
pub fn avatar(g: &Gfx, r: Rect, photo: Option<&Image>, color: Rgb, name: &str) {
    match photo {
        Some(img) => g.image(img, r, true),
        None => {
            g.circle(r.cx(), r.cy(), r.w / 2.0, color);
            let letter: String = name.chars().next().map(|c| c.to_uppercase().collect()).unwrap_or_default();
            let font = if r.w >= 30.0 { Font::UiBold } else { Font::Small };
            g.text(&letter, r, font, base(), Align::Center);
        }
    }
}

impl Menu {
    pub fn hover_category(&self)->Option<Category>{match self.hover.and_then(|p|self.command(p)){Some(MenuCmd::Category(c))=>Some(c),_=>None}}
    pub fn category_rect(&self,category:Category)->Option<RECT>{
        let row=self.items.iter().zip(self.rows()).find_map(|(item,r)|match item{Item::Action{cmd:MenuCmd::Category(c),..}if *c==category=>Some(r),_=>None})?;
        let scale=self.dpi as f32/96.0;let mut p=POINT::default();unsafe{let _=ClientToScreen(self.hwnd,&mut p);}
        Some(RECT{left:p.x+(row.x*scale)as i32,top:p.y+(row.y*scale)as i32,right:p.x+(row.right()*scale)as i32,bottom:p.y+(row.bottom()*scale)as i32})
    }
    fn accessible_parts(&self)->Vec<(Part,String,Rect,bool)> {
        let mut parts=Vec::new();
        for (i,(item,r)) in self.items.iter().zip(self.rows()).enumerate(){
            match item {
                Item::Action{label,enabled,side,..}=>{parts.push((Part::Row(i),label.clone(),r,*enabled));if side.is_some(){parts.push((Part::Side(i),format!("{label} — diğer işlem"),Rect::new(r.right()-SIDE_W,r.y,SIDE_W,r.h),*enabled));}},
                Item::Translation{..}=>{for (part,label) in [(Part::TranslationSource,"Orijinal dil"),(Part::TranslationTarget,"Çevir"),(Part::TranslationOptions,"Çeviri seçenekleri"),(Part::TranslationClose,"Çeviri panelini kapat")]{parts.push((part,label.into(),r,true));}},
                Item::Zoom=>{let(a,_,b)=Self::zoom_buttons(r);parts.push((Part::Minus,"Uzaklaştır".into(),a,self.zoom.is_some()));parts.push((Part::Plus,"Yakınlaştır".into(),b,self.zoom.is_some()));parts.push((Part::FullScreen,"Tam ekran".into(),Rect::new(r.right()-39.0,r.y+5.0,28.0,r.h-10.0),true));},
                Item::Slider{..}=>{for n in 0..=3 {parts.push((Part::Level(n),format!("Reklam engelleme seviyesi {n}"),r,true));}},
                Item::Download(d)=>{parts.push((Part::Row(i),d.line.clone(),r,d.done));parts.push((Part::Side(i),if d.done{"Klasörde göster"}else{"İndirmeyi iptal et"}.into(),r,true));},
                _=>{},
            }
        }
        parts
    }
    fn refresh_accessibility(&self){
        let Some(tree)=&self.accessibility else{return};let mut origin=POINT::default();unsafe{let _=ClientToScreen(self.hwnd,&mut origin);}
        let scale=self.dpi as f32/96.0;
        let parts=self.accessible_parts();
        let nodes=parts.iter().enumerate().map(|(i,(part,name,r,enabled))|crate::accessibility::Node{key:i as u32+1,name:name.clone(),rect:RECT{left:origin.x+(r.x*scale) as i32,top:origin.y+(r.y*scale) as i32,right:origin.x+(r.right()*scale) as i32,bottom:origin.y+(r.bottom()*scale) as i32},tab:false,selected:false,enabled:*enabled && !self.exiting && self.command(*part).is_some()}).collect();
        let heading=self.items.iter().filter_map(|i|if let Item::Header(h)=i{Some(format!("{} — {}",h.title,h.sub))}else{None}).collect::<Vec<_>>().join(" ");
        let focus=parts.iter().position(|(p,_,_,_)|Some(*p)==self.hover).map(|i|i as u32+1);
        tree.update(&format!("Sqywein menüsü {heading}"),nodes,focus);
    }
    pub fn accessibility_root(&mut self,wparam:WPARAM,lparam:LPARAM)->LRESULT{
        self.accessibility.get_or_insert_with(Default::default);self.refresh_accessibility();
        let root=self.accessibility.as_ref().unwrap().root(self.hwnd);
        unsafe{windows::Win32::UI::Accessibility::UiaReturnRawElementProvider(self.hwnd,wparam,lparam,&root)}
    }
    pub fn accessibility_invoke(&self,key:u32)->Option<MenuCmd>{
        if self.exiting{return None}let (part,_,_,enabled)=self.accessible_parts().get(key.checked_sub(1)? as usize)?.clone();if enabled{self.command(part)}else{None}
    }
    pub fn accessibility_focus(&mut self,key:u32){
        if let Some((part,_,_,_))=self.accessible_parts().get(key.saturating_sub(1) as usize){self.set_hover(Some(*part));unsafe{let _=SetFocus(Some(self.hwnd));let _=InvalidateRect(Some(self.hwnd),None,false);}self.refresh_accessibility();if let Some(tree)=&self.accessibility{tree.focus_event(self.hwnd,key);}}
    }
    /// `anchor`: menünün sağ üst köşesi (ekran pikseli).
    pub fn open(owner: HWND, kind: MenuKind, anchor: POINT, dpi: u32, items: Vec<Item>, zoom: Option<f64>, hardware: bool) -> Result<Self> {
        let s = dpi as f32 / 96.0;
        let h: f32 = items.iter().map(Item::height).sum::<f32>() + 2.0 * PAD;
        let width=if matches!(kind,MenuKind::Translate|MenuKind::Permission){380.0}else if matches!(kind,MenuKind::Main|MenuKind::Category(_)|MenuKind::WebContext){320.0}else{MENU_WIDTH};
        let (mut wpx, mut hpx) = ((width*s).round() as i32, (h * s).round() as i32);
        let mut info = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        unsafe { let _ = GetMonitorInfoW(MonitorFromPoint(anchor, MONITOR_DEFAULTTONEAREST), &mut info); }
        let work = info.rcWork;
        hpx = hpx.min((work.bottom - work.top - 16).max(100));
        wpx = wpx.min((work.right - work.left - 16).max(100));
        let anchor = POINT { x: anchor.x.clamp(work.left + wpx, work.right), y: anchor.y.clamp(work.top, work.bottom - hpx) };
        unsafe {
            let instance: HINSTANCE = GetModuleHandleW(None)?.into();
            // Katmanlı pencere: açılırken saydamlıktan netliğe geçer.
            // Camda katmanlı pencere kullanılamaz (Windows camı çizmez): o zaman yalnızca kayarak açılır.
            // Menüler opaktır: yazılar ClearType ile tam keskin çizilir (camda ClearType çalışmaz).
            let glass = false;
            let hwnd = CreateWindowExW(
                WS_EX_TOOLWINDOW | if glass { WINDOW_EX_STYLE(0) } else { WS_EX_LAYERED },
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
            let dark: i32 = i32::from(is_dark());
            let _ = DwmSetWindowAttribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE, &dark as *const _ as _, 4);
            let corner = DWMWCP_ROUND;
            let _ = DwmSetWindowAttribute(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE, &corner as *const _ as _, 4);
            let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), if motion_enabled() { 0 } else { 255 }, LWA_ALPHA);
            let target = POINT { x: anchor.x - wpx, y: anchor.y };
            let _ = SetWindowPos(hwnd, None, target.x, target.y - if motion_enabled() { (5.0 * s) as i32 } else { 0 }, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
            if motion_enabled() { SetTimer(Some(hwnd), FADE_TIMER, 16, None); }
            let mut gfx = Gfx::new()?;
            gfx.hardware = hardware;
            gfx.set_dpi(dpi);
            if glass && crate::app::apply_backdrop(hwnd, true) {
                gfx.set_glass(true);
            }
            Ok(Self { accessibility:None, hwnd, kind, gfx, items, hover: None, zoom, dpi, size: (wpx as u32, hpx as u32), fade: if motion_enabled() { Some((std::time::Instant::now(), target)) } else { None }, alpha: if motion_enabled() { 0 } else { 255 }, exit_alpha: 255, exiting: false, drag: None, hold: None, scroll: 0.0 })
        }
    }

    fn rows(&self) -> Vec<Rect> {
        let mut y = PAD - self.scroll;
        self.items
            .iter()
            .map(|it| {
                let r = Rect::new(0.0, y, self.size.0 as f32 * 96.0 / self.dpi as f32, it.height());
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
        let plus = Rect::new(row.right() - 48.0 - 28.0, row.y + 5.0, 28.0, row.h - 10.0);
        let value = Rect::new(plus.x - 52.0, plus.y, 52.0, plus.h);
        let minus = Rect::new(value.x - 28.0, plus.y, 28.0, plus.h);
        (minus, value, plus)
    }

    fn point(&self, lparam: LPARAM) -> (f32, f32) {
        let s = self.dpi as f32 / 96.0;
        ((lparam.0 & 0xFFFF) as i16 as f32 / s, ((lparam.0 >> 16) & 0xFFFF) as i16 as f32 / s)
    }

    /// Kaydırıcının yolu (x0, x1) ve seçili seviyesi; menüde kaydırıcı yoksa `None`.
    fn slider(&self) -> Option<(f32, f32, u8)> {
        self.items.iter().zip(self.rows()).find_map(|(item, r)| match item {
            Item::Slider { level, .. } => { let (x0, x1, _) = Self::slider_track(r); Some((x0, x1, *level)) }
            _ => None,
        })
    }

    /// Basılan yer kaydırıcıdaysa sürükleme başlar.
    pub fn begin_drag(&mut self, lparam: LPARAM) -> bool {
        if !matches!(self.hit(lparam), Some(Part::Level(_))) { return false; }
        let Some((x0, x1, _)) = self.slider() else { return false };
        self.drag = Some(self.point(lparam).0.clamp(x0, x1));
        true
    }

    pub fn dragging(&self) -> bool { self.drag.is_some() }

    pub fn drag_to(&mut self, lparam: LPARAM) {
        if let (Some(_), Some((x0, x1, _))) = (self.drag, self.slider()) {
            self.drag = Some(self.point(lparam).0.clamp(x0, x1));
        }
    }

    /// Sürükleme biter; bırakılan yere en yakın seviye döner.
    pub fn end_drag(&mut self) -> Option<u8> {
        let x = self.drag.take()?;
        let (x0, x1, _) = self.slider()?;
        Some((((x - x0) / ((x1 - x0) / 3.0)).round()).clamp(0.0, 3.0) as u8)
    }

    pub fn hit(&self, lparam: LPARAM) -> Option<Part> {
        let (x, y) = self.point(lparam);
        for (i, (item, r)) in self.items.iter().zip(self.rows()).enumerate() {
            if !r.contains(x, y) {
                continue;
            }
            return match item {
                Item::Translation { .. } => Some(if x > r.right() - 40.0 { Part::TranslationClose } else if x > r.right() - 80.0 { Part::TranslationOptions } else if x < r.cx() - 36.0 { Part::TranslationSource } else { Part::TranslationTarget }),
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
                Item::Zoom => {
                    if Rect::new(r.right()-39.0,r.y+5.0,28.0,r.h-10.0).contains(x,y){return Some(Part::FullScreen)}
                    self.zoom?;
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

    pub fn scroll_by(&mut self, delta: f32) {
        self.cancel_hold(); self.hover = None;
        let content = self.items.iter().map(Item::height).sum::<f32>() + 2.0 * PAD;
        let visible = self.size.1 as f32 * 96.0 / self.dpi as f32;
        self.scroll = (self.scroll + delta).clamp(0.0, (content - visible).max(0.0));
        unsafe { let _ = InvalidateRect(Some(self.hwnd), None, false); }
    }
    pub fn begin_hold(&mut self, lparam: LPARAM) {
        self.hold = self.hit(lparam).filter(|p| matches!(p, Part::Row(i) if matches!(self.items.get(*i), Some(Item::Download(d)) if d.done)));
        if self.hold.is_some() { unsafe { SetTimer(Some(self.hwnd), 2, 500, None); } }
    }
    pub fn cancel_hold_if_moved(&mut self, part: Option<Part>) { if self.hold != part { self.cancel_hold(); } }
    pub fn cancel_hold(&mut self) { self.hold = None; unsafe { let _ = KillTimer(Some(self.hwnd), 2); } }
    pub fn hold_command(&mut self) -> Option<MenuCmd> {
        let part = self.hold.take(); self.cancel_hold();
        match part { Some(Part::Row(i)) => match self.items.get(i) { Some(Item::Download(d)) if d.done => Some(MenuCmd::PreviewDownload(d.id)), _ => None }, _ => None }
    }
    pub fn keyboard(&mut self, key: u32) -> Option<MenuCmd> {
        if key==0x27{if let Some(c)=self.hover_category(){return Some(MenuCmd::Category(c))}}
        if key==0x27{if let Some(MenuCmd::Category(category))=self.hover.and_then(|p|self.command(p)){return Some(MenuCmd::Category(category))}}
        let parts: Vec<Part> = self.items.iter().enumerate().flat_map(|(i, item)| match item {
            Item::Translation { .. } => vec![Part::TranslationSource, Part::TranslationTarget, Part::TranslationOptions, Part::TranslationClose],
            Item::Action { enabled: true, .. } => vec![Part::Row(i)],
            Item::Zoom=>if self.zoom.is_some(){vec![Part::Minus,Part::Plus,Part::FullScreen]}else{vec![Part::FullScreen]},
            Item::Download(d) if d.done => vec![Part::Row(i), Part::Side(i)],
            _ => vec![],
        }).collect();
        if key == 0x20 && !crate::keys::modifiers().ctrl {
            if let Some(Part::Row(i)) = self.hover { if let Some(Item::Download(d)) = self.items.get(i) { if d.done { return Some(MenuCmd::PreviewDownload(d.id)); } } }
        }
        if key == 0x0D { return self.hover.and_then(|p| self.command(p)); }
        if parts.is_empty() || ![0x26, 0x28, 0x09, 0x25, 0x27].contains(&key) { return None; }
        let back = key == 0x26 || key == 0x25;
        let index = self.hover.and_then(|p| parts.iter().position(|v| *v == p));
        let next = match index { Some(i) if back => (i + parts.len() - 1) % parts.len(), Some(i) => (i + 1) % parts.len(), None => if back { parts.len() - 1 } else { 0 } };
        let part = parts[next];
        let row = match part { Part::Row(i) | Part::Side(i) => self.rows().get(i).copied(), _ => self.rows().first().copied() };
        if let Some(row) = row {
            let visible = self.size.1 as f32 * 96.0 / self.dpi as f32;
            if row.y < PAD { self.scroll_by(row.y - PAD); }
            else if row.bottom() > visible - PAD { self.scroll_by(row.bottom() - visible + PAD); }
        }
        self.set_hover(Some(part)); unsafe { let _ = InvalidateRect(Some(self.hwnd), None, false); } None
    }
    pub fn translation_summary(&self) -> bool { self.items.len() == 4 && matches!(self.items.first(), Some(Item::Translation { .. })) }

    /// Fare hareketi; çizim gerekiyorsa `true`.
    pub fn set_hover(&mut self, part: Option<Part>) -> bool {
        let changed = self.hover != part;
        self.hover = part;
        changed
    }

    pub fn command(&self, part: Part) -> Option<MenuCmd> {
        match part {
            Part::FullScreen=>Some(MenuCmd::FullScreen),
            Part::TranslationClose => Some(MenuCmd::TranslationClose),
            Part::TranslationSource => Some(MenuCmd::TranslationOriginal),
            Part::TranslationTarget => Some(MenuCmd::Translate),
            Part::TranslationOptions => Some(MenuCmd::TranslationOptions),
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

    /// Açılış animasyonunun bir adımı (menü zamanlayıcısından çağrılır).
    pub fn fade_step(&mut self) {
        let Some((start, target)) = self.fade else { return };
        let duration = if self.exiting { 90.0 } else { FADE_MS };
        let t = if motion_enabled() { (start.elapsed().as_secs_f32() * 1000.0 / duration).min(1.0) } else { 1.0 };
        let e = ease_out(t);
        let s = self.dpi as f32 / 96.0;
        unsafe {
            self.alpha = if self.exiting { ((1.0 - e) * self.exit_alpha as f32) as u8 } else { (e * 255.0) as u8 };
            let _ = SetLayeredWindowAttributes(self.hwnd, COLORREF(0), self.alpha, LWA_ALPHA);
            let y = if self.exiting { target.y + (e * 3.0 * s) as i32 } else { target.y - ((1.0 - e) * 5.0 * s) as i32 };
            let _ = SetWindowPos(self.hwnd, None, target.x, y, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
            if t >= 1.0 {
                let _ = KillTimer(Some(self.hwnd), FADE_TIMER);
                self.fade = None;
            }
        }
    }

    pub fn start_exit(&mut self) -> bool {
        if !motion_enabled() { return false; }
        let mut rect = RECT::default();
        unsafe {
            if GetWindowRect(self.hwnd, &mut rect).is_err() { return false; }
            self.exiting = true;
            self.exit_alpha = self.alpha;
            self.fade = Some((std::time::Instant::now(), POINT { x: rect.left, y: rect.top }));
            SetTimer(Some(self.hwnd), FADE_TIMER, 16, None);
        }
        true
    }
    pub fn exit_finished(&self) -> bool { self.exiting && self.fade.is_none() }

    /// Menü açıkken içeriği değişti (ör. uzantı sabitlendi).
    pub fn set_items(&mut self, items: Vec<Item>) {
        self.cancel_hold();
        self.hover = None;
        self.items = items;
        self.scroll = 0.0;
        // Satır sayısı değiştiyse pencere ve çizim yüzeyi yeni boya göre ayarlanır.
        let s = self.dpi as f32 / 96.0;
        let h: f32 = self.items.iter().map(Item::height).sum::<f32>() + 2.0 * PAD;
        let mut info = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        unsafe { let _ = GetMonitorInfoW(MonitorFromWindow(self.hwnd, MONITOR_DEFAULTTONEAREST), &mut info); }
        let hpx = ((h * s).round() as u32).min((info.rcWork.bottom - info.rcWork.top - 16).max(100) as u32);
        if hpx != self.size.1 {
            self.size.1 = hpx;
            self.gfx.resize(self.size.0, hpx);
            unsafe {
                let mut rect = RECT::default(); let _ = GetWindowRect(self.hwnd, &mut rect);
                let y = rect.top.min(info.rcWork.bottom - hpx as i32).max(info.rcWork.top);
                if let Some((_, target)) = &mut self.fade { target.y = y; }
                let _ = SetWindowPos(self.hwnd, None, rect.left, y, self.size.0 as i32, hpx as i32, SWP_NOZORDER | SWP_NOACTIVATE);
            }
        }
        unsafe { let _ = InvalidateRect(Some(self.hwnd), None, false); }
    }

    pub fn paint(&mut self) {
        self.refresh_accessibility();
        let (w, h) = self.size;
        if !self.gfx.begin(self.hwnd, w, h) {
            return;
        }
        let g = &self.gfx;
        let full = Rect::new(0.0, 0.0, w as f32 * 96.0 / self.dpi as f32, h as f32 * 96.0 / self.dpi as f32);
        if self.gfx.is_glass() {
            g.clear_transparent();
            let (c, a) = crate::theme::menu_tint();
            g.fill_alpha(full, c, a);
        } else {
            g.clear(raised());
        }
        g.round_outline_alpha(full, 11.0, line(), 1.0, 0.8);

        for (i, (item, r)) in self.items.iter().zip(self.rows()).enumerate() {
            match item {
                Item::Translation { source, target, translated } => {
                    let width = (r.w - 88.0) / 2.0;
                    for (j, label) in [source, target].iter().enumerate() {
                        let tab = Rect::new(r.x + 8.0 + j as f32 * width, r.y + 4.0, width, r.h - 8.0);
                        let selected = *translated == (j == 1);
                        if self.hover == Some(if j == 0 { Part::TranslationSource } else { Part::TranslationTarget }) { g.round(tab.inset(2.0, 3.0), 6.0, hover()); }
                        g.text(label, tab, Font::UiBold, if selected { accent() } else { muted() }, Align::Center);
                        if selected { g.fill(Rect::new(tab.x + 8.0, tab.bottom() - 2.0, tab.w - 16.0, 2.0), accent()); }
                    }
                    for (part, x) in [(Part::TranslationOptions, r.right() - 76.0), (Part::TranslationClose, r.right() - 40.0)] { if self.hover == Some(part) { g.round(Rect::new(x, r.y + 8.0, 32.0, 32.0), 6.0, hover()); } }
                    for dx in [-5.0, 0.0, 5.0] { g.circle(r.right() - 60.0 + dx, r.cy(), 1.4, ink()); }
                    g.text(ICON_CLOSE, Rect::new(r.right() - 40.0, r.y, 32.0, r.h), Font::SmallIcon, ink(), Align::Center);
                }
                Item::Header(head) => {
                    let card = r.inset(8.0, 4.0);
                    g.round(card, 10.0, hover());
                    avatar(g, Rect::new(card.x + 12.0, card.cy() - 20.0, 40.0, 40.0), head.avatar.as_deref(), head.color, &head.title);
                    g.text(&head.title, Rect::new(card.x + 64.0, card.y + 12.0, card.w - 74.0, 20.0), Font::UiBold, ink(), Align::Left);
                    g.text(&head.sub, Rect::new(card.x + 64.0, card.y + 33.0, card.w - 74.0, 18.0), Font::Small, muted(), Align::Left);
                }
                Item::Action { label, hint, enabled, side, icon, cmd } => {
                    let row_hover = self.hover == Some(Part::Row(i));
                    let side_hover = self.hover == Some(Part::Side(i));
                    if *enabled && (row_hover || side_hover) {
                        g.round(r.inset(6.0, 1.0), 7.0, hover());
                    }
                    let mut x = r.x + 16.0;
                    if let Some(img) = icon {
                        g.image(img, Rect::new(x, r.cy() - 8.0, 16.0, 16.0), false);
                        x += 26.0;
                    }
                    let glyph=match cmd{
                        MenuCmd::NewTab=>Some(ICON_ADD),MenuCmd::History=>Some("\u{E81C}"),MenuCmd::Downloads=>Some(ICON_DOWNLOAD),MenuCmd::Passwords=>Some("\u{E8D7}"),MenuCmd::Settings=>Some("\u{E713}"),MenuCmd::Print=>Some("\u{E749}"),MenuCmd::Find=>Some(ICON_SEARCH),MenuCmd::Reader=>Some("\u{E736}"),MenuCmd::Context(_)=>{let label=label.to_lowercase();if label.starts_with("geri"){Some(ICON_BACK)}else if label.starts_with("yenile"){Some(ICON_RELOAD)}else if label.contains("kopyala"){Some("\u{E8C8}")}else if label.contains("yapıştır"){Some("\u{E77F}")}else if label.starts_with("yazdır"){Some("\u{E749}")}else{None}},_=>None};
                    if let Some(glyph)=glyph.filter(|_|icon.is_none()){g.text(glyph,Rect::new(x,r.cy()-8.0,16.0,16.0),Font::Icon,if *enabled{muted()}else{faint()},Align::Center);x+=26.0;}
                    let color = if *enabled { ink() } else { faint() };
                    let category=matches!(cmd,MenuCmd::Category(_));
                    let reserve = if category{32.0}else if hint.is_empty() && side.is_none() { 16.0 } else { SIDE_W + 8.0 };
                    g.text(label, Rect::new(x, r.y, r.right() - x - reserve, r.h), Font::Ui, color, Align::Left);
                    let hint_r = Rect::new(r.right() - SIDE_W - 14.0, r.y, SIDE_W, r.h);
                    if side.is_some() && side_hover {
                        g.round(hint_r.inset(0.0, 5.0), 6.0, line());
                    }
                    g.text(if category{"\u{E76C}"}else{hint}, hint_r.inset(6.0, 0.0), if category{Font::SmallIcon}else{Font::Small}, if side.is_some() { muted() } else { faint() }, Align::Right);
                }
                Item::Sep => g.fill(Rect::new(r.x + 10.0, r.cy(), r.w - 20.0, 1.0), line()),
                Item::Slider { title, level } => {
                    g.text(title, Rect::new(r.x + 12.0, r.y + 8.0, r.w - 24.0, 24.0), Font::UiBold, ink(), Align::Center);
                    let (x0, x1, cy) = Self::slider_track(r);
                    let step = (x1 - x0) / 3.0;
                    let at = |n: u8| x0 + n as f32 * step;
                    // Apple tarzı: ince yol, dolu kısım vurgu renginde, beyaz yuvarlak düğme.
                    let knob_x = self.drag.unwrap_or(at(*level));
                    let shown = (((knob_x - x0) / step).round()).clamp(0.0, 3.0) as u8;
                    g.round(Rect::new(x0 - 3.0, cy - 3.0, x1 - x0 + 6.0, 6.0), 3.0, line());
                    g.round(Rect::new(x0 - 3.0, cy - 3.0, knob_x - x0 + 6.0, 6.0), 3.0, accent());
                    for n in 0..=3u8 {
                        let filled = at(n) <= knob_x + 0.5;
                        g.circle(at(n), cy, 4.0, if filled { accent() } else { line() });
                        if self.hover == Some(Part::Level(n)) && self.drag.is_none() && n != *level {
                            g.round_alpha(Rect::new(at(n) - 14.0, cy - 14.0, 28.0, 28.0), 14.0, hover(), 0.8);
                        }
                    }
                    let grow = if self.drag.is_some() { 2.0 } else { 0.0 };
                    g.glow(knob_x, cy + 2.0, 20.0, 20.0, 0x000000, 0.35);
                    g.circle(knob_x, cy, 13.0 + grow, 0xFFFFFF);
                    g.text("filtreleme modu", Rect::new(r.x, cy + 28.0, r.w, 18.0), Font::Small, muted(), Align::Center);
                    let name = crate::adblock::LEVEL_NAMES[shown as usize];
                    g.text(name, Rect::new(r.x, cy + 46.0, r.w, 20.0), Font::UiBold, accent(), Align::Center);
                }
                Item::Qr { size, cells } => {
                    // Telefon kamerasıyla okunur: beyaz kart üzerinde siyah hücreler.
                    let card = Rect::new(r.cx() - 98.0, r.y + 8.0, 196.0, 196.0);
                    g.round(card, 14.0, 0xFFFFFF);
                    let cell = 168.0 / *size as f32;
                    let (x0, y0) = (card.x + 14.0, card.y + 14.0);
                    for (k, dark) in cells.iter().enumerate() {
                        if *dark {
                            let (cx, cy) = ((k % size) as f32, (k / size) as f32);
                            g.fill(Rect::new(x0 + cx * cell, y0 + cy * cell, cell + 0.35, cell + 0.35), 0x000000);
                        }
                    }
                }
                Item::Download(d) => {
                    if self.hover == Some(Part::Row(i)) && d.done {
                        g.round(r.inset(6.0, 2.0), 8.0, hover());
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
                            g.round(icon, 8.0, if d.done { accent() } else { hover() });
                            g.text(ICON_DOC, icon, Font::Icon, if d.done { base() } else { ink() }, Align::Center);
                        }
                    }
                    let tx = icon.right() + 12.0;
                    let tw = r.right() - tx - 58.0;
                    g.text(&d.name, Rect::new(tx, r.y + 9.0, tw, 20.0), Font::UiBold, ink(), Align::Left);
                    g.text(&d.line, Rect::new(tx, r.y + 29.0, tw, 18.0), Font::Small, muted(), Align::Left);
                    if let Some(p) = d.progress {
                        let bar = Rect::new(tx, r.y + 50.0, tw, 3.0);
                        g.round(bar, 1.5, line());
                        g.round(Rect::new(bar.x, bar.y, (bar.w * p.clamp(0.0, 1.0)).max(3.0), bar.h), 1.5, accent());
                    }
                    let side = Rect::new(r.right() - 48.0, r.cy() - 16.0, 32.0, 32.0);
                    if self.hover == Some(Part::Side(i)) {
                        g.round(side, 8.0, line());
                    }
                    if d.done {
                        g.text(ICON_FOLDER, side, Font::Icon, ink(), Align::Center);
                    } else if d.progress.is_some() {
                        g.text(ICON_CLOSE, side, Font::SmallIcon, muted(), Align::Center);
                    }
                }
                Item::Zoom => {
                    let enabled = self.zoom.is_some();
                    g.text("Yakınlaştır", Rect::new(r.x + 16.0, r.y, 120.0, r.h), Font::Ui, if enabled { ink() } else { faint() }, Align::Left);
                    let (minus, value, plus) = Self::zoom_buttons(r);
                    let full=Rect::new(r.right()-39.0,r.y+5.0,28.0,r.h-10.0);if self.hover==Some(Part::FullScreen){g.round(full,6.0,hover());}g.text("\u{E740}",full,Font::SmallIcon,ink(),Align::Center);
                    for (b, part, icon) in [(minus, Part::Minus, ICON_REMOVE), (plus, Part::Plus, ICON_ADD)] {
                        if enabled && self.hover == Some(part) {
                            g.round(b, 6.0, hover());
                        }
                        g.text(icon, b, Font::MidIcon, if enabled { ink() } else { faint() }, Align::Center);
                    }
                    let pct = self.zoom.map(|z| format!("%{}", (z * 100.0).round() as i32)).unwrap_or_else(|| "—".into());
                    g.text(&pct, value, Font::Ui, if enabled { ink() } else { faint() }, Align::Center);
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
        assert_eq!(parse_color("bozuk"), accent());
    }
}
