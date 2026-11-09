//! Versioned, purpose-bound DPAPI records with durable replacement and recovery.
use serde::{de::DeserializeOwned, Serialize};
use std::{
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
};
use windows::{
    core::HSTRING,
    Win32::Storage::FileSystem::{MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH},
};
use zeroize::Zeroizing;

const MAGIC: &[u8] = b"SQYSEAL\x01";
const MAX_RECORD: u64 = 32 * 1024 * 1024;

/// A named, re-entrant per-record mutex also serializes writers in other windows.
pub struct RecordLock(windows::Win32::Foundation::HANDLE);
impl RecordLock {
    pub fn acquire(path:&Path)->io::Result<Self>{
        use sha2::{Digest,Sha256};
        use windows::Win32::{Foundation::*,System::Threading::*};
        let absolute=std::path::absolute(path)?;
        let key=absolute.to_string_lossy().replace('/',"\\").to_lowercase();
        let hash=Sha256::digest(key.as_bytes()).iter().map(|b|format!("{b:02x}")).collect::<String>();
        let name=format!("Local\\Sqywein.Record.{hash}");
        unsafe{
            let handle=CreateMutexW(None,false,&HSTRING::from(name)).map_err(io::Error::other)?;
            let result=WaitForSingleObject(handle,2000);
            if result==WAIT_OBJECT_0||result==WAIT_ABANDONED{Ok(Self(handle))}else{let _=CloseHandle(handle);Err(io::Error::other("Veri kaydı başka pencerede meşgul"))}
        }
    }
}
impl Drop for RecordLock{fn drop(&mut self){unsafe{let _=windows::Win32::System::Threading::ReleaseMutex(self.0);let _=windows::Win32::Foundation::CloseHandle(self.0);}}}

pub fn is_sealed(path: &Path) -> bool {
    let mut header = [0u8; 8];
    std::fs::File::open(path)
        .and_then(|mut f| f.read_exact(&mut header))
        .is_ok()
        && header == MAGIC
}

pub fn context(path: &Path) -> String {
    format!(
        "Sqywein/storage/v1/{}",
        path.file_name().unwrap_or_default().to_string_lossy()
    )
}

pub fn seal(path: &Path, plain: &[u8]) -> io::Result<Vec<u8>> {
    let encrypted = crate::net::protect_scoped(plain, context(path).as_bytes())
        .ok_or_else(|| io::Error::other("Veri şifrelenemedi"))?;
    let mut bytes = MAGIC.to_vec();
    bytes.extend(encrypted);
    Ok(bytes)
}

pub fn open(path: &Path, bytes: &[u8]) -> Option<Zeroizing<Vec<u8>>> {
    bytes
        .strip_prefix(MAGIC)
        .and_then(|b| crate::net::unprotect_scoped(b, context(path).as_bytes()))
        .map(Zeroizing::new)
}

fn backup(path: &Path) -> PathBuf {
    path.with_extension(format!(
        "{}.bak",
        path.extension().unwrap_or_default().to_string_lossy()
    ))
}

fn bounded_read(path: &Path) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(MAX_RECORD + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_RECORD {
        return Err(io::Error::other("Kayıt boyutu sınırı aşıldı"));
    }
    Ok(bytes)
}

/// Plain JSON is accepted only for migration. New ciphertext never falls back to JSON.
pub fn read_json<T: DeserializeOwned>(path: &Path) -> io::Result<T> {
    let decode = |bytes: &[u8]| -> Option<T> {
        if bytes.starts_with(MAGIC) {
            serde_json::from_slice(&open(path, bytes)?).ok()
        } else {
            serde_json::from_slice(bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes)).ok()
        }
    };
    let main = bounded_read(path);
    if let Ok(bytes) = &main {
        if let Some(value) = decode(bytes) {
            return Ok(value);
        }
    }
    if let Ok(bytes) = bounded_read(&backup(path)) {
        // Backups are always encrypted and belong to the original record.
        if let Some(value) = open(path, &bytes).and_then(|b| serde_json::from_slice(&b).ok()) {
            return Ok(value);
        }
    }
    match main {
        Err(e) if e.kind() == io::ErrorKind::NotFound && !backup(path).exists() => Err(e),
        _ => Err(io::Error::other("Kayıt okunamadı; üzerine yazılmadı")),
    }
}

fn replace(temp: &Path, path: &Path) -> io::Result<()> {
    unsafe {
        MoveFileExW(
            &HSTRING::from(temp.as_os_str()),
            &HSTRING::from(path.as_os_str()),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    }
    .map_err(io::Error::other)
}

fn write_synced(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = fs::File::create(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

pub fn write_sealed(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if bytes.len() as u64 > MAX_RECORD {
        return Err(io::Error::other("Kayıt boyutu sınırı aşıldı"));
    }
    if open(path, bytes).is_none() {
        return Err(io::Error::other("Geçersiz şifreli kayıt"));
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temp = path.with_extension("sealed.tmp");
    let result = (|| {
        write_synced(&temp, bytes)?;
        if let Ok(old) = bounded_read(path) {
            if open(path, &old).is_some() {
                let bak = backup(path);
                let bak_temp = path.with_extension("backup.tmp");
                write_synced(&bak_temp, &old)?;
                replace(&bak_temp, &bak)?;
            }
        }
        replace(&temp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result
}

pub fn write_json<T: Serialize>(path: &Path, value: &T) -> io::Result<()> {
    let plain = Zeroizing::new(serde_json::to_vec(value)?);
    write_sealed(path, &seal(path, &plain)?)
}

/// Previous versions used unscoped DPAPI for cookies and the password vault.
pub fn read_secret_json<T: DeserializeOwned + Serialize>(path: &Path) -> io::Result<T> {
    let bytes = match bounded_read(path) {
        Ok(b) => b,
        Err(_) => return read_json(path),
    };
    if bytes.starts_with(MAGIC) {
        return read_json(path);
    }
    let Some(plain) = crate::net::unprotect(&bytes) else {
        let saved = bounded_read(&backup(path))
            .map_err(|_| io::Error::other("Kayıt ve yedeği doğrulanamadı"))?;
        return serde_json::from_slice(
            &open(path, &saved).ok_or_else(|| io::Error::other("Kayıt doğrulanamadı"))?,
        )
        .map_err(io::Error::other);
    };
    let plain = Zeroizing::new(plain);
    let value: T = serde_json::from_slice(&plain)?;
    write_json(path, &value)?;
    Ok(value)
}

pub fn remove(path: &Path) {
    for p in [
        path.to_owned(),
        backup(path),
        path.with_extension("sealed.tmp"),
        path.with_extension("backup.tmp"),
    ] {
        let _ = fs::remove_file(p);
    }
}

pub fn forget_backup(path: &Path) {
    let _ = fs::remove_file(backup(path));
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn migration_recovery_tampering_and_profile_binding() {
        let dir = std::env::temp_dir().join(format!("sqywein-seal-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("state.json");
        fs::write(&path, br#"{"secret":"legacy"}"#).unwrap();
        let first: serde_json::Value = read_json(&path).unwrap();
        assert_eq!(first["secret"], "legacy");
        write_json(&path, &first).unwrap();
        let bytes = fs::read(&path).unwrap();
        assert!(!bytes.windows(6).any(|b| b == b"legacy"));
        assert!(open(&dir.join("other-profile.json"), &bytes).is_none());
        write_json(&path, &serde_json::json!({"secret":"new"})).unwrap();
        let mut corrupt = fs::read(&path).unwrap();
        let end = corrupt.len() - 1;
        corrupt[end] ^= 1;
        fs::write(&path, corrupt).unwrap();
        let recovered: serde_json::Value = read_json(&path).unwrap();
        assert_eq!(recovered, first);
        remove(&path);
        let _ = fs::remove_dir(dir);
    }
}
