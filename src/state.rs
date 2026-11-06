//! Oturum dosyası. C# sürümüyle aynı klasör ve aynı `state.json` biçimi kullanılır;
//! bu sürümün tanımadığı alanlar olduğu gibi korunur.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::{collections::BTreeMap, fs, io, path::PathBuf};

#[derive(Serialize, Deserialize, Default, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct SavedTab {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub url: String,
}

/// Yer işareti. C# sürümündeki "Favorites" listesiyle aynı biçim.
#[derive(Serialize, Deserialize, Default, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct Bookmark {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub url: String,
}

/// Ayarlar sayfasıyla birebir aynı alan adları (camelCase), doğrudan gönderilir.
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub search_engine: String,
    pub startup: String,
    pub freeze_tabs: bool,
    pub sleep_tabs: bool,
    pub hardware_acceleration: bool,
    /// Yüzde; 0 = sistem ölçeği.
    pub default_zoom: u32,
    pub save_passwords: bool,
    pub autofill: bool,
    pub tracking: String,
    /// Reklam/izleme için üçüncü taraf çerezleri ve izleyicileri engeller.
    pub privacy_shield: bool,
    pub show_bookmarks_bar: bool,
    pub search_suggestions: bool,
    /// Reklam ve izleyici engelleyici.
    pub adblock: bool,
    /// Varsayılan filtreleme modu: 0 Kapalı, 1 Temel, 2 İdeal, 3 Tam.
    pub adblock_level: u8,
    /// Vurgu rengi: purple, blue, green, orange, pink, teal.
    pub accent: String,
    /// GitHub'dan otomatik güncelleme.
    pub auto_update: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            search_engine: "google".into(),
            startup: "restore".into(),
            freeze_tabs: true,
            sleep_tabs: false,
            hardware_acceleration: true,
            default_zoom: 0,
            save_passwords: true,
            autofill: true,
            tracking: "balanced".into(),
            privacy_shield: false,
            show_bookmarks_bar: true,
            search_suggestions: true,
            adblock: true,
            adblock_level: 2,
            accent: "purple".into(),
            auto_update: true,
        }
    }
}

impl Settings {
    /// Arama motorunun öneri adresi (OpenSearch JSON: ["sorgu", ["öneri", ...]]).
    pub fn suggest_url(&self) -> &'static str {
        match self.search_engine.as_str() {
            "bing" => "https://api.bing.com/osjson.aspx?query=",
            "duckduckgo" => "https://duckduckgo.com/ac/?type=list&q=",
            "yandex" => "https://suggest.yandex.com.tr/suggest-ff.cgi?part=",
            "brave" => "https://search.brave.com/api/suggest?q=",
            _ => "https://suggestqueries.google.com/complete/search?client=firefox&hl=tr&ie=utf-8&oe=utf-8&q=",
        }
    }

    pub fn search_url(&self) -> &'static str {
        match self.search_engine.as_str() {
            "bing" => "https://www.bing.com/search?q=",
            "duckduckgo" => "https://duckduckgo.com/?q=",
            "yandex" => "https://yandex.com.tr/search/?text=",
            "brave" => "https://search.brave.com/search?q=",
            _ => "https://www.google.com/search?q=",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "PascalCase", default)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub color: String,
    /// Tarayıcıda açık Google hesabı (algılanırsa).
    pub email: String,
    pub account_name: String,
    pub photo_url: String,
}

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "PascalCase", default)]
pub struct DownloadRecord {
    pub id: u32,
    pub name: String,
    pub path: String,
    pub url: String,
    /// progress, done, failed, canceled, paused
    pub status: String,
    pub received: u64,
    pub total: u64,
    pub when: String,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct AppState {
    #[serde(default)]
    pub tabs: Vec<SavedTab>,
    #[serde(default)]
    pub active_index: i32,
    #[serde(default)]
    pub favorites: Vec<Bookmark>,
    /// Ana sayfadaki kısayol kutucukları.
    #[serde(default)]
    pub shortcuts: Vec<Bookmark>,
    #[serde(default)]
    pub settings: Settings,
    /// Site başına yakınlaştırma: alan adı → oran.
    #[serde(default)]
    pub zoom: BTreeMap<String, f64>,
    #[serde(default)]
    pub profiles: Vec<Profile>,
    #[serde(default)]
    pub current_profile: String,
    /// Etkin olmayan profillerin kayıtlı sekmeleri.
    #[serde(default)]
    pub sessions: BTreeMap<String, Vec<SavedTab>>,
    #[serde(default)]
    pub pinned_extensions: Vec<String>,
    /// Eski sürümden: engelleyicinin kapatıldığı siteler (açılışta modlara taşınır).
    #[serde(default)]
    pub adblock_allow: Vec<String>,
    /// Site → filtreleme modu (varsayılandan farklıysa).
    #[serde(default)]
    pub adblock_levels: BTreeMap<String, u8>,
    /// Pencerenin son konumu: [sol, üst, sağ, alt, ekranı kaplıyor mu].
    #[serde(default)]
    pub window: Option<[i32; 5]>,
    /// Son güncelleme denetimi (Unix saniyesi).
    #[serde(default)]
    pub update_checked: u64,
    #[serde(default)]
    pub download_list: Vec<DownloadRecord>,
    /// Bizim kurduğumuz uzantıların klasörleri: uzantı kimliği → klasör.
    #[serde(default)]
    pub extension_folders: BTreeMap<String, String>,
    /// Mağaza kimliği → kurulu uzantı kimliği (kurulunca "Eklendi" görünsün).
    #[serde(default)]
    pub store_installs: BTreeMap<String, String>,
    /// Tanımadığımız alanlar: yazarken geri konur.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

pub const DEFAULT_PROFILE: &str = "default";

impl AppState {
    /// Profil listesi boşsa varsayılan profili ekler.
    pub fn ensure_profiles(&mut self) {
        if self.profiles.is_empty() {
            self.profiles.push(Profile { id: DEFAULT_PROFILE.into(), name: "Kişisel".into(), color: "#AF85FC".into(), ..Default::default() });
        }
        if !self.profiles.iter().any(|p| p.id == self.current_profile) {
            self.current_profile = self.profiles[0].id.clone();
        }
    }

    pub fn profile(&self) -> &Profile {
        self.profiles.iter().find(|p| p.id == self.current_profile).unwrap_or(&self.profiles[0])
    }

    pub fn profile_mut(&mut self) -> &mut Profile {
        let id = self.current_profile.clone();
        let i = self.profiles.iter().position(|p| p.id == id).unwrap_or(0);
        &mut self.profiles[i]
    }
}

/// WebView2 profil klasörünün adı. Varsayılan profil C# sürümüyle aynı klasörü kullanır.
pub fn engine_profile_name(id: &str) -> String {
    if id == DEFAULT_PROFILE { "Default".into() } else { format!("Sqywein-{id}") }
}

/// SQYWEIN_DATA ayarlıysa veriler oraya yazılır; testler gerçek profile dokunmaz.
pub fn root() -> PathBuf {
    match std::env::var_os("SQYWEIN_DATA") {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => {
            let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(|| ".".into());
            base.join("Sqywein")
        }
    }
}

fn state_path() -> PathBuf {
    root().join("state.json")
}

pub fn load() -> AppState {
    let mut state: AppState = fs::read(state_path())
        .ok()
        .and_then(|bytes| serde_json::from_slice(strip_bom(&bytes)).ok())
        .unwrap_or_default(); // İlk açılış ya da bozuk dosya: sıfırdan başlanır.
    state.ensure_profiles();
    state
}

pub fn save(state: &AppState) -> io::Result<()> {
    let dir = root();
    fs::create_dir_all(&dir)?;
    let path = state_path();
    let temp = path.with_extension("json.tmp");
    fs::write(&temp, serde_json::to_vec(state)?)?;
    fs::rename(temp, path)
}

fn strip_bom(bytes: &[u8]) -> &[u8] {
    bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bilinmeyen_alanlar_korunur() {
        let json = r#"{"Tabs":[{"Title":"A","Url":"https://a.com"}],"ActiveIndex":0,"Favorites":[{"Title":"G","Url":"https://g.com"}],"Downloads":[{"Name":"x"}]}"#;
        let state: AppState = serde_json::from_str(json).unwrap();
        assert_eq!(state.tabs[0].url, "https://a.com");
        assert_eq!(state.favorites[0].title, "G");
        let back = serde_json::to_string(&state).unwrap();
        assert!(back.contains("\"Favorites\""));
        assert!(back.contains("\"Downloads\""));
    }

    #[test]
    fn ayarlar_varsayilanlari() {
        let state: AppState = serde_json::from_str(r#"{"Settings":{"searchEngine":"bing"}}"#).unwrap();
        assert_eq!(state.settings.search_url(), "https://www.bing.com/search?q=");
        assert!(state.settings.freeze_tabs);
        assert!(!state.settings.sleep_tabs);
    }
}
