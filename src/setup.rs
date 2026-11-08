//! Kurulum ve kaldırma. Aynı exe "SqyweinSetup.exe" adıyla (ya da --install
//! ile) açılınca kendini %LOCALAPPDATA%\Programs\Sqywein'e kurar: Başlat menüsü
//! ve masaüstü kısayolu, "Yüklü uygulamalar" kaydı ve tarayıcı adaylığı.
//! Yönetici izni gerekmez; her şey yalnızca bu kullanıcı içindir.

use crate::{integration, update::CURRENT};
use std::{os::windows::process::CommandExt, path::{Path, PathBuf}, process::Command};
use windows::{
    core::*,
    Win32::{System::Registry::*, UI::{Shell::*, WindowsAndMessaging::*}},
};

const UNINSTALL: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\Sqywein";
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub enum Mode {
    Install { silent: bool },
    Uninstall { silent: bool },
}

/// Komut satırı ve exe adına göre kurulum modu.
pub fn mode() -> Option<Mode> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let silent = args.iter().any(|a| a == "--silent");
    if args.iter().any(|a| a == "--uninstall") {
        return Some(Mode::Uninstall { silent });
    }
    let setup_name = std::env::current_exe().ok()
        .and_then(|p| p.file_stem().map(|s| s.to_string_lossy().to_lowercase()))
        .is_some_and(|s| s.contains("setup"));
    (setup_name || args.iter().any(|a| a == "--install")).then_some(Mode::Install { silent })
}

pub fn install_dir() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(|| ".".into());
    base.join("Programs").join("Sqywein")
}

fn message(text: &str, flags: MESSAGEBOX_STYLE) -> MESSAGEBOX_RESULT {
    unsafe { MessageBoxW(None, &HSTRING::from(text), w!("Sqywein Kurulum"), flags) }
}

/// Açık Sqywein kapatılana kadar sorar; vazgeçilirse false.
fn wait_closed(silent: bool) -> bool {
    while integration::main_running() {
        if silent || message("Sqywein şu an açık. Kapatıp \"Yeniden dene\"ye bas.", MB_RETRYCANCEL | MB_ICONWARNING) != IDRETRY {
            return false;
        }
    }
    true
}

/// Çıkış kodu döner: 0 tamam, 1 vazgeçildi, 2 hata.
pub fn run(mode: Mode) -> i32 {
    let (result, silent) = match mode {
        Mode::Install { silent } => (install(silent), silent),
        Mode::Uninstall { silent } => (uninstall(silent), silent),
    };
    match result {
        Ok(()) => 0,
        Err(e) if e.is_empty() => 1,
        Err(e) => {
            if !silent {
                message(&format!("İşlem tamamlanamadı:\n{e}"), MB_OK | MB_ICONERROR);
            }
            2
        }
    }
}

fn install(silent: bool) -> std::result::Result<(), String> {
    let source = std::env::current_exe().map_err(|e| e.to_string())?;
    let dir = install_dir();
    let target = dir.join("sqywein.exe");
    if !silent {
        let text = format!(
            "Sqywein v{CURRENT} {}\n\nKonum: {}\nGeçmişin, şifrelerin ve ayarların korunur.",
            if target.exists() { "kurulu sürümün yerine konulsun mu?" } else { "bu bilgisayara kurulsun mu?" },
            dir.display()
        );
        if message(&text, MB_OKCANCEL | MB_ICONQUESTION) != IDOK {
            return Err(String::new());
        }
    }
    if !wait_closed(silent) {
        return Err(if silent { "Sqywein açık".into() } else { String::new() });
    }
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    if !same_file(&source, &target) {
        // Önce tam kopya hazırlanır; eski exe ancak o zaman kenara alınır.
        let staged = dir.join("sqywein.next.exe");
        let old = dir.join("sqywein.old.exe");
        std::fs::copy(&source, &staged).map_err(|e| format!("Kopyalanamadı: {e}"))?;
        let _ = std::fs::remove_file(&old);
        if target.exists() {
            std::fs::rename(&target, &old).map_err(|e| e.to_string())?;
        }
        if let Err(e) = std::fs::rename(&staged, &target) {
            let _ = std::fs::rename(&old, &target);
            return Err(e.to_string());
        }
        let _ = std::fs::remove_file(&old);
    }
    integration::register_browser_at(&target).map_err(|e| e.to_string())?;
    integration::create_shortcut(&target, &FOLDERID_Desktop).map_err(|e| e.to_string())?;
    write_uninstall_entry(&target).map_err(|e| e.to_string())?;
    if !silent {
        let _ = Command::new(&target).spawn();
    }
    Ok(())
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

fn write_dword(key: &str, name: &str, value: u32) -> Result<()> {
    unsafe {
        let mut handle = HKEY::default();
        RegCreateKeyExW(HKEY_CURRENT_USER, &HSTRING::from(key), None, PCWSTR::null(), REG_OPTION_NON_VOLATILE, KEY_SET_VALUE, None, &mut handle, None).ok()?;
        let result = RegSetValueExW(handle, &HSTRING::from(name), None, REG_DWORD, Some(&value.to_le_bytes())).ok();
        let _ = RegCloseKey(handle);
        result
    }
}

/// Ayarlar > Uygulamalar > Yüklü uygulamalar listesindeki kayıt.
fn write_uninstall_entry(exe: &Path) -> Result<()> {
    let quoted = format!("\"{}\"", exe.display());
    let size_kb = std::fs::metadata(exe).map_or(0, |m| (m.len() / 1024) as u32);
    for (name, value) in [
        ("DisplayName", "Sqywein".to_string()),
        ("DisplayVersion", CURRENT.to_string()),
        ("Publisher", "Sqyrix".to_string()),
        ("DisplayIcon", format!("{},0", exe.display())),
        ("InstallLocation", install_dir().display().to_string()),
        ("UninstallString", format!("{quoted} --uninstall")),
        ("QuietUninstallString", format!("{quoted} --uninstall --silent")),
        ("URLInfoAbout", format!("https://github.com/{}", crate::update::REPO)),
    ] {
        integration::write_value(UNINSTALL, name, &value)?;
    }
    write_dword(UNINSTALL, "NoModify", 1)?;
    write_dword(UNINSTALL, "NoRepair", 1)?;
    write_dword(UNINSTALL, "EstimatedSize", size_kb)
}

fn uninstall(silent: bool) -> std::result::Result<(), String> {
    if !silent && message("Sqywein bu bilgisayardan kaldırılsın mı?", MB_OKCANCEL | MB_ICONQUESTION) != IDOK {
        return Err(String::new());
    }
    if !wait_closed(silent) {
        return Err(if silent { "Sqywein açık".into() } else { String::new() });
    }
    let data = std::env::var_os("LOCALAPPDATA").map(|b| PathBuf::from(b).join("Sqywein"));
    let remove_data = !silent
        && data.as_ref().is_some_and(|d| d.exists())
        && message(
            "Tarama verileri de (geçmiş, şifreler, çerezler, ayarlar) silinsin mi?\n\n\"Hayır\" dersen yeniden kurduğunda kaldığın yerden devam edersin.",
            MB_YESNO | MB_ICONQUESTION | MB_DEFBUTTON2,
        ) == IDYES;
    integration::unregister_browser();
    for folder in [&FOLDERID_Programs, &FOLDERID_Desktop] {
        if let Some(path) = integration::shortcut_path(folder) {
            let _ = std::fs::remove_file(path);
        }
    }
    unsafe {
        let _ = RegDeleteTreeW(HKEY_CURRENT_USER, &HSTRING::from(UNINSTALL));
    }
    // Çalışan exe kendini silemez: kapanınca gizli bir cmd klasörü siler.
    let mut script = format!("ping -n 3 127.0.0.1 >nul & rmdir /s /q \"{}\"", install_dir().display());
    if let (true, Some(data)) = (remove_data, &data) {
        script.push_str(&format!(" & rmdir /s /q \"{}\"", data.display()));
    }
    Command::new("cmd").raw_arg(format!("/c {script}")).creation_flags(CREATE_NO_WINDOW).spawn().map_err(|e| e.to_string())?;
    if !silent {
        message("Sqywein kaldırıldı.", MB_OK | MB_ICONINFORMATION);
    }
    Ok(())
}
