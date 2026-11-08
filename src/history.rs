#![allow(dead_code)]
//! Geçmiş ve adres önerileri; C# ile aynı dosya ve puanlama kullanılır.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::{
    fs, io,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

pub const MAX_ENTRIES: usize = 5000;
const SECOND: i128 = 1_000_000_000;
const DAY: i128 = 86_400 * SECOND;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "PascalCase")]
pub struct HistoryEntry {
    pub url: String,
    pub title: String,
    pub last_visit: String,
    pub visits: i32,
    /// Tanımadığımız alanlar yazarken geri konur.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Default for HistoryEntry {
    fn default() -> Self {
        Self {
            url: String::new(), title: String::new(),
            last_visit: "0001-01-01T00:00:00".into(), visits: 0,
            extra: Map::new(),
        }
    }
}

impl HistoryEntry {
    pub fn display_url(&self) -> String { shorten(&self.url) }

    /// Windows'ta yerel saat; diğer sistemlerde UTC gösterilir.
    pub fn when(&self) -> String {
        let date = local_date(parse_iso(&self.last_visit).unwrap_or(MIN_DATE));
        let today = local_date(now());
        if date[..3] == today[..3] {
            format!("{:02}:{:02}", date[3], date[4])
        } else {
            format!("{:02}.{:02}.{:04} {:02}:{:02}", date[2], date[1], date[0], date[3], date[4])
        }
    }
}

/// Favori listesi henüz state modülünde tür olarak bulunmuyor.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "PascalCase")]
pub struct Favorite {
    pub title: String,
    pub url: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Suggestion {
    pub title: String,
    pub url: String,
}

impl Suggestion {
    pub fn display_url(&self) -> String { shorten(&self.url) }
}

#[derive(Default)]
pub struct History {
    // Eşit puanlarda yükleme sırası korunur; adres karşılaştırması birebirdir.
    entries: Vec<HistoryEntry>,
    dirty: bool,
}

impl History {
    pub fn new(entries: impl IntoIterator<Item = HistoryEntry>) -> Self {
        let mut history = Self::default();
        for entry in entries {
            if let Some(old) = history.entries.iter_mut().find(|e| e.url == entry.url) {
                *old = entry;
            } else {
                history.entries.push(entry);
            }
        }
        history
    }

    pub fn load() -> Self { Self::load_profile(crate::state::DEFAULT_PROFILE) }
    pub fn load_profile(profile: &str) -> Self { Self::load_from(&Self::profile_path(profile)) }
    fn profile_path(profile: &str) -> std::path::PathBuf {
        crate::state::root().join(if profile == crate::state::DEFAULT_PROFILE { "history.json".into() } else { format!("history-{profile}.json") })
    }
    pub fn save_profile(&mut self, profile: &str) -> io::Result<()> { self.save_to(&Self::profile_path(profile)) }

    fn load_from(path: &Path) -> Self {
        let entries: Vec<HistoryEntry> = fs::read(path).ok()
            .and_then(|bytes| serde_json::from_slice(
                bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(&bytes)
            ).ok()).unwrap_or_default();
        Self::new(entries)
    }

    pub fn record(&mut self, url: &str, title: &str) {
        if !starts_ignore_case(url, "http://") && !starts_ignore_case(url, "https://") {
            return;
        }
        let index = match self.entries.iter().position(|e| e.url == url) {
            Some(index) => index,
            None => {
                self.entries.push(HistoryEntry { url: url.into(), ..HistoryEntry::default() });
                self.entries.len() - 1
            }
        };
        let entry = &mut self.entries[index];
        entry.last_visit = format_iso(now());
        entry.visits = entry.visits.wrapping_add(1);
        if !title.is_empty() { entry.title = title.into(); }
        self.dirty = true;
    }

    pub fn update_title(&mut self, url: &str, title: &str) {
        if title.is_empty() { return; }
        if let Some(entry) = self.entries.iter_mut().find(|e| e.url == url) {
            if entry.title != title {
                entry.title = title.into();
                self.dirty = true;
            }
        }
    }

    pub fn remove(&mut self, url: &str) {
        if let Some(index) = self.entries.iter().position(|e| e.url == url) {
            self.entries.remove(index);
            self.dirty = true;
        }
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.dirty = true;
    }

    pub fn is_dirty(&self) -> bool { self.dirty }

    pub fn recent(&self, filter: &str, limit: usize) -> Vec<&HistoryEntry> {
        let tokens = tokens(filter);
        let mut entries: Vec<_> = self.entries.iter()
            .filter(|e| matches_all(&e.url, &e.title, &tokens)).collect();
        entries.sort_by_key(|e| std::cmp::Reverse(visit_time(e)));
        entries.truncate(limit);
        entries
    }

    pub fn suggest(&self, query: &str, favorites: &[Favorite], skip_prefix: &str, limit: usize) -> Vec<Suggestion> {
        self.suggest_at(query, favorites, skip_prefix, limit, now())
    }

    fn suggest_at(&self, query: &str, favorites: &[Favorite], skip_prefix: &str, limit: usize, now: i128) -> Vec<Suggestion> {
        let tokens = tokens(query);
        if tokens.is_empty() { return Vec::new(); }
        let mut scored: Vec<(String, i64, String)> = Vec::new();
        for f in favorites {
            if !matches_all(&f.url, &f.title, &tokens) { continue; }
            let score = 1000 + text_score(&f.url, &f.title, &tokens);
            if let Some(old) = scored.iter_mut().find(|p| p.0 == f.url) {
                old.1 = score;
                old.2 = f.title.clone();
            } else {
                scored.push((f.url.clone(), score, f.title.clone()));
            }
        }
        for e in &self.entries {
            // Arama sonuçları öneriden çıkarılır; geçmişte görünmeye devam eder.
            if e.url.starts_with(skip_prefix) || !matches_all(&e.url, &e.title, &tokens) { continue; }
            let age = now - visit_time(e);
            let score = text_score(&e.url, &e.title, &tokens)
                + i64::from(e.visits.min(20)) * 3
                + if age < DAY { 30 } else if age < 7 * DAY { 15 } else { 0 };
            if let Some(old) = scored.iter_mut().find(|p| p.0 == e.url) {
                old.1 += score;
                if !e.title.is_empty() { old.2 = e.title.clone(); }
            } else {
                scored.push((e.url.clone(), score, e.title.clone()));
            }
        }
        scored.sort_by_key(|p| std::cmp::Reverse(p.1));
        scored.into_iter().take(limit).map(|(url, _, title)| Suggestion {
            title: if title.is_empty() { shorten(&url) } else { title }, url,
        }).collect()
    }

    pub fn save(&mut self) -> io::Result<()> {
        self.save_to(&crate::state::root().join("history.json"))
    }

    fn save_to(&mut self, path: &Path) -> io::Result<()> {
        // C# gibi sınır yalnızca kaydederken uygulanır.
        if self.entries.len() > MAX_ENTRIES {
            let mut order: Vec<_> = (0..self.entries.len()).collect();
            order.sort_by_key(|&i| std::cmp::Reverse(visit_time(&self.entries[i])));
            let mut keep = vec![false; self.entries.len()];
            for &index in order.iter().take(MAX_ENTRIES) { keep[index] = true; }
            let mut index = 0;
            self.entries.retain(|_| { let retain = keep[index]; index += 1; retain });
        }
        if let Some(dir) = path.parent() { fs::create_dir_all(dir)?; }
        let temp = path.with_extension("json.tmp");
        fs::write(&temp, serde_json::to_vec(&self.entries)?)?;
        fs::rename(temp, path)?;
        self.dirty = false;
        Ok(())
    }

    pub fn shorten(url: &str) -> String { shorten(url) }
}

pub fn load() -> History { History::load() }

pub fn shorten(url: &str) -> String {
    let mut text = url;
    for prefix in ["https://www.", "http://www.", "https://", "http://"] {
        if starts_ignore_case(text, prefix) { text = &text[prefix.len()..]; break; }
    }
    text.strip_suffix('/').unwrap_or(text).to_owned()
}

fn starts_ignore_case(text: &str, prefix: &str) -> bool {
    text.get(..prefix.len()).is_some_and(|s| s.eq_ignore_ascii_case(prefix))
}

fn fold(text: &str) -> String {
    // Yalnızca i/ı ayrımı kaldırılır; ş/ğ/ü/ö/ç korunur.
    text.chars().map(|c| match c { 'İ' | 'I' | 'ı' => 'i', _ => c })
        .flat_map(char::to_lowercase).collect()
}

fn tokens(text: &str) -> Vec<String> {
    fold(text).split(' ').filter(|s| !s.is_empty()).map(str::to_owned).collect()
}

fn matches_all(url: &str, title: &str, tokens: &[String]) -> bool {
    let url = fold(url);
    let title = fold(title);
    tokens.iter().all(|k| url.contains(k.as_str()) || title.contains(k.as_str()))
}

fn text_score(url: &str, title: &str, tokens: &[String]) -> i64 {
    let url = fold(&shorten(url));
    let title = fold(title);
    tokens.iter().map(|k| {
        if url.starts_with(k.as_str()) { 200 }
        else if title.starts_with(k.as_str()) || title.contains(&format!(" {k}")) { 60 }
        else { 20 }
    }).sum()
}

fn now() -> i128 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_nanos() as i128,
        Err(e) => -(e.duration().as_nanos() as i128),
    }
}

const MIN_DATE: i128 = -62_135_596_800 * SECOND;

fn visit_time(entry: &HistoryEntry) -> i128 {
    parse_iso(&entry.last_visit).unwrap_or(MIN_DATE)
}

// Gregoryen takvim, Unix başlangıcına göre gün sayısı.
fn days_from_date(year: i64, month: i64, day: i64) -> i64 {
    let year = year - if month <= 2 { 1 } else { 0 };
    let era = year.div_euclid(400);
    let y = year - era * 400;
    let m = month + if month > 2 { -3 } else { 9 };
    era * 146097 + y * 365 + y / 4 - y / 100 + (153 * m + 2) / 5 + day - 1 - 719468
}

fn utc_date(time: i128) -> [i64; 6] {
    let seconds = time.div_euclid(SECOND);
    let days = seconds.div_euclid(86400) as i64;
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    let year = year + if month <= 2 { 1 } else { 0 };
    let within = seconds.rem_euclid(86400) as i64;
    [year, month, day, within / 3600, within / 60 % 60, within % 60]
}

fn format_iso(time: i128) -> String {
    let [y, m, d, h, min, s] = utc_date(time);
    // DateTime gibi 100 ns hassasiyetinde UTC yazılır.
    format!("{y:04}-{m:02}-{d:02}T{h:02}:{min:02}:{s:02}.{:07}Z", time.rem_euclid(SECOND) / 100)
}

fn parse_iso(text: &str) -> Option<i128> {
    fn number(bytes: &[u8]) -> Option<i64> {
        bytes.iter().try_fold(0, |n, b| {
            if b.is_ascii_digit() { Some(n * 10 + i64::from(*b - b'0')) } else { None }
        })
    }
    let b = text.as_bytes();
    if b.len() < 19 || b[4] != b'-' || b[7] != b'-' || b[10] != b'T'
        || b[13] != b':' || b[16] != b':' { return None; }
    let y = number(&b[..4])?;
    let m = number(&b[5..7])?;
    let d = number(&b[8..10])?;
    let h = number(&b[11..13])?;
    let min = number(&b[14..16])?;
    let s = number(&b[17..19])?;
    let leap = y % 4 == 0 && (y % 100 != 0 || y % 400 == 0);
    let max_day = match m { 2 => if leap { 29 } else { 28 }, 4 | 6 | 9 | 11 => 30, 1..=12 => 31, _ => return None };
    if !(1..=9999).contains(&y) || !(1..=max_day).contains(&d) || h > 23 || min > 59 || s > 59 { return None; }
    let mut pos = 19;
    let mut fraction = 0;
    if b.get(pos) == Some(&b'.') {
        pos += 1;
        let start = pos;
        while b.get(pos).is_some_and(|c| c.is_ascii_digit()) { pos += 1; }
        let digits = pos - start;
        if digits == 0 || digits > 7 { return None; }
        fraction = i128::from(number(&b[start..pos])?) * 10_i128.pow(9 - digits as u32);
    }
    let offset = match &b[pos..] {
        [] | [b'Z'] => 0,
        [sign @ (b'+' | b'-'), a, c, b':', e, f] => {
            let hours = number(&[*a, *c])?;
            let minutes = number(&[*e, *f])?;
            if hours > 14 || minutes > 59 || (hours == 14 && minutes != 0) { return None; }
            (hours * 3600 + minutes * 60) * if *sign == b'+' { 1 } else { -1 }
        }
        _ => return None,
    };
    Some(i128::from(days_from_date(y, m, d) * 86400 + h * 3600 + min * 60 + s - offset) * SECOND + fraction)
}

#[cfg(not(windows))]
fn local_date(time: i128) -> [i64; 6] { utc_date(time) }

#[cfg(windows)]
fn local_date(time: i128) -> [i64; 6] {
    #[repr(C)]
    #[derive(Default)]
    struct WinTime { year: u16, month: u16, weekday: u16, day: u16, hour: u16, minute: u16, second: u16, millis: u16 }
    #[link(name = "kernel32")]
    extern "system" {
        fn SystemTimeToTzSpecificLocalTimeEx(zone: *const std::ffi::c_void, utc: *const WinTime, local: *mut WinTime) -> i32;
    }
    let date = utc_date(time);
    let utc = WinTime { year: date[0] as u16, month: date[1] as u16, day: date[2] as u16,
        hour: date[3] as u16, minute: date[4] as u16, second: date[5] as u16, ..WinTime::default() };
    let mut local = WinTime::default();
    // Boş bölge işaretçisi Windows'un geçerli saat dilimini seçer.
    let ok = unsafe { SystemTimeToTzSpecificLocalTimeEx(std::ptr::null(), &utc, &mut local) };
    if ok == 0 { return date; }
    [local.year.into(), local.month.into(), local.day.into(), local.hour.into(), local.minute.into(), local.second.into()]
}

/// Şu anki zaman, ISO-8601 (UTC).
pub fn now_iso() -> String {
    format_iso(now())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(url: &str, title: &str, time: i128, visits: i32) -> HistoryEntry {
        HistoryEntry { url: url.into(), title: title.into(), last_visit: format_iso(time), visits, ..HistoryEntry::default() }
    }

    struct TempDir(std::path::PathBuf);
    impl TempDir {
        fn new() -> Self {
            static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
            let path = std::env::temp_dir().join(format!("sqywein-history-{}-{}-{}", std::process::id(), now(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
        fn file(&self) -> std::path::PathBuf { self.0.join("history.json") }
    }
    impl Drop for TempDir {
        fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); }
    }

    #[test]
    fn csharp_json_ve_bom() {
        let json = r#"[{"Url":"https://örnek.com/","Title":"İstanbul","LastVisit":"2026-11-06T12:34:56.789Z","Visits":4,"Future":{"Enabled":true}}]"#;
        let expected: Value = serde_json::from_str(json).unwrap();
        let dir = TempDir::new();
        fs::write(dir.file(), [b"\xEF\xBB\xBF".as_slice(), json.as_bytes()].concat()).unwrap();
        let mut history = History::load_from(&dir.file());
        assert!(!history.is_dirty());
        assert_eq!(history.entries.len(), 1);
        assert_eq!(serde_json::to_value(&history.entries).unwrap(), expected);
        history.save_to(&dir.file()).unwrap();
        // Var olan hedefin üzerine ikinci atomik kayıt.
        history.save_to(&dir.file()).unwrap();
        assert_eq!(serde_json::from_slice::<Value>(&fs::read(dir.file()).unwrap()).unwrap(), expected);
        assert!(!dir.file().with_extension("json.tmp").exists());
        fs::write(dir.file(), b"broken").unwrap();
        assert!(History::load_from(&dir.file()).entries.is_empty());
    }

    #[test]
    fn turkce_katlama() {
        assert_eq!(fold("İIıi Şş Ğğ Üü Öö Çç"), "iiii şş ğğ üü öö çç");
        assert!(matches_all("https://ornek.com/", "İstanbul IŞIK GÜÇ", &tokens("istanbul ışık güç")));
        assert!(!matches_all("", "şğüöç", &tokens("sguoc")));
        assert_eq!(tokens("  İSTANBUL  IŞIK "), vec!["istanbul", "işik"]);
        assert_eq!(tokens("a\tb"), vec!["a\tb"]);
    }

    #[test]
    fn puanlar_ve_oneri_sirasi() {
        let time = parse_iso("2026-11-06T12:00:00Z").unwrap();
        assert_eq!(text_score("https://www.rust.org/", "", &tokens("rust")), 200);
        assert_eq!(text_score("https://x/", "Learn Rust", &tokens("rust")), 60);
        assert_eq!(text_score("https://x/rust", "", &tokens("rust")), 20);
        let history = History::new(vec![
            entry("https://rust.org/", "", time - 8 * DAY, 0),
            entry("https://new/", "Rust", time, 20),
            entry("https://week/", "Rust", time - DAY, 20),
            entry("https://old/", "Rust", time - 7 * DAY, 20),
            entry("https://search/?q=rust", "", time, 100),
        ]);
        let favorites = vec![Favorite { url: "https://favorite/".into(), title: "Rust".into() }];
        let result = history.suggest_at("rust", &favorites, "https://search/", 10, time);
        assert_eq!(result.iter().map(|s| s.url.as_str()).collect::<Vec<_>>(),
            vec!["https://favorite/", "https://rust.org/", "https://new/", "https://week/", "https://old/"]);
        assert_eq!(result[1].title, "rust.org");
        assert!(history.suggest_at("  ", &favorites, "", 10, time).is_empty());
        assert_eq!(history.suggest_at("rust", &favorites, "", 10, time).len(), 1);
        assert!(history.suggest_at("rust", &favorites, "!", 0, time).is_empty());
        assert_eq!(history.recent("rust", 10).len(), 5);
    }

    #[test]
    fn favori_ve_gecmis_birlesir() {
        let history = History::new(vec![entry("https://a/", "Rust history", 0, 20), entry("https://b/rust", "", 0, 0)]);
        let favorites = vec![
            Favorite { url: "https://a/".into(), title: "Rust favorite".into() },
            Favorite { url: "https://b/rust".into(), title: "Rust preserved".into() },
            Favorite { url: "https://c/".into(), title: "Rust".into() },
        ];
        let result = history.suggest_at("rust", &favorites, "!", 10, 0);
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].title, "Rust history");
        assert_eq!(result.iter().find(|s| s.url == "https://b/rust").unwrap().title, "Rust preserved");
    }

    #[test]
    fn ziyaret_tavani_gelecek_tarih_ve_esit_puanlar() {
        let history = History::new(vec![
            entry("https://a/", "Rust", 0, 20),
            entry("https://b/", "Rust", 0, 200),
            entry("https://future/", "Rust", DAY, 20),
        ]);
        let result = history.suggest_at("rust", &[], "!", 10, 0);
        assert_eq!(result.iter().map(|s| s.url.as_str()).collect::<Vec<_>>(),
            vec!["https://a/", "https://b/", "https://future/"]);
        assert!(matches_all("https://rust.org/", "Türkçe rehber", &tokens("rust türkçe")));
        assert!(!matches_all("https://rust.org/", "Türkçe rehber", &tokens("rust yok")));
    }

    #[test]
    fn yinelenen_adres_son_degeri_alir() {
        let history = History::new(vec![entry("https://a/", "First", 0, 1), entry("https://a/", "Last", DAY, 2)]);
        assert_eq!(history.entries.len(), 1);
        assert_eq!(history.entries[0].title, "Last");
        assert!(!history.is_dirty());
    }

    #[test]
    fn ziyaretler_ve_kirli_bayragi() {
        let mut history = History::default();
        history.record("about:blank", "x");
        history.remove("missing");
        history.update_title("missing", "x");
        assert!(!history.is_dirty());
        history.record("HTTPS://Example/", "First");
        history.record("HTTPS://Example/", "");
        assert!(history.is_dirty());
        assert_eq!(history.entries[0].title, "First");
        assert_eq!(history.entries[0].visits, 2);
        assert!(parse_iso(&history.entries[0].last_visit).is_some());
        history.dirty = false;
        history.update_title("HTTPS://Example/", "First");
        history.update_title("HTTPS://Example/", "");
        assert!(!history.is_dirty());
        history.update_title("HTTPS://Example/", "Second");
        assert!(history.is_dirty());
        history.record("https://example/", "Other");
        assert_eq!(history.entries.len(), 2);
        history.remove("HTTPS://Example/");
        assert_eq!(history.entries.len(), 1);
        history.clear();
        assert!(history.entries.is_empty());
        history.dirty = false;
        history.clear();
        assert!(history.is_dirty());
    }

    #[test]
    fn sinir_kaydederken_uygulanir() {
        let mut history = History::new((0..MAX_ENTRIES + 3).map(|i|
            entry(&format!("https://{i}/"), "", i as i128 * SECOND, 1)));
        assert_eq!(history.entries.len(), MAX_ENTRIES + 3);
        let dir = TempDir::new();
        history.dirty = true;
        history.save_to(&dir.file()).unwrap();
        assert!(!history.is_dirty());
        assert_eq!(history.entries.len(), MAX_ENTRIES);
        assert!(!history.entries.iter().any(|e| e.url == "https://2/"));
        assert!(history.entries.iter().any(|e| e.url == "https://3/"));
        assert_eq!(History::load_from(&dir.file()).entries.len(), MAX_ENTRIES);
    }

    #[test]
    fn kayit_hatasi_kirli_bayragini_silmez() {
        let dir = TempDir::new();
        fs::create_dir(dir.file()).unwrap();
        let mut history = History::default();
        history.record("https://a/", "A");
        assert!(history.save_to(&dir.file()).is_err());
        assert!(history.is_dirty());
    }

    #[test]
    fn tarihler_ve_siralama() {
        assert_eq!(parse_iso("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_iso("0001-01-01T00:00:00"), Some(MIN_DATE));
        for text in ["0001-01-01T00:00:00.0000000Z", "1969-12-31T23:59:59.9999999Z",
            "2000-02-29T12:34:56.1234567Z", "2026-11-06T12:34:56.7890000Z", "9999-12-31T23:59:59.9999999Z"] {
            assert_eq!(format_iso(parse_iso(text).unwrap()), text);
        }
        assert!(parse_iso("1900-02-29T00:00:00Z").is_none());
        assert_eq!(parse_iso("1970-01-01T03:00:00+03:00"), Some(0));
        assert_eq!(parse_iso("1969-12-31T23:00:00-01:00"), Some(0));
        let history = History::new(vec![
            HistoryEntry { url: "a".into(), last_visit: "2026-11-06T12:00:00Z".into(), ..HistoryEntry::default() },
            HistoryEntry { url: "b".into(), last_visit: "2026-11-06T12:00:00.001Z".into(), ..HistoryEntry::default() },
        ]);
        assert_eq!(history.recent("", 2)[0].url, "b");
        assert_eq!(shorten("HTTPS://WWW.Example/a//"), "Example/a/");
    }
}
