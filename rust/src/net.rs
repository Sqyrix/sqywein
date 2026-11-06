//! Küçük ağ ve güvenlik yardımcıları (arka plan iş parçacıklarında çağrılır).

use windows::{
    core::*,
    Win32::{
        Foundation::{LocalFree, HLOCAL},
        Security::Cryptography::{CryptProtectData, CryptUnprotectData, CRYPT_INTEGER_BLOB},
        System::Com::{
            CoInitializeEx, IStream,
            Urlmon::URLOpenBlockingStreamW,
            COINIT_MULTITHREADED,
        },
    },
};

/// Arka plan iş parçacığının başında bir kez çağrılır.
pub fn init_thread() {
    let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
}

/// Adresin içeriğini belleğe indirir; `max` baytı aşarsa keser.
pub fn fetch(url: &str, max: usize) -> Option<Vec<u8>> {
    unsafe {
        let mut stream: Option<IStream> = None;
        URLOpenBlockingStreamW(None, &HSTRING::from(url), &mut stream, 0, None).ok()?;
        let stream = stream?;
        let mut out = Vec::new();
        let mut buf = [0u8; 16 * 1024];
        loop {
            let mut read = 0u32;
            let hr = stream.Read(buf.as_mut_ptr() as _, buf.len() as u32, Some(&mut read));
            if hr.is_err() || read == 0 {
                break;
            }
            out.extend_from_slice(&buf[..read as usize]);
            if out.len() >= max {
                break;
            }
        }
        Some(out)
    }
}

fn blob(data: &[u8]) -> CRYPT_INTEGER_BLOB {
    CRYPT_INTEGER_BLOB { cbData: data.len() as u32, pbData: data.as_ptr() as *mut u8 }
}

/// Windows kullanıcı hesabına bağlı şifreleme (DPAPI): yalnızca bu kullanıcı çözebilir.
pub fn protect(data: &[u8]) -> Option<Vec<u8>> {
    unsafe {
        let mut out = CRYPT_INTEGER_BLOB::default();
        CryptProtectData(&blob(data), w!("Sqywein"), None, None, None, 0, &mut out).ok()?;
        let bytes = std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec();
        let _ = LocalFree(Some(HLOCAL(out.pbData as _)));
        Some(bytes)
    }
}

pub fn unprotect(data: &[u8]) -> Option<Vec<u8>> {
    unsafe {
        let mut out = CRYPT_INTEGER_BLOB::default();
        CryptUnprotectData(&blob(data), None, None, None, None, 0, &mut out).ok()?;
        let bytes = std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec();
        let _ = LocalFree(Some(HLOCAL(out.pbData as _)));
        Some(bytes)
    }
}

/// OpenSearch öneri yanıtı: `["sorgu", ["öneri1", "öneri2", ...]]`.
pub fn parse_suggestions(bytes: &[u8]) -> Vec<String> {
    let text = String::from_utf8_lossy(bytes);
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else { return Vec::new() };
    v.get(1)
        .and_then(|a| a.as_array())
        .map(|a| a.iter().filter_map(|s| s.as_str().map(str::to_string)).collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dpapi_gidis_donus() {
        let secret = b"oturum=abc123";
        let sealed = protect(secret).unwrap();
        assert_ne!(&sealed[..], &secret[..]);
        assert_eq!(unprotect(&sealed).unwrap(), secret);
    }

    /// Ağ gerektirir: `cargo test -- --ignored canli_oneriler`
    #[test]
    #[ignore]
    fn canli_oneriler() {
        init_thread();
        let url = format!("{}{}", crate::state::Settings::default().suggest_url(), crate::url::encode("kedi"));
        let list = parse_suggestions(&fetch(&url, 64 * 1024).expect("yanıt yok"));
        assert!(!list.is_empty(), "öneri gelmedi");
        println!("{list:?}");
    }

    #[test]
    fn oneriler() {
        let list = parse_suggestions(r#"["kedi",["kedi maması","kedi cinsleri"],[],{}]"#.as_bytes());
        assert_eq!(list, vec!["kedi maması", "kedi cinsleri"]);
        assert!(parse_suggestions(b"bozuk").is_empty());
    }
}
