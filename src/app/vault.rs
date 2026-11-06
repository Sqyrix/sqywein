//! Şifre yöneticisinin uygulama tarafı: sayfalardaki otomatik doldurma,
//! "şifre kaydedilsin mi?" sorusu, Şifreler sayfası ve Windows Hello kapısı.
//!
//! Güvenlik: bir sayfaya yalnızca kendi adresiyle eşleşen hesaplar gösterilir;
//! adresi sayfanın söylediğinden değil, motorun bildirdiği belge adresinden
//! alırız. Şifreyi göstermek ve dışa aktarmak Windows Hello ister; doğrulama
//! bir dakika geçerli kalır (Chrome'daki gibi).

use super::*;
use crate::hello;
use serde_json::{json, Value};

/// Hello doğrulamasının geçerli kaldığı süre.
const HELLO_GRACE: Duration = Duration::from_secs(60);

/// Doğrulama bekleyen işlem.
#[derive(Clone)]
pub(super) enum Gate {
    Reveal(u64),
    Export,
}

/// Kaydedilmeyi bekleyen giriş: (sayfa adresi, kullanıcı adı, şifre, güncelleme mi).
pub(super) struct PendingLogin {
    pub url: String,
    pub username: String,
    pub password: String,
    pub update: bool,
}

impl App {
    fn reply(&self, tab_id: u32, msg: Value) {
        if let Some(v) = self.index_of(tab_id).and_then(|i| self.tabs[i].view.clone()) {
            engine::post_json(&v, &msg.to_string());
        }
    }

    fn save_vault(&mut self) {
        let profile = self.state.current_profile.clone();
        let _ = self.vault.save(&profile);
    }

    // ---------- Sayfalardaki giriş formları ----------

    pub(super) fn on_password_message(&mut self, tab_id: u32, frame: Option<ICoreWebView2Frame>, source: &str, json: &str) {
        let Ok(msg) = serde_json::from_str::<Value>(json) else { return };
        crate::log::line(&format!("ŞİFRE #{tab_id} {} {} {}", msg["type"].as_str().unwrap_or("?"), if frame.is_some() { "çerçeve" } else { "sayfa" }, url::host(source)));
        // Yanıt, mesajı gönderen çerçeveye (iframe) ya da sayfaya gider.
        let send = |app: &App, msg: Value| match &frame {
            Some(f) => {
                if let Ok(f2) = f.cast::<ICoreWebView2Frame2>() {
                    unsafe { let _ = f2.PostWebMessageAsJson(&HSTRING::from(msg.to_string())); }
                }
            }
            None => app.reply(tab_id, msg),
        };
        if !self.state.settings.save_passwords || self.index_of(tab_id).is_some_and(|i| self.tabs[i].private && msg["type"] == "pw.submitted") {
            return;
        }
        match msg["type"].as_str().unwrap_or("") {
            "pw.query" => {
                let accounts: Vec<Value> = self.vault.matches(source).iter().map(|c| json!({ "id": c.id, "username": c.username })).collect();
                if !accounts.is_empty() {
                    send(self, json!({ "type": "pw.accounts", "accounts": accounts }));
                }
            }
            "pw.fill" => {
                let id = msg["id"].as_u64().unwrap_or(0);
                // Yalnızca bu sayfayla eşleşen bir hesabın şifresi verilir.
                let found = self.vault.matches(source).iter().find(|c| c.id == id).map(|c| (c.username.clone(), c.password.clone()));
                if let Some((username, password)) = found {
                    self.vault.touch(id);
                    self.save_vault();
                    send(self, json!({ "type": "pw.secret", "id": id, "username": username, "password": password }));
                }
            }
            "pw.submitted" => {
                let username = msg["username"].as_str().unwrap_or("").to_string();
                let password = msg["password"].as_str().unwrap_or("").to_string();
                if password.is_empty() || self.vault.never(source) {
                    return;
                }
                let existing = self.vault.matches(source);
                if existing.iter().any(|c| c.username == username && c.password == password) {
                    return; // Zaten kayıtlı.
                }
                let update = existing.iter().any(|c| c.username == username);
                self.pending_login = Some(PendingLogin { url: source.to_string(), username, password, update });
                self.open_menu(MenuKind::SavePassword);
            }
            _ => {}
        }
    }

    pub(super) fn save_password_items(&self) -> Vec<Item> {
        let Some(p) = &self.pending_login else { return vec![Item::disabled("Kaydedilecek şifre yok")] };
        let host = url::host(&p.url).to_string();
        let title = if p.update { "Şifre güncellensin mi?" } else { "Şifre kaydedilsin mi?" };
        let who = if p.username.is_empty() { host.clone() } else { format!("{} · {host}", p.username) };
        vec![
            Item::Header(menu::Header { title: title.into(), sub: who, avatar: self.icon_for(&p.url), color: accent() }),
            Item::act(if p.update { "Güncelle" } else { "Kaydet" }, "", MenuCmd::SavePassword),
            Item::act("Bu sitede asla", "", MenuCmd::NeverSavePassword),
            Item::act("Şimdi değil", "", MenuCmd::DismissPassword),
        ]
    }

    pub(super) fn save_pending_login(&mut self, never: bool) {
        let Some(p) = self.pending_login.take() else { return };
        if never {
            self.vault.set_never(&p.url);
        } else {
            let _ = self.vault.upsert(&p.url, &p.username, &p.password);
        }
        self.save_vault();
        self.notify_passwords_page();
    }

    // ---------- Şifreler sayfası ----------

    fn favicon_url(&self, origin: &str) -> String {
        let host = url::host(origin).to_string();
        if let Some(hit) = self.favicon_urls.borrow().get(&host) {
            return hit.clone();
        }
        let url = self.icon_for(origin).map(|img| crate::image::to_data_url(&img)).unwrap_or_default();
        // Simge henüz inmediyse boş sonuç saklanmaz; sonraki listede tekrar denenir.
        if !url.is_empty() {
            self.favicon_urls.borrow_mut().insert(host, url.clone());
        }
        url
    }

    fn passwords_json(&self, query: &str) -> Value {
        let items: Vec<Value> = self
            .vault
            .list(query)
            .iter()
            .map(|c| json!({ "id": c.id, "origin": c.origin, "username": c.username, "lastUsed": c.last_used, "icon": self.favicon_url(&c.origin) }))
            .collect();
        json!({ "type": "passwords.list", "items": items })
    }

    pub(super) fn notify_passwords_page(&self) {
        let data = self.passwords_json("").to_string();
        for t in &self.tabs {
            if t.url.starts_with(&url::internal("passwords")) {
                if let Some(v) = &t.view {
                    engine::post_json(v, &data);
                }
            }
        }
    }

    /// İç sayfadan gelen `passwords.*` mesajları. Yanıt varsa döner.
    pub(super) fn on_passwords_page(&mut self, tab_id: u32, kind: &str, msg: &Value) -> Option<Value> {
        let s = |k: &str| msg[k].as_str().unwrap_or("").to_string();
        match kind {
            "passwords.list" => Some(self.passwords_json(&s("query"))),
            "passwords.reveal" => {
                self.gate(tab_id, Gate::Reveal(msg["id"].as_u64().unwrap_or(0)));
                None
            }
            "passwords.export" => {
                self.gate(tab_id, Gate::Export);
                None
            }
            "passwords.add" => {
                let _ = self.vault.upsert(&s("url"), &s("username"), &s("password"));
                self.save_vault();
                Some(self.passwords_json(""))
            }
            "passwords.update" => {
                self.vault.update(msg["id"].as_u64().unwrap_or(0), &s("username"), &s("password"));
                self.save_vault();
                Some(self.passwords_json(""))
            }
            "passwords.remove" => {
                self.vault.remove(msg["id"].as_u64().unwrap_or(0));
                self.save_vault();
                Some(self.passwords_json(""))
            }
            "passwords.import" => {
                let r = self.vault.import_csv(&s("csv"));
                self.save_vault();
                self.notify_passwords_page();
                Some(json!({ "type": "passwords.import", "added": r.added, "updated": r.updated, "skipped": r.skipped }))
            }
            _ => None,
        }
    }

    // ---------- Windows Hello kapısı ----------

    fn gate(&mut self, tab_id: u32, action: Gate) {
        if self.hello_until.is_some_and(|t| Instant::now() < t) {
            self.fulfil(tab_id, action);
            return;
        }
        let first = self.hello_pending.is_empty();
        self.hello_pending.push((tab_id, action));
        if first {
            self.hello_request += 1;
            hello::verify(self.hwnd, "Kayıtlı şifrelere erişmek için kimliğini doğrula.", self.hello_request);
        }
    }

    fn fulfil(&mut self, tab_id: u32, action: Gate) {
        match action {
            Gate::Reveal(id) => {
                if let Some(c) = self.vault.get(id) {
                    let msg = json!({ "type": "passwords.reveal", "id": id, "password": c.password });
                    self.reply(tab_id, msg);
                }
            }
            Gate::Export => {
                let csv = self.vault.export_csv();
                self.reply(tab_id, json!({ "type": "passwords.export", "csv": csv }));
            }
        }
    }

    pub(super) fn on_hello(&mut self, request: u32, outcome: hello::Outcome) {
        if request != self.hello_request {
            return;
        }
        let pending = std::mem::take(&mut self.hello_pending);
        match outcome {
            // Bu bilgisayarda Hello kurulu değilse erişim engellenmez (Windows oturumu zaten
            // açık); Ayarlar'dan Hello kurulursa bir sonraki istekte sorulur.
            hello::Outcome::Verified | hello::Outcome::Unavailable => {
                self.hello_until = Some(Instant::now() + HELLO_GRACE);
                for (tab, action) in pending {
                    self.fulfil(tab, action);
                }
            }
            hello::Outcome::Denied => {
                for (tab, action) in pending {
                    let id = if let Gate::Reveal(id) = action { id } else { 0 };
                    self.reply(tab, json!({ "type": "passwords.denied", "id": id }));
                }
            }
        }
    }
}
