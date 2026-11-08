//! Ana sayfa (logo, arama, kısayol kutucukları), kısayol ekleme penceresi ve
//! yer işaretleri çubuğu.

use super::*;

pub(super) const BAR_H: f32 = 30.0;
pub(super) const DIALOG_CLASS: PCWSTR = w!("SqyweinDialog");
const TILE_W: f32 = 104.0;
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
}

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
        self.full_screen.is_none() && self.state.settings.show_bookmarks_bar && !self.state.favorites.is_empty()
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

    pub(super) fn start_layout(&self, content: Rect) -> StartLayout {
        let n = self.state.shortcuts.len();
        let count = n.min(MAX_TILES) + usize::from(n < MAX_TILES);
        let cols = count.clamp(1, 5);
        let rows = count.div_ceil(5);
        let block = 72.0 + 6.0 + 40.0 + 34.0 + 54.0 + 30.0 + rows as f32 * TILE_H;
        let top = (content.cy() - block / 2.0).max(content.y + 24.0);
        let cx = content.cx();
        let logo = Rect::new(cx - 36.0, top, 72.0, 72.0);
        let title = Rect::new(content.x, logo.bottom() + 6.0, content.w, 40.0);
        let search_w = (content.w - 64.0).clamp(120.0, 600.0);
        let search = Rect::new(cx - search_w / 2.0, title.bottom() + 34.0, search_w, 54.0);
        let mut tiles = Vec::new();
        let y0 = search.bottom() + 30.0;
        for k in 0..count {
            let (row, col) = (k / 5, k % 5);
            let in_row = if row + 1 == rows { count - row * 5 } else { 5 };
            let x0 = cx - in_row as f32 * TILE_W / 2.0;
            tiles.push(Rect::new(x0 + col as f32 * TILE_W, y0 + row as f32 * TILE_H, TILE_W, TILE_H));
        }
        let add = (n < MAX_TILES).then(|| tiles.pop()).flatten();
        let _ = cols;
        StartLayout { logo, title, search, tiles, add }
    }

    pub(super) fn paint_start(&self, l: &Layout) {
        let g = &self.gfx;
        let private = self.tabs[self.active].private;
        let s = self.start_layout(l.content);
        // Zemin camdır (paint_content buzlu katmanı çizer).
        g.glow(l.content.cx(), s.logo.cy() + 20.0, 280.0, 120.0, accent(), if private { 0.16 } else { 0.10 });
        // Sekmeler uyurken kedi de uyur: Sqywein'in hafifliğinin görünür hali.
        match (&self.sleeping_mascot, &self.logo) {
            (Some(cat), _) if self.asleep_count > 0 && !private => {
                let h = 70.0;
                let w = h * cat.width as f32 / cat.height as f32;
                let r = Rect::new(s.logo.cx() - w / 2.0, s.logo.bottom() - h, w, h);
                g.image(cat, r, false);
                for (k, (dx, dy, size)) in [(0.66, -0.05, 10.0), (0.76, -0.22, 13.0), (0.88, -0.42, 16.0)].iter().enumerate() {
                    let z = Rect::new(r.x + r.w * dx, r.y + r.h * dy, 24.0, 24.0);
                    let _ = k;
                    g.text("z", z, if *size > 12.0 { Font::UiBold } else { Font::Small }, accent(), Align::Left);
                }
            }
            (_, Some(img)) => g.image(img, s.logo, false),
            _ => g.circle(s.logo.cx(), s.logo.cy(), 30.0, accent()),
        }
        g.text(if private { "Gizli sekme" } else { "Sqywein" }, s.title, Font::Big, ink(), Align::Center);
        if !private && !self.ram_line.is_empty() {
            // Hafiflik görünür olsun: şu anki bellek ve uyuyan sekmeler.
            g.text(&self.ram_line, Rect::new(l.content.x, s.title.bottom() + 2.0, l.content.w, 18.0), Font::Small, faint(), Align::Center);
        }

        let r = s.search;
        let focused = unsafe { GetFocus() } == self.start_edit;
        if focused {
            // Odaktayken vurgu renginde yumuşak bir hale.
            g.round_outline_alpha(r.inset(-5.0, -5.0), 31.0, accent(), 5.0, 0.07);
            g.round_outline_alpha(r.inset(-2.5, -2.5), 29.0, accent(), 3.0, 0.14);
        }
        g.round(r, 27.0, search());
        g.round_outline(r, 27.0, if focused { accent() } else { line() }, if focused { 1.5 } else { 1.0 });
        g.text(ICON_SEARCH, Rect::new(r.x + 18.0, r.y, 26.0, r.h), Font::Icon, if focused { accent() } else { muted() }, Align::Center);

        if private {
            g.text("Geçmiş ve çerezler bu sekme kapanınca silinir.", Rect::new(l.content.x, r.bottom() + 24.0, l.content.w, 20.0), Font::Small, faint(), Align::Center);
            return;
        }
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
            self.paint_tile(add, "Kısayol ekle", None, self.hover == Hit::TileAdd, true);
        }
    }

    fn tile_remove(t: Rect) -> Rect {
        Rect::new(t.right() - 26.0, t.y + 4.0, 20.0, 20.0)
    }

    fn paint_tile(&self, t: Rect, label: &str, icon: Option<&Image>, hovered: bool, add: bool) {
        let g = &self.gfx;
        if hovered {
            g.round(t.inset(4.0, 2.0), 12.0, hover());
        }
        let (cx, cy) = (t.cx(), t.y + 34.0);
        g.circle(cx, cy, 26.0, if hovered { line() } else { search() });
        let inner = Rect::new(cx - 12.0, cy - 12.0, 24.0, 24.0);
        if add {
            g.text(ICON_ADD, inner, Font::Icon, ink(), Align::Center);
        } else {
            match icon {
                Some(img) => g.image(img, inner, false),
                None => menu::avatar(g, inner, None, accent(), label),
            }
        }
        g.text(label, Rect::new(t.x + 8.0, t.y + 66.0, t.w - 16.0, 20.0), Font::Small, ink(), Align::Center);
    }

    /// Ana sayfadaki bir noktanın karşılığı.
    pub(super) fn start_hit(&self, content: Rect, x: f32, y: f32) -> Hit {
        if self.tabs[self.active].private {
            return Hit::None;
        }
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
            self.state.shortcuts.remove(i);
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
