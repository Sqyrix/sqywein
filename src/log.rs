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
        let _ = writeln!(f, "{} {text}", crate::history::now_iso());
    }
}
