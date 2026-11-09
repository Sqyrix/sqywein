//! Kalıcılık: oturum çerezleri ve site simgesi önbelleği.
//!
//! Süreli çerezleri motor zaten diske yazar. Süresiz (oturum) çerezler ise
//! tarayıcı kapanınca silinir; bazı siteler girişi bunlarla tutar. Chrome'un
//! "kaldığın yerden devam et" davranışı gibi bunları kapanırken saklıyor,
//! açılışta geri koyuyoruz. Dosya Windows hesabına bağlı DPAPI ile şifrelidir.

use super::*;
use crate::{engine::CookieData, net};
use std::path::PathBuf;

/// Kapanırken çerezlerin toplanması için en fazla bu kadar beklenir.
pub(super) const TIMER_CLOSE: usize = 4;

fn cookie_file(profile: &str) -> PathBuf {
    state::root().join(format!("session-{profile}.bin"))
}

fn favicon_dir() -> PathBuf {
    state::root().join("favicons")
}

fn favicon_file(host: &str) -> PathBuf {
    let safe: String = host.chars().map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '-' { c } else { '_' }).collect();
    favicon_dir().join(format!("{safe}.png"))
}

impl App {
    fn restores_session(&self) -> bool {
        self.state.settings.startup != "newtab"
    }

    /// İlk görünüm hazır olunca, sayfa açılmadan önce oturum çerezleri geri konur.
    pub(super) fn restore_session_cookies(&mut self, view: &ICoreWebView2) {
        if self.cookies_restored {
            return;
        }
        self.cookies_restored = true;
        if !self.restores_session() || crate::windows_session::secondary() {
            return;
        }
        if let Ok(cookies) = crate::storage::read_secret_json::<Vec<CookieData>>(&cookie_file(&self.state.current_profile)) {
            engine::restore_cookies(view, &cookies);
        }
    }

    /// Çerezlerin anlık görüntüsünü ister (sonuç `on_cookies` ile gelir).
    pub(super) fn snapshot_cookies(&self) -> bool {
        if !self.restores_session() || self.clearing_data.contains(&self.state.current_profile) {
            return false;
        }
        match self.any_view() {
            Some(view) => {
                let Some(generation)=crate::cookie_checkpoint::generation(&self.state.current_profile) else{return false};
                engine::snapshot_session_cookies(&view, self.state.current_profile.clone(), generation)
            }
            None => false,
        }
    }

    pub(super) fn on_cookies(&mut self, profile: &str, generation: u64, cookies: Option<Vec<CookieData>>) {
        let Some(_guard)=crate::cookie_checkpoint::lock_current(profile,generation) else{if self.closing{self.finish_close();}return};
        if let Some(mut cookies) = cookies {
          use sha2::{Digest, Sha256};
          cookies.sort_by(|a,b| (&a.domain,&a.path,&a.name).cmp(&(&b.domain,&b.path,&b.name)));
          if let Ok(json) = serde_json::to_vec(&cookies).map(zeroize::Zeroizing::new) {
            let hash: [u8; 32] = Sha256::digest(&json).into();
            let path = cookie_file(profile);
            if self.cookie_hashes.get(&profile.to_owned()) == Some(&hash) && path.exists() {
                if self.closing { self.finish_close(); }
                return;
            }
            if crate::storage::write_json(&path,&cookies).is_ok() {
                self.cookie_hashes.insert(profile.to_owned(), hash);
            }
          }
        }
        if self.closing {
            self.finish_close();
        }
    }

    pub(super) fn clear_saved_cookies(&mut self) {
        self.cookie_hashes.remove(&self.state.current_profile);
        if let Ok(epoch)=crate::cookie_checkpoint::begin(&self.state.current_profile){self.cookie_epochs.insert(self.state.current_profile.clone(),epoch);}
        let path = cookie_file(&self.state.current_profile);
        crate::storage::remove(&path);
        let _ = std::fs::remove_file(path.with_extension("tmp"));
    }

    pub(super) fn finish_cookie_clear(&mut self,profile:&str){if let Some(epoch)=self.cookie_epochs.remove(profile){crate::cookie_checkpoint::finish(profile,epoch);}}

    /// Pencere kapanışı: önce çerezler toplanır, sonra her şey kapatılır.
    pub(super) fn begin_close(&mut self) {
        if self.closing {
            return;
        }
        self.closing = true;
        self.close_menu();
        self.close_popup();
        self.close_suggest();
        self.close_dialog();
        if self.snapshot_cookies() {
            unsafe { SetTimer(Some(self.hwnd), TIMER_CLOSE, 1200, None) };
        } else {
            self.finish_close();
        }
    }

    pub(super) fn finish_close(&mut self) {
        self.cleanup_auth_popups(true);
        unsafe { let _ = KillTimer(Some(self.hwnd), TIMER_CLOSE); }
        self.state.session_clean = true;
        self.save();
        crate::windows_session::close();
        let _ = self.history.save_profile(&self.state.current_profile);
        for t in &mut self.tabs {
            t.close_view();
        }
        if let Some(c) = self.account_view.take() {
            unsafe { let _ = c.Close(); }
        }
        // Güncelleme kurulduysa yeni sürüm şimdi açılır (eskisi kapanınca devam eder).
        self.launch_restart();
        let hwnd = self.hwnd;
        defer(move || unsafe { let _ = DestroyWindow(hwnd); });
    }

    // ---------- Site simgeleri ----------

    /// Sitenin simgesi: önce bellek, sonra disk; yoksa arka planda indirilir.
    pub(super) fn icon_for(&self, page_url: &str) -> Option<Rc<Image>> {
        let host = url::host(page_url).to_string();
        if host.is_empty() || url::is_internal(page_url) {
            return None;
        }
        if let Some(hit) = self.icon_cache.borrow_mut().get(&host) {
            return hit.clone();
        }
        let img = crate::image::from_file(&favicon_file(&host), 32).map(Rc::new);
        if img.is_none() && self.icon_fetching.borrow().len() >= 4 { return None; }
        self.icon_cache.borrow_mut().insert(host.clone(), img.clone());
        if img.is_none() && !self.state.settings.privacy_shield && self.icon_fetching.borrow_mut().insert(host.clone()) {
            // Yer işaretleri ve kısayollar için: hiç ziyaret edilmemiş sitenin simgesi.
            std::thread::spawn(move || {
                net::init_thread();
                let bytes = net::fetch(&format!("https://www.google.com/s2/favicons?domain={host}&sz=32"), 256 * 1024).unwrap_or_default();
                engine::push_background(engine::Background::Favicon(host, bytes));
            });
        }
        img
    }

    /// Simge geldi: sekmeye konur ve diske yazılır.
    pub(super) fn store_favicon(&mut self, page_url: &str, img: Option<&Image>, bytes: &[u8]) {
        let host = url::host(page_url).to_string();
        if host.is_empty() || bytes.is_empty() || img.is_none() || url::is_internal(page_url) {
            return;
        }
        let _ = std::fs::create_dir_all(favicon_dir());
        let _ = std::fs::write(favicon_file(&host), bytes);
        self.icon_cache.borrow_mut().remove(&host);
    }

    pub(super) fn on_favicon_downloaded(&mut self, host: String, bytes: Vec<u8>) {
        self.icon_fetching.borrow_mut().remove(&host);
        let img = crate::image::from_bytes(&bytes, 32).map(Rc::new);
        if img.is_some() {
            let _ = std::fs::create_dir_all(favicon_dir());
            let _ = std::fs::write(favicon_file(&host), &bytes);
        }
        if img.is_some() {
            // Şifreler sayfası açıksa yeni gelen simgeyle tazelenir.
            self.favicon_urls.borrow_mut().remove(&host);
            self.icon_cache.borrow_mut().insert(host, img);
            if self.tabs.iter().any(|t| t.url.starts_with(&url::internal("passwords"))) {
                self.notify_passwords_page();
            }
        } else {
            self.icon_cache.borrow_mut().insert(host, img);
        }
        self.invalidate();
    }
}
