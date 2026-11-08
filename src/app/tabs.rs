//! Sekme davranışları: sağ tık menüsü, sürükleyerek sıralama, düğme ipuçları,
//! pencere konumunun hatırlanması, indirme sürerken kapanan sekmeler ve ana
//! sayfadaki bellek satırı.

use super::*;
use windows::Win32::UI::Controls::{
    SetWindowTheme, TOOLTIPS_CLASSW, TTF_SUBCLASS, TTM_ADDTOOLW, TTM_DELTOOLW, TTM_SETMAXTIPWIDTH, TTS_ALWAYSTIP, TTS_NOPREFIX, TTTOOLINFOW,
};

/// Sürüklenen sekme: kimliği, imlecin sekme içindeki yeri, başladığı nokta.
pub(super) struct Drag {
    pub id: u32,
    pub grab: f32,
    pub start: f32,
    pub moving: bool,
}

const TIP_ID: usize = 1;

impl App {
    pub(super) fn request_manual_sleep(&mut self, i: usize) {
        if i == self.active { return; }
        if let Some(t) = self.tabs.get_mut(i) {
            if let Some(v) = &t.view { t.sleep_check = true; t.sleep_discard = true; engine::check_rest(v, t.id, t.epoch, true); }
        }
    }
    // ---------- Sağ tık menüsü ----------

    pub(super) fn open_tab_menu(&mut self, i: usize) {
        let Some(tab) = self.tabs.get(i) else { return };
        self.context_tab = tab.id;
        let mut pt = POINT::default();
        unsafe { let _ = GetCursorPos(&mut pt); }
        // Menü imlecin sağına açılır (menü, verilen noktayı sağ kenar sayar).
        pt.x += (MENU_WIDTH * self.scale()) as i32;
        self.menu_anchor = Some(pt);
        self.open_menu(MenuKind::TabContext);
    }

    pub(super) fn tab_menu_items(&self) -> Vec<Item> {
        let Some(i) = self.index_of(self.context_tab) else { return vec![Item::disabled("Sekme kapandı")] };
        let t = &self.tabs[i];
        let mut sleep = Item::act("Sekmeyi uyut", "", MenuCmd::TabSleep);
        if let Item::Action { enabled, .. } = &mut sleep {
            // Açık sekme uyutulmaz; uyuyan sekme zaten uyuyor.
            *enabled = i != self.active && t.ctrl.is_some();
        }
        let mut right = Item::act("Sağdaki sekmeleri kapat", "", MenuCmd::TabCloseRight);
        if let Item::Action { enabled, .. } = &mut right {
            *enabled = i + 1 < self.tabs.len();
        }
        vec![
            Item::act("Sağına yeni sekme", "", MenuCmd::TabNewRight),
            Item::Sep,
            Item::act("Yeniden yükle", "Ctrl+R", MenuCmd::TabReload),
            Item::act("Çoğalt", "", MenuCmd::TabDuplicate),
            Item::act(if t.pinned { "Sabitlemeyi kaldır" } else { "Sekmeyi sabitle" }, "", MenuCmd::TabPin),
            Item::act(if t.muted { "Sekmenin sesini aç" } else { "Sekmeyi sessize al" }, "", MenuCmd::TabMute),
            sleep,
            Item::act("Diğer sekmeleri uyut", "", MenuCmd::TabSleepOthers),
            Item::Sep,
            Item::act("Diğer sekmeleri kapat", "", MenuCmd::TabCloseOthers),
            right,
            Item::act("Sekmeyi kapat", "Ctrl+W", MenuCmd::TabClose),
        ]
    }

    pub(super) fn tab_command(&mut self, cmd: MenuCmd) {
        let Some(i) = self.index_of(self.context_tab) else { return };
        let id = self.tabs[i].id;
        match cmd {
            MenuCmd::TabMute => {
                self.tabs[i].muted = !self.tabs[i].muted;
                if let Some(v) = &self.tabs[i].view { engine::set_muted(v, self.tabs[i].muted); }
            }
            MenuCmd::TabPin => {
                let active_id = self.tabs[self.active].id;
                self.tabs[i].pinned = !self.tabs[i].pinned;
                self.tabs.sort_by_key(|t| !t.pinned);
                self.active = self.index_of(active_id).unwrap_or(0);
                self.layout_children();
            }
            MenuCmd::TabNewRight => {
                let at = (i + 1).max(self.tabs.iter().filter(|t| t.pinned).count());
                let tab = Tab::new(self.next_id, String::new(), String::new());
                self.next_id += 1;
                self.tabs.insert(at, tab);
                if at <= self.active {
                    self.active += 1;
                }
                self.activate(at);
                self.animate();
            }
            MenuCmd::TabReload => {
                if let Some(v) = &self.tabs[i].view {
                    unsafe { let _ = v.Reload(); }
                } else {
                    self.activate(i);
                }
            }
            MenuCmd::TabDuplicate => {
                let url = self.tabs[i].url.clone();
                let private = self.tabs[i].private;
                self.insert_tab(url, Some(id), private);
            }
            MenuCmd::TabSleep => {
                self.request_manual_sleep(i);
            }
            MenuCmd::TabSleepOthers => {
                for (k, t) in self.tabs.iter_mut().enumerate() {
                    if k != self.active && k != i {
                        if let Some(v) = &t.view { t.sleep_check = true; t.sleep_discard = true; engine::check_rest(v, t.id, t.epoch, true); }
                    }
                }
            }
            MenuCmd::TabCloseOthers => {
                let keep = id;
                while let Some(k) = self.tabs.iter().position(|t| t.id != keep && !t.pinned) {
                    self.close_tab(k);
                }
            }
            MenuCmd::TabCloseRight => {
                while let Some(start) = self.index_of(id) {
                    let Some(k) = (start + 1..self.tabs.len()).find(|k| !self.tabs[*k].pinned) else { break };
                    self.close_tab(k);
                }
            }
            MenuCmd::TabClose => self.close_tab(i),
            _ => {}
        }
        self.dirty = true;
        self.invalidate();
    }

    // ---------- Sürükleyerek sıralama ----------

    pub(super) fn begin_drag(&mut self, i: usize, x: f32) {
        let l = self.layout();
        let Some(r) = self.shown_tabs(&l).get(i).copied() else { return };
        self.drag = Some(Drag { id: self.tabs[i].id, grab: x - r.x, start: x, moving: false });
        unsafe { SetCapture(self.hwnd) };
    }

    /// Fare hareketi sürüklemeye aitse `true`.
    pub(super) fn drag_move(&mut self, x: f32) -> bool {
        let Some(d) = &mut self.drag else { return false };
        if !d.moving && (x - d.start).abs() < 6.0 {
            return true;
        }
        d.moving = true;
        let (id, grab) = (d.id, d.grab);
        let Some(from) = self.index_of(id) else { return true };
        let l = self.layout();
        // İmlecin üstünde durduğu yuva; uçlarda ilk/son sekmeye sabitlenir.
        let to = l.tabs.iter().position(|r| x < r.right()).unwrap_or(l.tabs.len().saturating_sub(1)).min(self.tabs.len() - 1);
        let pins = self.tabs.iter().filter(|t| t.pinned).count();
        let to = if self.tabs[from].pinned { to.min(pins.saturating_sub(1)) } else { to.max(pins) };
        if to != from {
            let tab = self.tabs.remove(from);
            self.tabs.insert(to, tab);
            // Etkin sekme aynı sekme olarak kalır.
            if self.active == from {
                self.active = to;
            } else if from < self.active && to >= self.active {
                self.active -= 1;
            } else if from > self.active && to <= self.active {
                self.active += 1;
            }
            self.dirty = true;
            self.hover = Hit::Tab(to);
            self.previous_hover = Hit::None;
        }
        if let Some(k) = self.index_of(id) {
            // Şeridin içinde kalır: aynı moddaki ilk ve son sekmenin arası.
            let private = self.tabs[k].private;
            let visible: Vec<Rect> = self.tabs.iter().zip(&l.tabs).filter(|(t, _)| t.private == private).map(|(_, r)| *r).collect();
            let min_x = visible.first().map_or(8.0, |r| r.x);
            let max_x = visible.last().map_or(min_x, |r| r.x).max(min_x);
            self.tabs[k].x = (x - grab).clamp(min_x, max_x);
            if k == self.active { self.selection_x = self.tabs[k].x; self.selection_w = self.tabs[k].w; }
        }
        self.animate();
        self.invalidate();
        true
    }

    /// Sürükleme bittiyse `true` (tıklama sayılmaz).
    pub(super) fn end_drag(&mut self) -> bool {
        let Some(d) = self.drag.take() else { return false };
        unsafe { let _ = ReleaseCapture(); }
        if d.moving { self.drop_snap = Some(d.id); }
        self.animate();
        d.moving
    }

    pub(super) fn dragging_id(&self) -> Option<u32> {
        self.drag.as_ref().filter(|d| d.moving).map(|d| d.id)
    }

    // ---------- İpuçları ----------

    pub(super) fn create_tooltip(&mut self) {
        unsafe {
            let Ok(instance) = GetModuleHandleW(None) else { return };
            let Ok(tip) = CreateWindowExW(
                WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
                TOOLTIPS_CLASSW,
                None,
                WS_POPUP | WINDOW_STYLE(TTS_ALWAYSTIP | TTS_NOPREFIX),
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                Some(self.hwnd),
                None,
                Some(instance.into()),
                None,
            ) else {
                return;
            };
            let _ = SetWindowTheme(tip, w!("DarkMode_Explorer"), None);
            SendMessageW(tip, TTM_SETMAXTIPWIDTH, None, Some(LPARAM(420)));
            self.tooltip = tip;
        }
    }

    fn tip_text(&self, hit: Hit) -> Option<String> {
        let tab = self.tabs.get(self.active)?;
        Some(match hit {
            Hit::Back => "Geri (Alt+Sol)".into(),
            Hit::Forward => "İleri (Alt+Sağ)".into(),
            Hit::Reload => if tab.loading { "Durdur".into() } else { "Yenile (F5)".into() },
            Hit::NewTab => "Yeni sekme (Ctrl+T)".into(),
            Hit::Star => if self.is_bookmarked(&tab.url) { "Yer işaretini kaldır".into() } else { "Yer işaretlerine ekle (Ctrl+D)".into() },
            Hit::Shield => "Reklam engelleyici".into(),
            Hit::Update => format!("Sqywein v{} hazır: yeniden başlatıp kurmak için tıkla", self.update_ready.as_ref()?.0),
            Hit::ZoomPill => "Yakınlaştırmayı sıfırla".into(),
            Hit::AddExtension => "Bu uzantıyı Sqywein'e ekle".into(),
            Hit::Downloads => "İndirmeler (Ctrl+J)".into(),
            Hit::Puzzle => "Uzantılar".into(),
            Hit::Profile => {
                let p = self.state.profile();
                if p.email.is_empty() { p.name.clone() } else { format!("{}\n{}", p.name, p.email) }
            }
            Hit::Menu => "Sqywein'i özelleştir ve yönet".into(),
            Hit::Min => "Küçült".into(),
            Hit::Max => if unsafe { IsZoomed(self.hwnd) }.as_bool() { "Geri al".into() } else { "Ekranı kapla".into() },
            Hit::Close => "Kapat".into(),
            Hit::Tab(i) => {
                let t = self.tabs.get(i)?;
                let mut s = t.label();
                if t.pinned { s.push_str(" · Sabitlenmiş"); }
                if t.muted { s.push_str(" · Sessiz"); }
                if !t.url.is_empty() {
                    s.push('\n');
                    s.push_str(&url::display(&t.url));
                }
                if t.ctrl.is_none() && !t.url.is_empty() {
                    s.push_str("\nUyuyor: tıklayınca uyanır");
                }
                s
            }
            Hit::TabClose(_) => "Sekmeyi kapat (Ctrl+W)".into(),
            Hit::Pinned(i) => {
                let id = self.pinned_extensions().get(i)?.clone();
                self.extensions.iter().find(|e| e.id == id)?.name.clone()
            }
            Hit::BarItem(i) => {
                let b = self.state.favorites.get(i)?;
                format!("{}\n{}", b.title, b.url)
            }
            Hit::Tile(i) => self.state.shortcuts.get(i)?.url.clone(),
            Hit::TileRemove(_) => "Kısayolu kaldır".into(),
            Hit::TileAdd => "Kısayol ekle".into(),
            _ => return None,
        })
    }

    fn hit_rect(&self, hit: Hit) -> Option<Rect> {
        let l = self.layout();
        Some(match hit {
            Hit::Back => l.back,
            Hit::Forward => l.forward,
            Hit::Reload => l.reload,
            Hit::NewTab => l.new_tab,
            Hit::Star => l.star,
            Hit::Shield => l.shield?,
            Hit::Update => l.update?,
            Hit::ZoomPill => l.zoom_pill?,
            Hit::AddExtension => l.add_ext?,
            Hit::Downloads => l.downloads?,
            Hit::Puzzle => l.puzzle,
            Hit::Profile => l.profile,
            Hit::Menu => l.menu,
            Hit::Min => l.min,
            Hit::Max => l.max,
            Hit::Close => l.close,
            Hit::Tab(i) | Hit::TabClose(i) => *l.tabs.get(i)?,
            Hit::Pinned(i) => *l.pinned.get(i)?,
            Hit::BarItem(i) => *l.bar_items.get(i)?,
            _ => return None,
        })
    }

    /// Fare yeni bir öğenin üstüne geldi: ipucu o öğeye taşınır.
    pub(super) fn update_tooltip(&mut self) {
        if self.tooltip.is_invalid() {
            return;
        }
        let tip = self.tooltip;
        let mut info = TTTOOLINFOW {
            cbSize: std::mem::size_of::<TTTOOLINFOW>() as u32,
            uFlags: TTF_SUBCLASS,
            hwnd: self.hwnd,
            uId: TIP_ID,
            ..Default::default()
        };
        unsafe { SendMessageW(tip, TTM_DELTOOLW, None, Some(LPARAM(&info as *const _ as isize))) };
        let hit = self.hover;
        let (Some(text), Some(r)) = (self.tip_text(hit), self.hit_rect(hit).or_else(|| self.start_tile_rect(hit))) else { return };
        self.tooltip_text = wide(&text);
        info.rect = self.px(r);
        info.lpszText = PWSTR(self.tooltip_text.as_ptr() as *mut u16);
        unsafe { SendMessageW(tip, TTM_ADDTOOLW, None, Some(LPARAM(&info as *const _ as isize))) };
    }

    fn start_tile_rect(&self, hit: Hit) -> Option<Rect> {
        let s = self.start_layout(self.layout().content);
        match hit {
            Hit::Tile(i) | Hit::TileRemove(i) => s.tiles.get(i).copied(),
            Hit::TileAdd => s.add,
            _ => None,
        }
    }

    // ---------- Pencere konumu ----------

    /// Kayıtlı pencere konumu ve boyutu (ekranlardan birinin üstündeyse).
    /// Uygulamanın dışında uygulanmalı: pencere boyut mesajları uygulamaya ulaşabilsin.
    pub(super) fn saved_placement(&self) -> Option<WINDOWPLACEMENT> {
        let [left, top, right, bottom, maximized] = self.state.window?;
        let rect = RECT { left, top, right, bottom };
        if right - left < 200 || bottom - top < 150 {
            return None;
        }
        if unsafe { MonitorFromRect(&rect, MONITOR_DEFAULTTONULL) }.is_invalid() {
            return None; // O ekran artık bağlı değil.
        }
        Some(WINDOWPLACEMENT {
            length: std::mem::size_of::<WINDOWPLACEMENT>() as u32,
            showCmd: if maximized != 0 { SW_SHOWMAXIMIZED.0 as u32 } else { SW_SHOWNORMAL.0 as u32 },
            rcNormalPosition: rect,
            ..Default::default()
        })
    }

    pub(super) fn remember_placement(&mut self) {
        if self.full_screen.is_some() {
            return;
        }
        let mut p = WINDOWPLACEMENT { length: std::mem::size_of::<WINDOWPLACEMENT>() as u32, ..Default::default() };
        if unsafe { GetWindowPlacement(self.hwnd, &mut p) }.is_ok() {
            let r = p.rcNormalPosition;
            let maximized = i32::from(unsafe { IsZoomed(self.hwnd) }.as_bool());
            self.state.window = Some([r.left, r.top, r.right, r.bottom, maximized]);
        }
    }

    // ---------- İndirme sürerken kapanan sekmeler ----------

    /// Sekme kapanırken indirmesi sürüyorsa görünüm gizlice yaşatılır; indirme bitince kapanır.
    pub(super) fn keep_for_download(&mut self, tab: &mut Tab) -> bool {
        let busy = self.download_owner.iter().any(|(dl, owner)| *owner == tab.id && self.downloads.iter().any(|(d, _)| d == dl));
        if !busy {
            return false;
        }
        if let Some(ctrl) = tab.ctrl.take() {
            unsafe { let _ = ctrl.SetIsVisible(false); }
            self.lingering.push((tab.id, ctrl));
        }
        tab.view = None;
        true
    }

    /// İndirme bitti: artık indirmesi kalmayan yaşatılmış görünümler kapanır.
    pub(super) fn release_lingering(&mut self) {
        let active: Vec<u32> = self.downloads.iter().filter_map(|(d, _)| self.download_owner.get(d).copied()).collect();
        self.lingering.retain(|(tab, ctrl)| {
            if active.contains(tab) {
                true
            } else {
                unsafe { let _ = ctrl.Close(); }
                false
            }
        });
    }

    /// Yalnızca indirme için açılan sekme (sayfa hiç yüklenmeden indirmeye döndü) kapatılır.
    pub(super) fn close_download_only_tab(&mut self, tab_id: u32) {
        let Some(i) = self.index_of(tab_id) else { return };
        if self.tabs[i].opener.is_some() && !self.tabs[i].has_loaded {
            self.close_tab(i);
        }
    }

    // ---------- Ana sayfadaki bellek satırı ----------

    pub(super) fn update_ram_line(&mut self) {
        if !self.on_start_page() || unsafe { IsIconic(self.hwnd) }.as_bool() {
            return;
        }
        let mut pids = vec![unsafe { windows::Win32::System::Threading::GetCurrentProcessId() }];
        if let Some(env) = &self.env {
            pids.extend(engine::process_ids(env));
        }
        let mb = crate::memory::total(pids) / (1024 * 1024);
        let asleep = self.tabs.iter().filter(|t| !t.url.is_empty() && (t.ctrl.is_none() || t.mem == Mem::Dozing)).count();
        self.asleep_count = asleep;
        let line = if asleep > 0 { format!("Şu an {mb} MB kullanıyor · {asleep} sekme uyuyor") } else { format!("Şu an {mb} MB kullanıyor") };
        if line != self.ram_line {
            self.ram_line = line;
            self.invalidate();
        }
    }
}
