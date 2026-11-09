//! Otomatik güncellemenin uygulama tarafı: zamanlı denetim, "Güncelle"
//! düğmesi ve yeni sürümle yeniden başlatma.

use super::*;
use crate::update::{self, Outcome};

/// Otomatik denetim aralığı.
const CHECK_EVERY: u64 = 12 * 3600;

fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

impl App {
    /// Açılışta ve 12 saatte bir: süresi geldiyse arka planda denetler.
    pub(super) fn maybe_check_updates(&mut self) {
        if !self.state.settings.auto_update || self.update_busy || self.update_ready.is_some() {
            return;
        }
        if now_secs().saturating_sub(self.state.update_checked) < CHECK_EVERY {
            return;
        }
        self.check_updates();
    }

    /// Hemen denetler (Ayarlar > "Güncellemeleri denetle").
    pub(super) fn check_updates(&mut self) {
        if self.update_busy {
            return;
        }
        self.update_busy = true;
        self.update_status = "Denetleniyor…".into();
        self.notify_pages("settings");
        std::thread::spawn(|| {
            crate::net::init_thread();
            engine::push_background(engine::Background::Update(update::check()));
        });
    }

    pub(super) fn on_update(&mut self, outcome: Outcome) {
        self.update_busy = false;
        self.state.update_checked = now_secs();
        self.dirty = true;
        self.update_status = match &outcome {
            Outcome::Latest => format!("Sqywein güncel (v{})", update::CURRENT),
            Outcome::Ready(v, _) => format!("v{v} hazır: yeniden başlatınca kurulur"),
            Outcome::Failed(e) => format!("Denetlenemedi: {e}"),
        };
        if let Outcome::Ready(v, path) = outcome {
            crate::log::line(&format!("GÜNCELLEME v{v} hazır"));
            self.update_ready = Some((v, path));
            self.layout_children();
        }
        self.notify_pages("settings");
        self.invalidate();
    }

    /// "Güncelle": yeni exe yerine konur, oturum kaydedilip yeni sürüm açılır.
    pub(super) fn apply_update(&mut self) {
        if crate::integration::windows().len()>1{self.open_surface(super::overlay::Surface::Notice("Güncellemeyi kurmadan önce diğer Sqywein pencerelerini kapatın. Açık sekmeleriniz korunur.".into()));return;}
        let Some((version, path)) = self.update_ready.clone() else { return };
        match update::install(&path) {
            Ok(exe) => {
                crate::log::line(&format!("GÜNCELLEME v{version} kuruldu, yeniden başlatılıyor"));
                self.restart_with = Some(exe);
                self.remember_placement();
                self.begin_close();
            }
            Err(e) => {
                self.update_ready = None;
                self.update_status = format!("Kurulamadı: {e}");
                self.layout_children();
                self.invalidate();
            }
        }
    }

    /// Kapanış bitince: güncelleme kurulduysa yeni sürüm başlatılır; eskisinin
    /// kapanmasını bekler ki profil klasörü serbest kalsın.
    pub(super) fn launch_restart(&mut self) {
        let Some(exe) = self.restart_with.take() else { return };
        let pid = std::process::id();
        let _ = std::process::Command::new(exe).arg(format!("--wait-pid={pid}")).spawn();
    }

    pub(super) fn update_json(&self) -> serde_json::Value {
        serde_json::json!({
            "version": update::CURRENT,
            "status": self.update_status,
            "ready": self.update_ready.as_ref().map(|(v, _)| v.clone()),
        })
    }

    pub(super) fn update_pill_width(&self) -> Option<f32> {
        self.update_ready.as_ref().map(|_| 92.0)
    }

    pub(super) fn paint_update_pill(&self, r: Rect) {
        let g = &self.gfx;
        g.round(r, 12.0, if self.hover == Hit::Update { ink() } else { accent() });
        g.text("Güncelle", r, Font::Small, base(), Align::Center);
    }
}

/// Yeni sürüm açılırken: eski sürümün kapanması beklenir (en fazla 15 sn).
pub fn wait_for_previous() {
    use windows::Win32::System::Threading::{OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE};
    let Some(pid) = std::env::args().find_map(|a| a.strip_prefix("--wait-pid=").and_then(|p| p.parse::<u32>().ok())) else { return };
    unsafe {
        if let Ok(handle) = OpenProcess(PROCESS_SYNCHRONIZE, false, pid) {
            let _ = WaitForSingleObject(handle, 15_000);
            let _ = windows::Win32::Foundation::CloseHandle(handle);
        }
    }
}
