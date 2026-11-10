//! Oturum dosyası. C# sürümüyle aynı klasör ve aynı `state.json` biçimi kullanılır;
//! bu sürümün tanımadığı alanlar olduğu gibi korunur.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::{collections::BTreeMap, io, path::PathBuf};

#[derive(Serialize, Deserialize, Default, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct SavedTab {
    #[serde(default)]
    pub scroll: [f64; 2],
    #[serde(default)]
    pub pinned: bool,
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
    /// Empty target follows the Windows locale. Automatic translation is opt-in per language.
    pub translation_target: String,
    pub translation_always: Vec<String>,
    pub translation_never: Vec<String>,
    pub translation_sites: Vec<String>,
    /// Ana dil dışındaki sayfalar açılınca kendiliğinden çevrilsin mi?
    pub translation_auto: bool,
    /// Başlangıç sayfasında hava durumu ve döviz kartları.
    pub show_widgets: bool,
    /// Hava durumu için elle yazılan şehir; boşsa Windows konumu kullanılır.
    pub weather_city: String,
    pub theme: String,
    pub search_engine: String,
    pub startup: String,
    pub freeze_tabs: bool,
    pub never_sleep: Vec<String>,
    pub download_folder: String,
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
    /// Tek vurgu rengi: blue; eski renkler yüklemede taşınır.
    pub accent: String,
    /// GitHub'dan otomatik güncelleme.
    pub auto_update: bool,
    pub animations: bool,
    /// Pencerenin arkasında Windows camı (Acrylic) ve saydam araç çubuğu.
    pub glass: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            translation_target: String::new(),
            translation_always: Vec::new(),
            translation_never: Vec::new(),
            translation_sites: Vec::new(),
            translation_auto: true,
            show_widgets: true,
            weather_city: String::new(),
            theme: "system".into(),
            search_engine: "google".into(),
            startup: "restore".into(),
            freeze_tabs: true,
            never_sleep: Vec::new(),
            download_folder: String::new(),
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
            accent: "blue".into(),
            auto_update: true,
            animations: true,
            glass: true,
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
    #[serde(skip)]
    pub private: bool,
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
    #[serde(skip)]
    pub baseline: Map<String,Value>,
    #[serde(skip)]
    pub storage_error: Option<String>,
    #[serde(default,deserialize_with="null_default")]
    pub session_clean: bool,
    #[serde(default)]
    pub default_prompt_seen: bool,
    #[serde(default)]
    pub browser_registration: String,
    #[serde(default)]
    pub tabs: Vec<SavedTab>,
    #[serde(default)]
    pub active_index: i32,
    #[serde(default)]
    pub favorites: Vec<Bookmark>,
    /// Okuma Listesi: daha sonra okunacak sayfalar.
    #[serde(default,deserialize_with="null_default")]
    pub reading_list: Vec<Bookmark>,
    /// Ana sayfadaki kısayol kutucukları.
    #[serde(default)]
    pub shortcuts: Vec<Bookmark>,
    /// Favorilerden silinen siteler: sık ziyaret edilse de kendiliğinden geri eklenmez.
    #[serde(default)]
    pub shortcuts_removed: Vec<String>,
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
    /// Başlangıç sayfası kartlarının son verisi (açılışta hemen gösterilir).
    #[serde(default)]
    pub widgets: crate::widgets::Cache,
    /// Gizlilik raporu: (gün = Unix saniyesi / 86400, engellenen istek), son 7 gün.
    #[serde(default)]
    pub privacy_days: Vec<(u64, u64)>,
    #[serde(default)]
    #[serde(serialize_with = "public_downloads")]
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

fn public_downloads<S: serde::Serializer>(items: &[DownloadRecord], serializer: S) -> Result<S::Ok, S::Error> {
    items.iter().filter(|d| !d.private).collect::<Vec<_>>().serialize(serializer)
}

pub const DEFAULT_PROFILE: &str = "default";

pub fn null_default<'de,D,T>(deserializer:D)->Result<T,D::Error> where D:serde::Deserializer<'de>,T:serde::Deserialize<'de>+Default {Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())}

pub fn valid_profile(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.bytes().all(|c| c.is_ascii_alphanumeric() || c==b'-' || c==b'_')
}

impl AppState {
    fn normalize_appearance(&mut self) {
        self.settings.accent="blue".into();
        for profile in &mut self.profiles {profile.color="#0A84FF".into();}
    }

    /// Profil listesi boşsa varsayılan profili ekler.
    pub fn ensure_profiles(&mut self) {
        self.profiles.retain(|p| valid_profile(&p.id));
        if self.profiles.is_empty() {
            self.profiles.push(Profile { id: DEFAULT_PROFILE.into(), name: "Kişisel".into(), color: "#0A84FF".into(), ..Default::default() });
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
    let mut state: AppState = match crate::storage::read_json(&state_path()) {
        Ok(value) => value,
        Err(e) if e.kind()==io::ErrorKind::NotFound => AppState::default(),
        Err(e) => AppState { storage_error: Some(e.to_string()), ..Default::default() },
    };
    state.ensure_profiles();
    state.baseline=serde_json::to_value(&state).ok().and_then(|v|v.as_object().cloned()).unwrap_or_default();
    crate::windows_session::restore(&mut state);
    state.normalize_appearance();
    state
}

pub fn save(state: &mut AppState) -> io::Result<()> {
    if state.storage_error.is_some() { return Err(io::Error::other("Okunamayan oturumun üzerine yazılmadı")); }
    let _lock=crate::storage::RecordLock::acquire(&state_path())?;
    state.normalize_appearance();
    let ours=serde_json::to_value(&state)?;
    let mut latest:Value=match crate::storage::read_json(&state_path()){Ok(v)=>v,Err(e) if e.kind()==io::ErrorKind::NotFound=>Value::Object(Map::new()),Err(e)=>return Err(e)};
    let object=latest.as_object_mut().ok_or_else(||io::Error::other("Oturum biçimi geçersiz"))?;
    for (key,value) in ours.as_object().unwrap(){
        if crate::windows_session::secondary() && (crate::windows_session::private() || crate::integration::other_normal_windows()) && ["Tabs","ActiveIndex","Window","CurrentProfile","SessionClean"].contains(&key.as_str()){continue}
        let before=state.baseline.get(key).unwrap_or(&Value::Null);
        if object.get(key).is_some_and(Value::is_null) && !value.is_null(){object.insert(key.clone(),value.clone());}
        else if before!=value || !object.contains_key(key) {object.insert(key.clone(),merge_value(before,value,object.get(key).unwrap_or(&Value::Null)));}
    }
    latest["Settings"]["accent"]=Value::String("blue".into());
    if let Some(profiles)=latest["Profiles"].as_array_mut(){for profile in profiles{profile["Color"]=Value::String("#0A84FF".into());}}
    crate::windows_session::checkpoint(state)?;
    crate::storage::write_json(&state_path(),&latest)?;
    let mut current=latest;
    for key in ["Tabs","ActiveIndex","Window","CurrentProfile","SessionClean"]{current[key]=ours[key].clone();}
    let baseline=current.as_object().unwrap().clone();
    *state=serde_json::from_value(current)?;state.ensure_profiles();state.baseline=baseline;
    Ok(())
}

fn merge_value(base:&Value,ours:&Value,theirs:&Value)->Value{
    if ours==base{return theirs.clone()}
    if theirs==base||ours==theirs{return ours.clone()}
    if let (Some(b),Some(o),Some(t))=(base.as_object(),ours.as_object(),theirs.as_object()){
        let mut merged=t.clone();for key in b.keys().chain(o.keys()){
            let old=b.get(key).unwrap_or(&Value::Null);let new=o.get(key).unwrap_or(&Value::Null);
            if new!=old{if let Some(new)=o.get(key){merged.insert(key.clone(),merge_value(old,new,t.get(key).unwrap_or(&Value::Null)));}else{merged.remove(key);}}
        }return Value::Object(merged)
    }
    if let (Some(b),Some(o),Some(t))=(base.as_array(),ours.as_array(),theirs.as_array()){
        let key=|v:&Value|v.get("Id").or_else(||v.get("Url")).or_else(||v.get("id")).filter(|v|v.is_string()).cloned();
        if b.iter().chain(o).chain(t).all(|v|key(v).is_some()){
            let mut merged=t.clone();
            for old in b{let id=key(old);if !o.iter().any(|v|key(v)==id){merged.retain(|v|key(v)!=id);}}
            for new in o{let id=key(new);let old=b.iter().find(|v|key(v)==id);if old!=Some(new){if let Some(at)=merged.iter().position(|v|key(v)==id){merged[at]=merge_value(old.unwrap_or(&Value::Null),new,&merged[at]);}else{merged.push(new.clone());}}}
            return Value::Array(merged)
        }
    }ours.clone()
}

#[cfg(test)]
mod tests {
    #[test] fn legacy_colors_migrate_without_changing_session_or_profile_identity(){
        let mut state:AppState=serde_json::from_str(r##"{"Tabs":[{"Title":"Keep","Url":"https://example.invalid/keep"}],"CurrentProfile":"default","Profiles":[{"Id":"default","Name":"Keep profile","Color":"#AF85FC"}],"Settings":{"accent":"purple","theme":"dark"},"KeepUnknown":17}"##).unwrap();
        let tabs=state.tabs[0].url.clone();state.normalize_appearance();
        assert_eq!(state.settings.accent,"blue");assert_eq!(state.settings.theme,"dark");assert_eq!(state.tabs[0].url,tabs);assert_eq!(state.profiles[0].name,"Keep profile");assert_eq!(state.profiles[0].color,"#0A84FF");assert_eq!(state.extra["KeepUnknown"],17);
    }
    #[test] fn legacy_null_session_marker_keeps_the_session(){
        let state:AppState=serde_json::from_str(r#"{"SessionClean":null,"ReadingList":null,"Tabs":[{"Title":"Keep","Url":"https://example.invalid/keep"}],"Settings":{"theme":"dark"}}"#).unwrap();
        assert!(!state.session_clean);assert_eq!(state.tabs.len(),1);assert_eq!(state.tabs[0].title,"Keep");assert_eq!(state.settings.theme,"dark");
    }
    #[test] fn profile_ids_cannot_escape_the_data_directory(){
        for id in ["../other","a/b","a\\b","", ".", "C:other"]{assert!(!valid_profile(id));}
        let mut s=AppState{profiles:vec![Profile{id:"../../outside".into(),..Default::default()}],current_profile:"../../outside".into(),..Default::default()};
        s.ensure_profiles();assert_eq!(s.current_profile,DEFAULT_PROFILE);assert!(valid_profile(&s.profile().id));
    }
    use super::*;

    #[test]
    fn private_downloads_never_reach_disk() {
        let mut state = AppState::default();
        state.download_list.push(DownloadRecord { private: true, url: "https://private.invalid/secret".into(), path: "private-path".into(), ..Default::default() });
        state.download_list.push(DownloadRecord { url: "https://public.invalid/file".into(), ..Default::default() });
        let json = serde_json::to_string(&state).unwrap();
        assert!(!json.contains("private.invalid")); assert!(!json.contains("private-path"));
        let loaded: AppState = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.download_list.len(), 1);
        assert_eq!(state.download_list.len(), 2);
    }

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
