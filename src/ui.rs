//! İç sayfalar (sqywein://history vb.). Dosyalar exe'ye gömülüdür ve
//! https://sqywein.ui/ adresinden bellekten sunulur; diske hiçbir şey yazılmaz.

use crate::url::UI_ORIGIN;

macro_rules! files {
    ($($name:literal => $mime:literal),* $(,)?) => {
        const FILES: &[(&str, &[u8], &str)] = &[
            $(($name, include_bytes!(concat!("../ui/", $name)), $mime)),*
        ];
    };
}

files! {
    "overlay.html" => "text/html",
    "app.css" => "text/css",
    "app.js" => "text/javascript",
    "history.html" => "text/html",
    "downloads.html" => "text/html",
    "bookmarks.html" => "text/html",
    "settings.html" => "text/html",
    "extensions.html" => "text/html",
    "passwords.html" => "text/html",
}

/// İstenen adresin içeriği ve türü; bilinmeyen dosya için `None`.
pub fn file(uri: &str) -> Option<(&'static [u8], &'static str)> {
    let rest = uri.strip_prefix(UI_ORIGIN)?;
    let name = rest.split(['?', '#']).next().unwrap_or(rest).trim_start_matches('/');
    let name = if name.is_empty() { "history.html" } else { name };
    FILES.iter().find(|(n, ..)| *n == name).map(|(_, body, mime)| (*body, *mime))
}

#[cfg(test)]
mod tests {
    #[test]
    fn dosyalar_bulunur() {
        assert!(super::file("https://sqywein.ui/settings.html?x=1").is_some());
        assert!(super::file("https://sqywein.ui/app.css").is_some());
        assert!(super::file("https://sqywein.ui/../secret").is_none());
        assert!(super::file("https://example.com/app.css").is_none());
    }
}
