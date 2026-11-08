//! İlk öneri engelleyici değildir; native düğmeler klavye/UIA ile erişilebilir.
use super::*;

pub(super) struct DefaultPrompt { pub action: HWND, pub dismiss: HWND }

impl App {
    pub(super) fn refresh_theme(&mut self) {
        BRUSHES.with(|b| { if let Some((a,c))=b.borrow_mut().take(){unsafe{let _=DeleteObject(a.into());let _=DeleteObject(c.into());}} });
        let dark = i32::from(is_dark());
        unsafe {let _=DwmSetWindowAttribute(self.hwnd,DWMWA_USE_IMMERSIVE_DARK_MODE,&dark as *const _ as _,4);}
        let message=crate::theme::message().to_string();
        for tab in &self.tabs {
            if let Some(view)=&tab.view {
                engine::apply_color_scheme(view);
                engine::post_json(view,&message);
            }
        }
        if let Some(m)=&self.menu {unsafe{let _=DwmSetWindowAttribute(m.hwnd,DWMWA_USE_IMMERSIVE_DARK_MODE,&dark as *const _ as _,4);let _=InvalidateRect(Some(m.hwnd),None,false);}}
        for window in [self.dialog.as_ref().map(|d|d.hwnd),self.suggest.as_ref().map(|s|s.hwnd)].into_iter().flatten(){unsafe{
            let _=DwmSetWindowAttribute(window,DWMWA_USE_IMMERSIVE_DARK_MODE,&dark as *const _ as _,4);
            let _=InvalidateRect(Some(window),None,true);
        }}
        if let Some(prompt)=&self.default_prompt {for button in [prompt.action,prompt.dismiss]{unsafe{
            let _=windows::Win32::UI::Controls::SetWindowTheme(button,if is_dark(){w!("DarkMode_Explorer")}else{w!("Explorer")},None);
            let _=InvalidateRect(Some(button),None,true);
        }}}
        unsafe {let _=InvalidateRect(Some(self.edit),None,true);let _=InvalidateRect(Some(self.start_edit),None,true);}
        self.invalidate();
    }

    pub(super) fn init_browser_integration(&mut self) {
        if std::env::var_os("SQYWEIN_DATA").is_none() {
            let stamp = format!("3|{}", std::env::current_exe().unwrap_or_default().display());
            if self.state.browser_registration != stamp && crate::integration::register_browser().is_ok() {
                self.state.browser_registration = stamp; self.dirty = true;
            }
        }
        if !self.state.default_prompt_seen && !crate::integration::is_dev() {
            self.state.default_prompt_seen = true; self.dirty = true;
            if !crate::integration::is_default_browser() {
                let make = |text: &str, id: usize| unsafe {
                    CreateWindowExW(WINDOW_EX_STYLE(0), w!("BUTTON"), &HSTRING::from(text), WS_CHILD | WS_TABSTOP,
                        0,0,0,0,Some(self.hwnd),Some(HMENU(id as _)),Some(HINSTANCE(GetModuleHandleW(None).unwrap_or_default().0)),None)
                };
                match (make("Varsayılan tarayıcı yap",301),make("Şimdi değil",302)) {
                    (Ok(action),Ok(dismiss)) => {
                        for button in [action,dismiss] { unsafe { let _ = windows::Win32::UI::Controls::SetWindowTheme(button,w!("DarkMode_Explorer"),None); } }
                        self.default_prompt = Some(DefaultPrompt { action,dismiss });
                    }
                    (action,dismiss) => { for hwnd in [action.ok(),dismiss.ok()].into_iter().flatten() { unsafe { let _ = DestroyWindow(hwnd); } } }
                }
                self.layout_children(); self.invalidate();
            }
        }
        if self.dirty { self.save(); }
    }
    pub(super) fn default_prompt_command(&mut self, hwnd: HWND) -> bool {
        let Some(prompt) = &self.default_prompt else { return false };
        if hwnd != prompt.action && hwnd != prompt.dismiss { return false; }
        let choose = hwnd == prompt.action;
        self.dismiss_default_prompt();
        if choose { defer(|| {
            if let Err(error) = crate::integration::open_default_settings() { unsafe {
                MessageBoxW(None,&HSTRING::from(error.to_string()),w!("Sqywein"),MB_OK | MB_ICONERROR);
            } }
        }); }
        true
    }
    pub(super) fn dismiss_default_prompt(&mut self) {
        if let Some(prompt) = self.default_prompt.take() { defer(move || unsafe { let _ = DestroyWindow(prompt.action); let _ = DestroyWindow(prompt.dismiss); }); }
        self.layout_children(); self.invalidate(); self.focus_content();
    }
    pub(super) fn place_default_prompt(&self, l: &Layout) {
        let Some(prompt) = &self.default_prompt else { return };
        for (button,width,offset) in [(prompt.action,200.0,112.0),(prompt.dismiss,96.0,8.0)] {
            unsafe {
                if let Some(r) = l.default_banner {
                    let p = self.px(Rect::new(r.right()-offset-width,r.bottom()-40.0,width,32.0));
                    let _ = SetWindowPos(button,None,p.left,p.top,p.right-p.left,p.bottom-p.top,SWP_NOZORDER | SWP_NOACTIVATE | SWP_SHOWWINDOW);
                    SendMessageW(button,WM_SETFONT,Some(WPARAM(self.edit_font.0 as usize)),Some(LPARAM(1)));
                } else { let _ = ShowWindow(button,SW_HIDE); }
            }
        }
    }
    pub(super) fn paint_default_prompt(&self, l: &Layout) {
        let Some(r) = l.default_banner else { return };
        self.gfx.fill(r,raised());
        let compact = r.h > 60.0;
        let text = Rect::new(r.x+12.0,r.y+6.0,if compact {r.w-24.0} else {(r.w-340.0).max(0.0)},32.0);
        self.gfx.text("Bağlantılarınızı Sqywein ile açmak ister misiniz?",text,Font::Ui,ink(),Align::Left);
    }
    pub(super) fn default_prompt_key(&mut self, target: HWND, vk: u32) -> bool {
        let Some(prompt) = &self.default_prompt else { return false };
        if vk == VK_F6.0 as u32 {
            if target == prompt.action || target == prompt.dismiss { self.focus_content(); }
            else { let action=prompt.action; defer(move || unsafe { let _ = SetFocus(Some(action)); }); }
            return true;
        }
        if target == prompt.action || target == prompt.dismiss {
            if vk == VK_RETURN.0 as u32 { return self.default_prompt_command(target); }
            if vk == VK_ESCAPE.0 as u32 { self.dismiss_default_prompt(); return true; }
            if vk == VK_TAB.0 as u32 {
                let next=if target==prompt.action {prompt.dismiss} else {self.edit};
                defer(move || unsafe { let _ = SetFocus(Some(next)); }); return true;
            }
        }
        false
    }
}
