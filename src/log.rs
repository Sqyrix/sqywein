//! Hata ayıklama kaydı: SQYWEIN_LOG bir dosya yolu gösteriyorsa gezinmeler,
//! indirmeler ve engellenen istekler oraya yazılır. Normal kullanımda kapalıdır;
//! hiçbir şey kaydedilmez ve bellek harcanmaz.

use std::{io::Write, sync::OnceLock};

fn path() -> Option<&'static str> {
    static PATH: OnceLock<Option<String>> = OnceLock::new();
    PATH.get_or_init(|| std::env::var("SQYWEIN_LOG").ok().filter(|p| !p.is_empty())).as_deref()
}

pub fn enabled() -> bool {
    path().is_some()
}

pub fn line(text: &str) {
    let Some(p) = path() else { return };
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(p) {
        let _ = writeln!(f, "{} {}", crate::history::now_iso(), redact(text));
    }
}

fn redact(text: &str) -> String {
    text.split_whitespace().map(|word| {
        if let Some(at)=word.find("https://").or_else(||word.find("http://")) {
            format!("{}{}", &word[..at],crate::passwords::origin(&word[at..]).unwrap_or_else(||"[adres]".into()))
        } else { word.to_owned() }
    }).collect::<Vec<_>>().join(" ")
}

#[cfg(test)] mod tests {
    #[test] fn debug_logs_do_not_keep_url_credentials_or_tokens() {
        let log=super::redact("NAV https://user:pass@example.com/callback/secret?code=token#access_token=private");
        for secret in ["user", "pass", "secret", "token", "private"] { assert!(!log.contains(secret)); }
        assert!(log.contains("example.com"));
    }
}
