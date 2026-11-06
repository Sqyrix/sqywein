//! Profil başına DPAPI ile şifrelenen parola kasası.

use serde::{Deserialize, Serialize};
use std::{fs, io, net::IpAddr, path::PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Credential {
    pub id: u64,
    pub origin: String,
    pub username: String,
    pub password: String,
    pub note: String,
    pub created: String,
    pub last_used: String,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Upsert {
    Added,
    Updated,
    Unchanged,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct ImportResult {
    pub added: usize,
    pub updated: usize,
    pub skipped: usize,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Vault {
    items: Vec<Credential>,
    blocked: Vec<String>,
    next_id: u64,
    #[serde(skip)]
    dirty: bool,
}

impl Vault {
    pub fn load(profile: &str) -> Vault {
        let mut vault: Self = fs::read(path(profile))
            .ok()
            .and_then(|bytes| crate::net::unprotect(&bytes))
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        vault.next_id = vault.next_id.max(
            vault
                .items
                .iter()
                .map(|c| c.id)
                .max()
                .unwrap_or(0)
                .saturating_add(1),
        );
        vault
    }

    pub fn save(&mut self, profile: &str) -> io::Result<()> {
        let target = path(profile);
        fs::create_dir_all(target.parent().unwrap())?;
        let bytes = crate::net::protect(&serde_json::to_vec(self)?)
            .ok_or_else(|| io::Error::other("Kasa şifrelenemedi"))?;
        let temp = target.with_extension("bin.tmp");
        let result = (|| {
            use io::Write;
            let mut file = fs::File::create(&temp)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            drop(file);
            fs::rename(&temp, &target)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temp);
        }
        result?;
        self.dirty = false;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.items.len()
    }
    pub fn list(&self, query: &str) -> Vec<&Credential> {
        let query = fold(query.trim());
        let mut items: Vec<_> = self
            .items
            .iter()
            .filter(|c| fold(&c.origin).contains(&query) || fold(&c.username).contains(&query))
            .collect();
        items.sort_by_key(|c| (site(&c.origin), fold(&c.username), c.id));
        items
    }
    pub fn get(&self, id: u64) -> Option<&Credential> {
        self.items.iter().find(|c| c.id == id)
    }

    pub fn matches(&self, page_url: &str) -> Vec<&Credential> {
        let Some(page) = origin(page_url) else {
            return Vec::new();
        };
        if !page.starts_with("http://") && !page.starts_with("https://") {
            return Vec::new();
        }
        let host = site(&page);
        let mut items: Vec<_> = self
            .items
            .iter()
            .filter(|c| {
                if !c.origin.starts_with("http://") && !c.origin.starts_with("https://") {
                    return false;
                }
                if page.starts_with("http://") && c.origin.starts_with("https://") {
                    return false;
                }
                let other = site(&c.origin);
                other == host || domain(&other) == domain(&host)
            })
            .collect();
        items.sort_by_key(|c| {
            (
                site(&c.origin) != host,
                c.origin != page,
                std::cmp::Reverse(&c.last_used),
                fold(&c.username),
                c.id,
            )
        });
        items
    }

    pub fn upsert(&mut self, page_url: &str, username: &str, password: &str) -> Upsert {
        let Some(origin) = origin(page_url) else {
            return Upsert::Unchanged;
        };
        if password.is_empty() {
            return Upsert::Unchanged;
        }
        if let Some(c) = self
            .items
            .iter_mut()
            .find(|c| c.origin == origin && c.username == username)
        {
            if c.password == password {
                return Upsert::Unchanged;
            }
            c.password = password.into();
            self.dirty = true;
            return Upsert::Updated;
        }
        let id = self.next_id.max(1);
        let Some(next) = id.checked_add(1) else {
            return Upsert::Unchanged;
        };
        self.next_id = next;
        self.items.push(Credential {
            id,
            origin,
            username: username.into(),
            password: password.into(),
            note: String::new(),
            created: now(),
            last_used: String::new(),
        });
        self.dirty = true;
        Upsert::Added
    }

    pub fn update(&mut self, id: u64, username: &str, password: &str) -> bool {
        if password.is_empty() {
            return false;
        }
        let Some(old) = self.get(id) else {
            return false;
        };
        if self
            .items
            .iter()
            .any(|c| c.id != id && c.origin == old.origin && c.username == username)
        {
            return false;
        }
        let c = self.items.iter_mut().find(|c| c.id == id).unwrap();
        if c.username != username || c.password != password {
            c.username = username.into();
            c.password = password.into();
            self.dirty = true;
        }
        true
    }
    pub fn remove(&mut self, id: u64) -> bool {
        let len = self.items.len();
        self.items.retain(|c| c.id != id);
        let removed = len != self.items.len();
        self.dirty |= removed;
        removed
    }
    pub fn touch(&mut self, id: u64) {
        if let Some(c) = self.items.iter_mut().find(|c| c.id == id) {
            c.last_used = now();
            self.dirty = true;
        }
    }
    pub fn never(&self, page_url: &str) -> bool {
        origin(page_url).is_some_and(|o| self.blocked.contains(&site(&o)))
    }
    pub fn set_never(&mut self, page_url: &str) {
        if let Some(o) = origin(page_url) {
            let host = site(&o);
            if !self.blocked.contains(&host) {
                self.blocked.push(host);
                self.dirty = true;
            }
        }
    }

    pub fn import_csv(&mut self, text: &str) -> ImportResult {
        let mut result = ImportResult::default();
        let mut rows = csv_rows(text.trim_start_matches('\u{feff}')).into_iter();
        let Some(Ok(header)) = rows.next() else {
            return result;
        };
        let column = |name: &str| {
            header
                .iter()
                .position(|h| h.trim().eq_ignore_ascii_case(name))
        };
        let (Some(url), Some(user), Some(pass)) =
            (column("url"), column("username"), column("password"))
        else {
            result.skipped = rows.count();
            return result;
        };
        let note = column("note");
        for row in rows {
            let Ok(row) = row else {
                result.skipped += 1;
                continue;
            };
            let value = |i| row.get(i).map(String::as_str).unwrap_or("");
            let Some(o) = origin(value(url)) else {
                result.skipped += 1;
                continue;
            };
            if value(pass).is_empty() {
                result.skipped += 1;
                continue;
            }
            let change = self.upsert(&o, value(user), value(pass));
            let mut changed_note = false;
            if let Some(i) = note {
                if let Some(c) = self
                    .items
                    .iter_mut()
                    .find(|c| c.origin == o && c.username == value(user))
                {
                    if c.note != value(i) {
                        c.note = value(i).into();
                        changed_note = true;
                        self.dirty = true;
                    }
                }
            }
            match change {
                Upsert::Added => result.added += 1,
                Upsert::Updated => result.updated += 1,
                Upsert::Unchanged if changed_note => result.updated += 1,
                Upsert::Unchanged => result.skipped += 1,
            }
        }
        result
    }
    pub fn export_csv(&self) -> String {
        let mut csv = String::from("name,url,username,password,note\r\n");
        for c in self.list("") {
            csv.push_str(
                &[
                    site(&c.origin),
                    c.origin.clone(),
                    c.username.clone(),
                    c.password.clone(),
                    c.note.clone(),
                ]
                .iter()
                .map(|s| csv_quote(s))
                .collect::<Vec<_>>()
                .join(","),
            );
            csv.push_str("\r\n");
        }
        csv
    }
}

fn path(profile: &str) -> PathBuf {
    // Profil kimliği dosya yolundan dışarı çıkamaz.
    let profile: String = profile
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c.to_string()
            } else {
                format!("%{:X}", c as u32)
            }
        })
        .collect();
    crate::state::root().join(format!("passwords-{profile}.bin"))
}
fn now() -> String {
    let mut value = crate::history::now_iso();
    if !value.ends_with('Z') {
        value.push('Z');
    }
    value
}
fn fold(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'İ' | 'I' | 'ı' => 'i',
            _ => c,
        })
        .flat_map(char::to_lowercase)
        .collect()
}

// Yalnızca web ve Android kaynakları; kullanıcı bilgisi ve yol saklanmaz.
fn origin(url: &str) -> Option<String> {
    let (scheme, rest) = url.trim().split_once("://")?;
    let scheme = scheme.to_ascii_lowercase();
    if !["http", "https", "android"].contains(&scheme.as_str()) {
        return None;
    }
    let authority = rest.split(['/', '?', '#']).next()?;
    if authority.is_empty()
        || authority
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || c == '\\')
    {
        return None;
    }
    if scheme == "android" {
        return Some(format!("android://{authority}"));
    }
    let authority = authority.rsplit('@').next()?.to_ascii_lowercase();
    let (host, port) = if authority.starts_with('[') {
        let end = authority.find(']')?;
        authority[1..end].parse::<std::net::Ipv6Addr>().ok()?;
        let tail = &authority[end + 1..];
        (
            &authority[..=end],
            if tail.is_empty() {
                None
            } else {
                Some(tail.strip_prefix(':')?)
            },
        )
    } else {
        let (h, p) = authority
            .split_once(':')
            .map_or((authority.as_str(), None), |(h, p)| (h, Some(p)));
        if h.is_empty()
            || !h
                .chars()
                .all(|c| c.is_alphanumeric() || c == '.' || c == '-')
            || h.split('.').any(|l| l.is_empty())
        {
            return None;
        }
        (h, p)
    };
    let port = match port {
        Some(p) => {
            let p = p.parse::<u16>().ok()?;
            if p == 0 {
                return None;
            }
            if (scheme == "http" && p == 80) || (scheme == "https" && p == 443) {
                String::new()
            } else {
                format!(":{p}")
            }
        }
        None => String::new(),
    };
    Some(format!("{scheme}://{host}{port}"))
}
fn site(origin: &str) -> String {
    let authority = origin.split_once("://").map_or(origin, |(_, s)| s);
    if authority.starts_with('[') {
        return authority
            .split(']')
            .next()
            .unwrap_or(authority)
            .trim_start_matches('[')
            .into();
    }
    authority.split(':').next().unwrap_or(authority).to_owned()
}
fn domain(host: &str) -> String {
    if host.parse::<IpAddr>().is_ok() {
        return host.into();
    }
    let labels: Vec<_> = host.split('.').collect();
    let count = if labels.len() >= 3
        && labels.last().is_some_and(|s| s.len() == 2)
        && [
            "com", "net", "org", "gov", "edu", "co", "ac", "k12", "gen", "bel", "web", "biz",
            "info", "tv",
        ]
        .contains(&labels[labels.len() - 2])
    {
        3
    } else {
        2
    };
    labels[labels.len().saturating_sub(count)..].join(".")
}
fn csv_quote(s: &str) -> String {
    if s.contains([',', '"', '\r', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.into()
    }
}

// Hatalı satır atlanır; tırnak içindeki satır sonları korunur.
fn csv_rows(text: &str) -> Vec<Result<Vec<String>, ()>> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut chars = text.chars().peekable();
    let mut quoted = false;
    let mut closed = false;
    let mut bad = false;
    let mut started = false;
    while let Some(c) = chars.next() {
        started = true;
        if quoted {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    quoted = false;
                    closed = true;
                }
            } else {
                field.push(c);
            }
        } else {
            match c {
                '"' if field.is_empty() && !closed => quoted = true,
                ',' => {
                    row.push(std::mem::take(&mut field));
                    closed = false;
                }
                '\r' | '\n' => {
                    if c == '\r' && chars.peek() == Some(&'\n') {
                        chars.next();
                    }
                    row.push(std::mem::take(&mut field));
                    rows.push(if bad {
                        Err(())
                    } else {
                        Ok(std::mem::take(&mut row))
                    });
                    row.clear();
                    bad = false;
                    closed = false;
                    started = false;
                }
                _ => {
                    if closed || c == '"' {
                        bad = true;
                    }
                    field.push(c);
                }
            }
        }
    }
    if started {
        row.push(field);
        rows.push(if bad || quoted { Err(()) } else { Ok(row) });
    }
    rows
}

#[cfg(test)]
mod tests {
    /// Ekran görüntüsü için örnek kasa: `cargo test -- --ignored ornek_kasa`
    #[test]
    #[ignore]
    fn ornek_kasa() {
        let mut vault = Vault::default();
        vault.import_csv("name,url,username,password,note
amazon.com.tr,https://www.amazon.com.tr/,emir@ornek.com,a1,
amazon.com.tr,https://www.amazon.com.tr/,is@ornek.com,a2,
github.com,https://github.com/login,sqyrix,g1,
youtube.com,https://accounts.google.com/,emir@gmail.com,y1,
adobe.com,https://account.adobe.com/,emir@ornek.com,d1,
192.168.1.1,http://192.168.1.1/,admin,r1,
");
        vault.save("default").unwrap();
    }

    use super::*;
    #[test]
    fn csv_edge_cases() {
        let mut v = Vault::default();
        let r = v.import_csv("\u{feff}password,note,username,url,name\r\n\"p,\"\"x\"\"\",\"satır\r\niki\",İpek,https://EXAMPLE.com/path,A\r\nnew,,İpek,https://example.com/,B\n,,x,https://a.com,A\nsecret,,x,,A\n\"unfinished");
        assert_eq!(
            r,
            ImportResult {
                added: 1,
                updated: 1,
                skipped: 3
            }
        );
        assert_eq!(v.list("ipek")[0].password, "new");
        assert_eq!(
            v.import_csv("name,url,username,password\nA,https://a.com,u,p")
                .added,
            1
        );
        assert!(
            csv_rows("a,\"b\r\nc\",\"d\"\"e\"\r\n")[0].as_ref().unwrap()
                == &vec!["a", "b\r\nc", "d\"e"]
        );
    }
    #[test]
    fn matching_rules() {
        let mut v = Vault::default();
        for url in [
            "https://a.example.com",
            "https://example.com",
            "http://a.example.com",
            "https://other.com",
            "android://hash@com.example",
            "https://giris.turkiye.gov.tr",
            "https://b.gov.tr",
            "https://127.0.0.1",
            "https://127.0.0.2",
        ] {
            v.upsert(url, "u", "p");
        }
        let m = v.matches("https://a.example.com/login");
        assert_eq!(m.len(), 3);
        assert_eq!(m[0].origin, "https://a.example.com");
        assert_eq!(m[1].origin, "http://a.example.com");
        assert_eq!(v.matches("http://a.example.com").len(), 1);
        assert_eq!(v.matches("https://www.turkiye.gov.tr").len(), 1);
        assert_eq!(v.matches("https://evil-example.com").len(), 0);
        assert_eq!(v.matches("https://127.0.0.1").len(), 1);
        assert!(v.matches("android://hash@com.example").is_empty());
    }
    #[test]
    fn mutations_and_roundtrip() {
        let mut v = Vault::default();
        assert_eq!(
            v.upsert("https://EXAMPLE.com:443/a", "u,\"\n", "p\r\nx"),
            Upsert::Added
        );
        assert_eq!(
            v.upsert("https://example.com/b", "u,\"\n", "p\r\nx"),
            Upsert::Unchanged
        );
        assert_eq!(
            v.upsert("https://example.com", "u,\"\n", "new"),
            Upsert::Updated
        );
        v.items[0].note = "not,\"\r\nsatır".into();
        let mut other = Vault::default();
        assert_eq!(other.import_csv(&v.export_csv()).added, 1);
        assert_eq!(other.items[0].note, v.items[0].note);
        assert_eq!(other.items[0].username, v.items[0].username);
        let id = v.items[0].id;
        assert!(v.update(id, "İPEK", "p"));
        assert!(!v.update(id, "u", ""));
        v.touch(id);
        assert!(v.get(id).unwrap().last_used.ends_with('Z'));
        assert_eq!(v.list("ıpek").len(), 1);
        v.set_never("https://example.com/a");
        assert!(v.never("http://example.com/b"));
        assert!(!v.never("https://a.example.com"));
        assert!(v.remove(id));
        assert!(!v.remove(id));
        assert_eq!(v.len(), 0);
        assert_eq!(v.upsert("file:///x", "u", "p"), Upsert::Unchanged);
    }
    #[test]
    fn encrypted_save_load() {
        let dir = std::env::temp_dir().join(format!(
            "sqywein-passwords-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let previous = std::env::var_os("SQYWEIN_DATA");
        std::env::set_var("SQYWEIN_DATA", &dir);
        struct Cleanup(PathBuf, Option<std::ffi::OsString>);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                if let Some(value) = &self.1 {
                    std::env::set_var("SQYWEIN_DATA", value);
                } else {
                    std::env::remove_var("SQYWEIN_DATA");
                }
                let _ = fs::remove_dir_all(&self.0);
            }
        }
        let _cleanup = Cleanup(dir.clone(), previous);
        let mut v = Vault::load("test");
        assert_eq!(v.len(), 0);
        v.upsert("https://example.com", "user", "plain-secret-marker");
        v.set_never("https://blocked.com");
        v.save("test").unwrap();
        assert!(!v.is_dirty());
        assert!(!fs::read(path("test"))
            .unwrap()
            .windows(19)
            .any(|w| w == b"plain-secret-marker"));
        let mut loaded = Vault::load("test");
        assert_eq!(loaded.items[0].password, "plain-secret-marker");
        assert!(loaded.never("http://blocked.com"));
        loaded.update(1, "user", "second");
        loaded.save("test").unwrap();
        assert_eq!(Vault::load("test").get(1).unwrap().password, "second");
        loaded.upsert("https://two.com", "u", "p");
        assert_eq!(loaded.items[1].id, 2);
        fs::write(path("test"), b"broken").unwrap();
        assert_eq!(Vault::load("test").len(), 0);
    }
}
