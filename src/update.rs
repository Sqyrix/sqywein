//! GitHub sürümleriyle otomatik güncelleme.
//!
//! Sqywein günde bir kez https://github.com/Sqyrix/sqywein/releases'teki son
//! sürüme bakar. Daha yenisi varsa `sqywein.exe`'yi ve yanındaki
//! `sqywein.exe.sha256` dosyasını arka planda indirir, özeti doğrular ve
//! araç çubuğunda "Güncelle" düğmesini gösterir. Basınca çalışan exe
//! `sqywein.old.exe` olarak kenara alınır (Windows çalışan dosyanın adını
//! değiştirmeye izin verir), yenisi yerine konur ve tarayıcı oturumuyla
//! birlikte yeniden açılır. Eski dosya bir sonraki açılışta silinir.

use crate::{net, state};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

pub const REPO: &str = "Sqyrix/sqywein";
pub const CURRENT: &str = env!("CARGO_PKG_VERSION");
const ASSET: &str = "sqywein.exe";

#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    /// Zaten en yeni sürüm.
    Latest,
    /// Yeni sürüm indirildi ve doğrulandı: (sürüm, dosya).
    Ready(String, PathBuf),
    Failed(String),
}

/// "v1.2.3" / "1.2.3" → [1, 2, 3]. Sayı olmayan kısımlar 0 sayılır.
pub fn parse_version(v: &str) -> [u32; 3] {
    let mut out = [0u32; 3];
    for (slot, part) in out.iter_mut().zip(v.trim().trim_start_matches(['v', 'V']).split(['.', '-', '+'])) {
        *slot = part.parse().unwrap_or(0);
    }
    out
}

pub fn is_newer(candidate: &str, current: &str) -> bool {
    parse_version(candidate) > parse_version(current)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

/// `.sha256` dosyası: "abc123…  sqywein.exe" ya da yalnızca özet.
fn expected_hash(text: &str) -> Option<String> {
    let h = text.split_whitespace().next()?.to_ascii_lowercase();
    (h.len() == 64 && h.bytes().all(|b| b.is_ascii_hexdigit())).then_some(h)
}

fn update_dir() -> PathBuf {
    state::root().join("update")
}

/// Son sürümü denetler; yeniyse indirip doğrular (arka plan iş parçacığında çağrılır).
pub fn check() -> Outcome {
    let api = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let Some(body) = net::fetch(&api, 1024 * 1024) else { return Outcome::Failed("GitHub'a ulaşılamadı".into()) };
    let Ok(release) = serde_json::from_slice::<Value>(&body) else { return Outcome::Failed("Sürüm bilgisi okunamadı".into()) };
    let Some(tag) = release["tag_name"].as_str() else {
        return Outcome::Failed("Henüz yayınlanmış sürüm yok".into());
    };
    if !is_newer(tag, CURRENT) {
        return Outcome::Latest;
    }
    let asset_url = |name: &str| -> Option<String> {
        release["assets"].as_array()?.iter().find(|a| a["name"] == name)?["browser_download_url"].as_str().map(str::to_string)
    };
    let (Some(exe_url), Some(sum_url)) = (asset_url(ASSET), asset_url(&format!("{ASSET}.sha256"))) else {
        return Outcome::Failed("Sürümde exe ya da özet dosyası yok".into());
    };
    let Some(expected) = net::fetch(&sum_url, 4096).and_then(|b| expected_hash(&String::from_utf8_lossy(&b))) else {
        return Outcome::Failed("Özet dosyası okunamadı".into());
    };
    let Some(exe) = net::fetch(&exe_url, 200 * 1024 * 1024) else { return Outcome::Failed("İndirilemedi".into()) };
    // Dosya bozuk ya da değiştirilmişse kurulmaz.
    if sha256_hex(&exe) != expected {
        return Outcome::Failed("İndirilen dosya doğrulanamadı".into());
    }
    let version = tag.trim_start_matches(['v', 'V']).to_string();
    let dir = update_dir();
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join(format!("sqywein-{version}.exe"));
    match std::fs::write(&path, &exe) {
        Ok(()) => Outcome::Ready(version, path),
        Err(e) => Outcome::Failed(format!("Kaydedilemedi: {e}")),
    }
}

fn old_exe(current: &Path) -> PathBuf {
    current.with_file_name("sqywein.old.exe")
}

/// Yeni exe'yi çalışan exe'nin yerine koyar. Başarılıysa yeni exe'nin yolu döner.
pub fn install(new_exe: &Path) -> std::io::Result<PathBuf> {
    let current = std::env::current_exe()?;
    let old = old_exe(&current);
    let _ = std::fs::remove_file(&old);
    std::fs::rename(&current, &old)?;
    if let Err(e) = std::fs::copy(new_exe, &current) {
        // Yarım kalırsa eski sürüm geri konur.
        let _ = std::fs::rename(&old, &current);
        return Err(e);
    }
    let _ = std::fs::remove_file(new_exe);
    Ok(current)
}

/// Açılışta: önceki güncellemeden kalan dosyalar silinir.
pub fn cleanup() {
    if let Ok(current) = std::env::current_exe() {
        let _ = std::fs::remove_file(old_exe(&current));
    }
    if let Ok(entries) = std::fs::read_dir(update_dir()) {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            // Şu anki ya da daha eski sürümün indirilmiş kopyası artık gereksiz.
            if let Some(v) = name.strip_prefix("sqywein-").and_then(|n| n.strip_suffix(".exe")) {
                if !is_newer(v, CURRENT) {
                    let _ = std::fs::remove_file(e.path());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn surum_karsilastirma() {
        assert!(is_newer("v0.2.0", "0.1.0"));
        assert!(is_newer("0.10.0", "0.9.9"));
        assert!(!is_newer("v0.1.0", "0.1.0"));
        assert!(!is_newer("0.0.9", "0.1.0"));
        assert_eq!(parse_version("v1.2.3-beta"), [1, 2, 3]);
    }

    #[test]
    fn ozet() {
        assert_eq!(sha256_hex(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        assert_eq!(expected_hash("BA7816BF8F01CFEA414140DE5DAE2223B00361A396177A9CB410FF61F20015AD  sqywein.exe").as_deref(),
            Some("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"));
        assert!(expected_hash("bozuk").is_none());
    }
}
