//! Google hesabı ve tarayıcı profilleri.
//!
//! WebView2'de Chrome'daki gibi "tarayıcıya giriş" yoktur; ama bir sitede
//! Google'a giriş yapıldığında çerezler profilde durur. Hangi hesabın açık
//! olduğunu Google'ın kendi hesap listesi adresinden, görünmeyen küçük bir
//! görünümde okuyup profile yazıyoruz (Chromium da aynı adresi kullanır).

use super::*;
use crate::engine::Background;
use serde_json::Value;
use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};

const LIST_ACCOUNTS: &str = "https://accounts.google.com/ListAccounts?gpsia=1&source=ChromiumBrowser&json=standard";
const PROFILE_COLORS: [&str; 6] = ["#AF85FC", "#5BC0EB", "#F4A259", "#7BD389", "#F25F5C", "#E4C1F9"];

/// Hesap listesinden ilk açık hesap: (ad, e-posta, fotoğraf adresi).
pub(super) fn parse_list_accounts(text: &str) -> Option<(String, String, String)> {
    let v: Value = serde_json::from_str(text.trim()).ok()?;
    let accounts = v.get(1)?.as_array()?;
    accounts.iter().find_map(|a| {
        let email = a.get(3)?.as_str()?.to_string();
        if email.is_empty() {
            return None;
        }
        let name = a.get(2).and_then(Value::as_str).unwrap_or("").to_string();
        let mut photo = a.get(4).and_then(Value::as_str).unwrap_or("").to_string();
        if photo.starts_with("//") {
            photo = format!("https:{photo}");
        }
        Some((name, email, photo))
    })
}

impl App {
    /// Açık Google hesabını yeniden okur. `force` değilse en fazla dakikada bir.
    pub(super) fn refresh_account(&mut self, force: bool) {
        let Some(env) = self.env.clone() else { return };
        if !force && self.account_checked.is_some_and(|t| t.elapsed() < Duration::from_secs(60)) {
            return;
        }
        self.account_checked = Some(Instant::now());
        match &self.account_view {
            Some(ctrl) => {
                if let Ok(view) = unsafe { ctrl.CoreWebView2() } {
                    unsafe { let _ = view.Navigate(&wide(LIST_ACCOUNTS)); }
                }
            }
            None => {
                let profile = self.engine_profile();
                let _ = engine::create_view(&env, self.hwnd, ACCOUNT_VIEW, &profile, false);
            }
        }
    }

    pub(super) fn on_account_view_ready(&mut self, ctrl: Option<ICoreWebView2Controller>) {
        let Some(ctrl) = ctrl else { return };
        let Ok(view) = engine::configure(&ctrl, ACCOUNT_VIEW, BASE, &self.state.settings) else {
            unsafe { let _ = ctrl.Close(); }
            return;
        };
        // Hesap okunmadan önce kayıtlı oturum çerezleri yerine konur.
        self.restore_session_cookies(&view);
        unsafe {
            let _ = ctrl.SetIsVisible(false);
            let _ = ctrl.SetBounds(RECT::default());
            let _ = view.Navigate(&wide(LIST_ACCOUNTS));
        }
        self.account_view = Some(ctrl);
    }

    pub(super) fn on_account_page_loaded(&mut self, ok: bool) {
        let view = self.account_view.as_ref().and_then(|c| unsafe { c.CoreWebView2() }.ok());
        match (ok, view) {
            (true, Some(view)) => engine::run_script(&view, ACCOUNT_VIEW, "document.body ? document.body.innerText : ''"),
            _ => self.close_account_view(),
        }
    }

    fn close_account_view(&mut self) {
        // Görünüm yalnızca okuma için açıldı; bellekte tutulmaz.
        if let Some(c) = self.account_view.take() {
            unsafe { let _ = c.Close(); }
        }
    }

    pub(super) fn on_account_json(&mut self, result: &str) {
        self.close_account_view();
        let text: String = serde_json::from_str(result).unwrap_or_default();
        let (name, email, photo) = parse_list_accounts(&text).unwrap_or_default();
        let p = self.state.profile_mut();
        if p.email == email && p.photo_url == photo && p.account_name == name {
            return;
        }
        p.email = email;
        p.account_name = name;
        p.photo_url = photo;
        self.dirty = true;
        self.load_avatar();
        self.notify_pages("settings");
        self.invalidate();
    }

    /// Profil fotoğrafını arka planda indirir.
    pub(super) fn load_avatar(&mut self) {
        let url = self.state.profile().photo_url.clone();
        self.avatar = None;
        if url.is_empty() {
            return;
        }
        std::thread::spawn(move || {
            let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
            let path = std::env::temp_dir().join(format!("sqywein-avatar-{}.img", std::process::id()));
            let ok = extensions::download_file(&url, &path);
            let img = if ok { crate::image::from_file(&path, 96) } else { None };
            let _ = std::fs::remove_file(&path);
            engine::push_background(Background::Avatar(url, img));
        });
    }

    pub(super) fn on_background(&mut self, b: Background) {
        match b {
            Background::Avatar(url, img) => {
                if url == self.state.profile().photo_url {
                    self.avatar = img.map(Rc::new);
                    self.invalidate();
                }
            }
            Background::ExtensionUnpacked(result) => self.on_extension_unpacked(result),
            Background::Favicon(host, bytes) => self.on_favicon_downloaded(host, bytes),
            Background::Suggestions(generation, list) => self.on_suggestions(generation, list),
            Background::Adblock(engines, fresh) => self.on_adblock_engine(engines, fresh),
            Background::Hello(request, outcome) => self.on_hello(request, outcome),
            Background::Update(outcome) => self.on_update(outcome),
        }
    }

    pub(super) fn profile_menu_items(&self) -> Vec<Item> {
        let p = self.state.profile();
        let signed_in = !p.email.is_empty();
        let title = if p.account_name.is_empty() { p.name.clone() } else { p.account_name.clone() };
        let sub = if signed_in { p.email.clone() } else { "Google hesabıyla oturum açılmadı".into() };
        let mut items = vec![
            Item::Header(menu::Header { title, sub, avatar: self.avatar.clone(), color: menu::parse_color(&p.color) }),
            if signed_in {
                Item::act("Google Hesabını yönet", "", MenuCmd::ManageAccount)
            } else {
                Item::act("Google hesabıyla oturum aç", "", MenuCmd::SignIn)
            },
            Item::act("Şifreler ve otomatik doldurma", "", MenuCmd::Passwords),
            Item::Sep,
        ];
        for (i, other) in self.state.profiles.iter().enumerate() {
            if other.id == self.state.current_profile {
                continue;
            }
            let hint = if other.email.is_empty() { "Geç".to_string() } else { other.email.clone() };
            items.push(Item::act(other.name.clone(), hint, MenuCmd::SwitchProfile(i)));
        }
        items.push(Item::act("Profil ekle", "", MenuCmd::AddProfile));
        items.push(Item::act("Profilleri yönet", "", MenuCmd::ManageProfiles));
        items
    }

    /// Profil değişince sekmeler o profilin oturumuyla değişir; her profilin
    /// çerezleri, şifreleri ve uzantıları ayrıdır.
    pub(super) fn switch_profile(&mut self, i: usize) {
        let Some(target) = self.state.profiles.get(i).map(|p| p.id.clone()) else { return };
        if target == self.state.current_profile {
            return;
        }
        self.close_popup();
        let (tabs, _) = self.session_tabs();
        let current = self.state.current_profile.clone();
        self.state.sessions.insert(current, tabs);
        for t in &mut self.tabs {
            t.close_view();
        }
        self.close_account_view();
        self.tabs.clear();
        self.ghosts.clear();
        self.extensions.clear();
        self.ext_icons.clear();
        self.state.current_profile = target.clone();
        // Her profilin şifreleri ayrıdır.
        self.vault = crate::passwords::Vault::load(&target);
        self.hello_until = None;
        for t in self.state.sessions.remove(&target).unwrap_or_default() {
            self.push_tab(t.url, t.title);
        }
        if self.tabs.is_empty() {
            self.push_tab(String::new(), String::new());
        }
        let past = Instant::now().checked_sub(Duration::from_secs(5)).unwrap_or_else(Instant::now);
        for t in &mut self.tabs {
            t.born = past;
        }
        self.active = usize::MAX;
        self.activate(0);
        self.load_avatar();
        self.refresh_account(true);
        self.dirty = true;
        self.save();
    }

    pub(super) fn add_profile(&mut self, name: Option<String>) {
        let n = self.state.profiles.len();
        let id = (1..).map(|k| format!("p{k}")).find(|id| !self.state.profiles.iter().any(|p| &p.id == id)).unwrap_or_default();
        let name = name.filter(|s| !s.trim().is_empty()).unwrap_or_else(|| format!("Profil {}", n + 1));
        let color = PROFILE_COLORS[n % PROFILE_COLORS.len()].to_string();
        self.state.profiles.push(state::Profile { id, name, color, ..Default::default() });
        self.switch_profile(self.state.profiles.len() - 1);
    }

    pub(super) fn rename_profile(&mut self, id: &str, name: &str) {
        if let Some(p) = self.state.profiles.iter_mut().find(|p| p.id == id) {
            if !name.trim().is_empty() {
                p.name = name.trim().to_string();
                self.dirty = true;
            }
        }
        self.invalidate();
    }

    pub(super) fn remove_profile(&mut self, id: &str) {
        if id == self.state.current_profile || self.state.profiles.len() <= 1 {
            return;
        }
        self.state.profiles.retain(|p| p.id != id);
        self.state.sessions.remove(id);
        self.dirty = true;
    }

    pub(super) fn profiles_json(&self) -> Value {
        Value::Array(
            self.state
                .profiles
                .iter()
                .map(|p| {
                    serde_json::json!({
                        "id": p.id,
                        "name": p.name,
                        "email": p.email,
                        "color": p.color,
                        "current": p.id == self.state.current_profile,
                    })
                })
                .collect(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::parse_list_accounts;

    #[test]
    fn hesap_listesi() {
        let text = r#"["gaia.l.a.r",[["gaia.l.a",1,"Emir","emir@gmail.com","https://lh3.googleusercontent.com/a/x",1,1,0,null,1,"123"]]]"#;
        let (name, email, photo) = parse_list_accounts(text).unwrap();
        assert_eq!((name.as_str(), email.as_str()), ("Emir", "emir@gmail.com"));
        assert!(photo.starts_with("https://lh3"));
        assert!(parse_list_accounts(r#"["gaia.l.a.r",[]]"#).is_none());
        assert!(parse_list_accounts("bozuk").is_none());
    }
}
