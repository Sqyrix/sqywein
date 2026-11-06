//! Adres çubuğuna yazılanı URL'ye ya da aramaya çevirir.

pub const SEARCH_URL: &str = "https://www.google.com/search?q=";

/// İç sayfaların gerçek adresi; adres çubuğunda `sqywein://ad` olarak görünür.
pub const UI_ORIGIN: &str = "https://sqywein.ui/";
pub const UI_PAGES: [&str; 6] = ["history", "downloads", "bookmarks", "settings", "extensions", "passwords"];

/// `sqywein://history` → `https://sqywein.ui/history.html`
pub fn internal(page: &str) -> String {
    format!("{UI_ORIGIN}{page}.html")
}

/// Adres çubuğunda gösterilecek hali: iç sayfalar `sqywein://ad` olur.
pub fn display(url: &str) -> String {
    match url.strip_prefix(UI_ORIGIN) {
        Some(rest) => {
            let page = rest.split(['?', '#']).next().unwrap_or(rest).trim_end_matches(".html");
            format!("sqywein://{page}")
        }
        None => url.to_string(),
    }
}

pub fn is_internal(url: &str) -> bool {
    url.starts_with(UI_ORIGIN)
}

/// Boş metin için `None`; adres gibi görünüyorsa adres, değilse arama URL'si döner.
pub fn to_url(text: &str) -> Option<String> {
    to_url_with(text, SEARCH_URL)
}

pub fn to_url_with(text: &str, search: &str) -> Option<String> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    if let Some(rest) = text.strip_prefix("sqywein://") {
        // "sqywein://passwords#import" gibi bir bölüm adresi de korunur.
        let (page, hash) = rest.split_once('#').map_or((rest, ""), |(p, h)| (p, h));
        let page = page.trim_end_matches('/');
        if UI_PAGES.contains(&page) {
            let hash = if hash.is_empty() { String::new() } else { format!("#{hash}") };
            return Some(format!("{}{hash}", internal(page)));
        }
    }
    if let Some((scheme, rest)) = text.split_once("://") {
        if !rest.is_empty() && is_scheme(scheme) && !text.contains(char::is_whitespace) {
            return Some(text.to_string());
        }
    }
    let lower = text.to_ascii_lowercase();
    if lower.starts_with("about:") || lower.starts_with("edge://") {
        return Some(text.to_string());
    }
    let is_local = lower.starts_with("localhost");
    let looks_like_host = !text.contains(char::is_whitespace)
        && (is_local || (text.contains('.') && !text.starts_with('.') && !text.ends_with('.')));
    if looks_like_host {
        return Some(format!("{}{}", if is_local { "http://" } else { "https://" }, text));
    }
    Some(format!("{search}{}", encode(text)))
}

fn is_scheme(s: &str) -> bool {
    let mut chars = s.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

/// Sorgu parametresi için yüzde kodlaması (UTF-8).
pub fn encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 3);
    for b in text.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Sekme başlığı yokken gösterilecek kısa ad: `https://www.ornek.com/a` → `ornek.com`.
pub fn host(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let host = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    host.strip_prefix("www.").unwrap_or(host)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adresler() {
        assert_eq!(to_url("github.com").unwrap(), "https://github.com");
        assert_eq!(to_url("  https://a.b/c ").unwrap(), "https://a.b/c");
        assert_eq!(to_url("localhost:3000").unwrap(), "http://localhost:3000");
        assert_eq!(to_url("about:blank").unwrap(), "about:blank");
        assert!(to_url("   ").is_none());
    }

    #[test]
    fn aramalar() {
        assert_eq!(to_url("rust dili").unwrap(), format!("{SEARCH_URL}rust+dili"));
        assert_eq!(to_url("çay").unwrap(), format!("{SEARCH_URL}%C3%A7ay"));
        assert_eq!(to_url("nokta.").unwrap(), format!("{SEARCH_URL}nokta."));
    }

    #[test]
    fn ic_sayfalar() {
        assert_eq!(to_url("sqywein://history").unwrap(), "https://sqywein.ui/history.html");
        assert_eq!(display("https://sqywein.ui/settings.html"), "sqywein://settings");
        assert_eq!(to_url_with("kedi", "https://duckduckgo.com/?q=").unwrap(), "https://duckduckgo.com/?q=kedi");
    }

    #[test]
    fn kisa_ad() {
        assert_eq!(host("https://www.ornek.com/a?b"), "ornek.com");
        assert_eq!(host("http://localhost:3000"), "localhost:3000");
    }
}
