//! Windows aday kaydı varsayılan seçimini değiştirmez; seçim Windows Ayarlar'dadır.
use std::{path::Path, process::Command};
use windows::{core::*, Win32::{Foundation::*, System::{Threading::CreateMutexW, Registry::*}, UI::{Shell::*, WindowsAndMessaging::*}}};

pub struct Instance(HANDLE);
impl Drop for Instance { fn drop(&mut self) { unsafe { let _ = CloseHandle(self.0); } } }
#[repr(C)]
pub struct CopyData { pub tag: usize, pub size: u32, pub bytes: *const u8 }
pub const LINK_MESSAGE: u32 = 0x004A;
pub const APP_ID: &str = "Sqywein.Browser";

/// Ayrı veri klasörüyle (SQYWEIN_DATA) çalışan geliştirme/test kopyası mı?
/// Böyleyse kurulu sürümle karışmaz: ayrı görev çubuğu simgesi, sistem kaydı yok.
pub fn is_dev() -> bool {
    std::env::var_os("SQYWEIN_DATA").is_some_and(|d| !d.is_empty())
}
pub fn app_id() -> &'static str {
    if is_dev() { "Sqywein.Browser.Dev" } else { APP_ID }
}
pub fn identify_process() {
    unsafe { let _ = SetCurrentProcessExplicitAppUserModelID(&HSTRING::from(app_id())); }
}
const APP_KEY: GUID = GUID::from_u128(0x9f4c2855_9f79_4b39_a8d0_e1d42de1d5f3);
/// Görev çubuğuna sabitleme: pencere kimliği ve sabitlenince ne çalışacağı.
/// Geliştirme kopyası sabitlenemez; sabitlenen simge hep kurulu sürümü açar.
fn set_window_identity(hwnd: HWND) -> Result<()> {
    use windows::Win32::{System::Com::StructuredStorage::PROPVARIANT, UI::Shell::PropertiesSystem::{IPropertyStore, SHGetPropertyStoreForWindow}};
    let key = |pid| PROPERTYKEY { fmtid: APP_KEY, pid };
    unsafe {
        let store: IPropertyStore = SHGetPropertyStoreForWindow(hwnd)?;
        store.SetValue(&key(5), &PROPVARIANT::from(app_id()))?;
        if is_dev() {
            store.SetValue(&key(9), &PROPVARIANT::from(true))?;
        } else {
            let exe = std::env::current_exe().map_err(|_| Error::from_hresult(E_FAIL))?;
            store.SetValue(&key(2), &PROPVARIANT::from(format!("\"{}\"", exe.display()).as_str()))?;
            store.SetValue(&key(3), &PROPVARIANT::from(format!("{},0", exe.display()).as_str()))?;
            store.SetValue(&key(4), &PROPVARIANT::from("Sqywein"))?;
        }
        store.Commit()
    }
}

fn instance_key() -> String {
    use sha2::{Digest, Sha256};
    match std::env::var_os("SQYWEIN_DATA") {
        Some(path) => {
            let path=std::path::absolute(&path).unwrap_or_else(|_|path.into());
            let normalized=path.to_string_lossy().replace('/',"\\").trim_end_matches('\\').to_lowercase();
            format!("Sqywein.Instance.{}",Sha256::digest(normalized.as_bytes()).iter().map(|b|format!("{b:02x}")).collect::<String>())
        }
        None => "Sqywein.MainInstance".into(),
    }
}
pub fn mark_window(hwnd: HWND) -> Result<()> {
    unsafe { SetPropW(hwnd, &HSTRING::from(instance_key()), Some(HANDLE(1usize as _)))?; }
    if let Err(e) = set_window_identity(hwnd) { crate::log::line(&format!("Pencere kimliği verilemedi: {e}")); }
    Ok(())
}
/// Kurulu (varsayılan veri klasörlü) Sqywein şu an açık mı?
pub fn main_running() -> bool {
    use windows::Win32::System::Threading::{OpenMutexW, SYNCHRONIZATION_SYNCHRONIZE};
    unsafe { OpenMutexW(SYNCHRONIZATION_SYNCHRONIZE, false, w!(r"Local\Sqywein.MainInstance")).map(|h| { let _ = CloseHandle(h); }).is_ok() }
}
fn existing_window() -> Option<HWND> {
    struct Search { key: HSTRING, found: Option<HWND> }
    unsafe extern "system" fn visit(hwnd: HWND, param: LPARAM) -> BOOL {
        let search = &mut *(param.0 as *mut Search);
        if !GetPropW(hwnd, &search.key).is_invalid() { search.found = Some(hwnd); return false.into(); }
        true.into()
    }
    let mut search = Search { key: HSTRING::from(instance_key()), found: None };
    unsafe { let _ = EnumWindows(Some(visit), LPARAM(&mut search as *mut _ as isize)); }
    search.found
}
pub fn launch_arguments(args: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut literal = false;
    args.into_iter().filter(|arg| {
        if !literal && arg == "--" { literal = true; false }
        else { literal || !arg.starts_with('-') }
    }).take(100).filter(|arg| arg.len() <= 8192).collect()
}
pub fn acquire_or_forward() -> Result<Option<Instance>> {
    unsafe {
        let mutex_name=HSTRING::from(format!("Local\\{}", instance_key()));
        let handle = CreateMutexW(None, false, &mutex_name)?;
        let exists = GetLastError() == ERROR_ALREADY_EXISTS;
        let instance = Instance(handle);
        if !exists { return Ok(Some(instance)); }
        for _ in 0..200 {
            if let Some(hwnd) = existing_window() {
                let bytes = serde_json::to_vec(&launch_arguments(std::env::args().skip(1))).unwrap_or_default();
                if bytes.len() > 64 * 1024 { return Err(Error::from_hresult(E_INVALIDARG)); }
                let data = CopyData { tag: 0x535159, size: bytes.len() as u32, bytes: bytes.as_ptr() };
                let mut result = 0;
                let sent = SendMessageTimeoutW(hwnd, LINK_MESSAGE, WPARAM(0), LPARAM(&data as *const _ as isize), SMTO_ABORTIFHUNG, 2000, Some(&mut result));
                if sent.0 == 0 { return Err(Error::from_hresult(E_FAIL)); }
                if result != 1 { std::thread::sleep(std::time::Duration::from_millis(50)); continue; }
                let _ = ShowWindow(hwnd, if IsIconic(hwnd).as_bool() { SW_RESTORE } else { SW_SHOW });
                let _ = SetForegroundWindow(hwnd);
                return Ok(None);
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        Err(Error::from_hresult(E_FAIL))
    }
}

const CAPABILITIES: &str = "Software\\Clients\\StartMenuInternet\\Sqywein\\Capabilities";
const TYPES: &[(&str, &str)] = &[(".htm","SqyweinHTML"),(".html","SqyweinHTML"),(".xhtml","SqyweinHTML"),
    (".pdf","SqyweinPDF"),(".svg","SqyweinImage"),(".webp","SqyweinImage"),(".png","SqyweinImage"),
    (".jpg","SqyweinImage"),(".jpeg","SqyweinImage"),(".gif","SqyweinImage")];
fn registration(exe: &Path) -> Vec<(String, String, String)> {
    let command = format!("\"{}\" -- \"%1\"", exe.display());
    let icon = format!("{},0", exe.display());
    let mut entries = Vec::new();
    let mut add = |key: String, name: &str, value: String| entries.push((key,name.into(),value));
    for (id,label) in [("SqyweinHTML","Sqywein HTML belgesi"),("SqyweinPDF","Sqywein PDF belgesi"),("SqyweinImage","Sqywein görseli")] {
        let key = format!("Software\\Classes\\{id}");
        add(key.clone(), "", label.into());
        add(format!("{key}\\DefaultIcon"), "", icon.clone());
        add(format!("{key}\\shell\\open\\command"), "", command.clone());
        add(format!("{key}\\Application"), "ApplicationName", "Sqywein".into());
        add(format!("{key}\\Application"), "ApplicationDescription", "Sqywein web tarayıcısı".into());
        add(format!("{key}\\Application"), "ApplicationIcon", icon.clone());
        add(format!("{key}\\Application"), "AppUserModelID", APP_ID.into());
        add(key.clone(), "AppUserModelID", APP_ID.into());
    }
    add("Software\\Classes\\SqyweinHTML".into(), "URL Protocol", String::new());
    let browser = "Software\\Clients\\StartMenuInternet\\Sqywein";
    add(browser.into(), "", "Sqywein".into());
    add(format!("{browser}\\DefaultIcon"), "", icon.clone());
    add(format!("{browser}\\shell\\open\\command"), "", format!("\"{}\"", exe.display()));
    add(CAPABILITIES.into(), "ApplicationName", "Sqywein".into());
    add(CAPABILITIES.into(), "ApplicationDescription", "Sqywein web tarayıcısı".into());
    add(CAPABILITIES.into(), "ApplicationIcon", icon);
    add(format!("{CAPABILITIES}\\Startmenu"), "StartMenuInternet", "Sqywein".into());
    for protocol in ["http","https"] { add(format!("{CAPABILITIES}\\URLAssociations"), protocol, "SqyweinHTML".into()); }
    let application = "Software\\Classes\\Applications\\sqywein.exe";
    add(application.into(), "FriendlyAppName", "Sqywein".into());
    add("Software\\Microsoft\\Windows\\CurrentVersion\\App Paths\\sqywein.exe".into(),"",exe.to_string_lossy().into_owned());
    add(format!("{application}\\shell\\open\\command"), "", command);
    for (ext,id) in TYPES {
        add(format!("{CAPABILITIES}\\FileAssociations"), ext, (*id).into());
        add(format!("Software\\Classes\\{ext}\\OpenWithProgids"), id, String::new());
        add(format!("{application}\\SupportedTypes"), ext, String::new());
    }
    add("Software\\RegisteredApplications".into(), "Sqywein", CAPABILITIES.into());
    entries
}
pub(crate) fn write_value(key: &str, name: &str, value: &str) -> Result<()> {
    unsafe {
        let mut handle = HKEY::default();
        RegCreateKeyExW(HKEY_CURRENT_USER, &HSTRING::from(key), None, PCWSTR::null(), REG_OPTION_NON_VOLATILE, KEY_SET_VALUE, None, &mut handle, None).ok()?;
        let data: Vec<u8> = value.encode_utf16().chain(Some(0)).flat_map(u16::to_le_bytes).collect();
        let result = RegSetValueExW(handle, &HSTRING::from(name), None, REG_SZ, Some(&data)).ok();
        let _ = RegCloseKey(handle);
        result
    }
}
pub fn register_browser() -> std::io::Result<()> {
    register_browser_at(&std::env::current_exe()?)
}
pub fn register_browser_at(exe: &Path) -> std::io::Result<()> {
    for (key,name,value) in registration(exe) {
        write_value(&key,&name,&value).map_err(std::io::Error::other)?;
    }
    create_shortcut(exe, &FOLDERID_Programs).map_err(std::io::Error::other)?;
    unsafe { SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, None, None); }
    Ok(())
}
/// Kaldırırken: register_browser_at'in yazdığı her şey silinir.
pub fn unregister_browser() {
    unsafe {
        for key in [r"Software\Classes\SqyweinHTML", r"Software\Classes\SqyweinPDF", r"Software\Classes\SqyweinImage",
            r"Software\Clients\StartMenuInternet\Sqywein", r"Software\Classes\Applications\sqywein.exe",
            r"Software\Microsoft\Windows\CurrentVersion\App Paths\sqywein.exe"] {
            let _ = RegDeleteTreeW(HKEY_CURRENT_USER, &HSTRING::from(key));
        }
        let _ = RegDeleteKeyValueW(HKEY_CURRENT_USER, w!(r"Software\RegisteredApplications"), w!("Sqywein"));
        for (ext,id) in TYPES {
            let _ = RegDeleteKeyValueW(HKEY_CURRENT_USER, &HSTRING::from(format!(r"Software\Classes\{ext}\OpenWithProgids")), &HSTRING::from(*id));
        }
        SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, None, None);
    }
}
pub fn shortcut_path(folder: &GUID) -> Option<std::path::PathBuf> {
    unsafe {
        let ptr = SHGetKnownFolderPath(folder, KF_FLAG_CREATE, None).ok()?;
        let text = ptr.to_string();
        windows::Win32::System::Com::CoTaskMemFree(Some(ptr.0 as _));
        Some(std::path::PathBuf::from(text.ok()?).join("Sqywein.lnk"))
    }
}
/// Başlat menüsü ya da masaüstü kısayolu (kurulu sürümün kimliğiyle).
pub fn create_shortcut(exe: &Path, folder: &GUID) -> Result<()> {
    use windows::Win32::System::Com::{CoCreateInstance,CoInitializeEx,CoUninitialize,IPersistFile,CLSCTX_INPROC_SERVER,COINIT_APARTMENTTHREADED,StructuredStorage::PROPVARIANT};
    use windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore;
    struct Apartment;
    impl Drop for Apartment {fn drop(&mut self){unsafe{CoUninitialize();}}}
    unsafe {
        CoInitializeEx(None,COINIT_APARTMENTTHREADED).ok()?;let _apartment=Apartment;
        let link:IShellLinkW=CoCreateInstance(&ShellLink,None,CLSCTX_INPROC_SERVER)?;
        link.SetPath(&HSTRING::from(exe.to_string_lossy().as_ref()))?;
        link.SetDescription(w!("Sqywein web browser"))?;
        link.SetIconLocation(&HSTRING::from(exe.to_string_lossy().as_ref()),0)?;
        let store:IPropertyStore=link.cast()?;
        let key=PROPERTYKEY{fmtid:APP_KEY,pid:5};
        store.SetValue(&key,&PROPVARIANT::from(APP_ID))?;store.Commit()?;
        let path=shortcut_path(folder).ok_or_else(|| Error::from_hresult(E_FAIL))?;
        link.cast::<IPersistFile>()?.Save(&HSTRING::from(path.to_string_lossy().as_ref()),true)?;
    }
    Ok(())
}
pub fn is_default_browser() -> bool {
    ["http","https"].iter().all(|protocol| unsafe {
        let mut value = [0u16;256]; let mut len = value.len() as u32;
        AssocQueryStringW(ASSOCF_IS_PROTOCOL, ASSOCSTR_PROGID, &HSTRING::from(*protocol), PCWSTR::null(), Some(PWSTR(value.as_mut_ptr())), &mut len).is_ok()
            && String::from_utf16_lossy(&value[..value.iter().position(|c| *c==0).unwrap_or(value.len())]).eq_ignore_ascii_case("SqyweinHTML")
    })
}
pub fn open_default_settings() -> std::io::Result<()> {
    // Geliştirme kopyası kendini sisteme kaydetmez; kurulu sürümün kaydı kullanılır.
    if !is_dev() { register_browser()?; }
    let result = unsafe { ShellExecuteW(None, w!("open"), w!("ms-settings:defaultapps?registeredAppUser=Sqywein"), PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL) };
    if result.0 as isize <= 32 { Err(std::io::Error::other("Windows varsayılan uygulamalar ekranı açılamadı")) } else { Ok(()) }
}
/// Sistem varsayılanı kendimiz olabilir; aynı URL'yi ona geri yollayıp döngü yaratmayız.
pub fn open_compatible_browser(url: &str) -> std::io::Result<()> {
    if !(url.starts_with("https://") || url.starts_with("http://")) { return Err(std::io::Error::other("Yalnızca web adresleri açılabilir")); }
    for root in ["ProgramFiles(x86)","ProgramFiles","LOCALAPPDATA"].into_iter().filter_map(std::env::var_os) {
      for suffix in ["Microsoft/Edge/Application/msedge.exe","Google/Chrome/Application/chrome.exe"] {
        let path=std::path::PathBuf::from(&root).join(suffix);
        if path.is_file() { Command::new(path).arg(url).spawn()?; return Ok(()); }
      }
    }
    Err(std::io::Error::other("Uyumlu başka tarayıcı bulunamadı"))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn registration_is_candidate_only_and_quotes_file_arguments() {
        let entries = registration(Path::new(r"C:\Program Files\Sqywein\sqywein.exe"));
        for (key,_,_) in &entries { assert!(!key.contains("UserChoice")); }
        for protocol in ["http","https"] { assert!(entries.iter().any(|(k,n,v)| k.ends_with("URLAssociations") && n==protocol && v=="SqyweinHTML")); }
        assert!(entries.iter().any(|(k,n,v)| k.ends_with("FileAssociations") && n==".pdf" && v=="SqyweinPDF"));
        assert!(entries.iter().any(|(k,_,v)| k.ends_with("open\\command") && v.ends_with(" -- \"%1\"")));
        assert!(!entries.iter().any(|(k,n,_)| k.ends_with("Classes\\.html") && n.is_empty()));
    }
    #[test]
    fn launch_parameters_preserve_oauth_and_skip_flags() {
        let auth = "http://127.0.0.1:4545/callback?code=a%2Bb&state=x%2Fy";
        assert_eq!(launch_arguments(["--no-rest".into(),"--".into(),auth.into()]),vec![auth]);
    }
}
