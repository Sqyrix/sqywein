//! Başlangıç sayfası kartları: hava durumu ve döviz kurları.
//!
//! Son veriler diske kaydedilir: sayfa açılır açılmaz görünür, yenisi arkada
//! gelir. Konum 6 saat hatırlanır; hava durumu ve kurlar ayrı iş parçacıklarında
//! aynı anda istenir. Kullanılan servisler anahtarsız ve ücretsizdir:
//! - Hava durumu: Open-Meteo (konum yaklaşık 1 km'ye yuvarlanır).
//! - Şehir adı: elle yazılan şehir Open-Meteo'da aranır; yoksa Windows konumu
//!   BigDataCloud ile yer adına çevrilir.
//! - Döviz: Frankfurter (Avrupa Merkez Bankası'nın günlük kurları).

use crate::{engine, net, url};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Weather {
    pub place: String,
    pub temp: f32,
    pub code: i32,
    pub day: bool,
    /// (gün adı, hava kodu, en yüksek, en düşük)
    pub days: Vec<(String, i32, f32, f32)>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Rate {
    pub code: String,
    /// 1 birimin TL karşılığı.
    pub value: f64,
    /// Önceki iş gününe göre % değişim.
    pub change: f64,
    /// Son günlerin kurları (eskiden yeniye), küçük grafik için.
    pub series: Vec<f64>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Rates {
    pub date: String,
    pub items: Vec<Rate>,
}

/// Bulunan konum: (enlem, boylam, yer adı) ve hangi şehir ayarıyla bulunduğu.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Location {
    pub lat: f64,
    pub lon: f64,
    pub place: String,
    pub city: String,
    pub at: u64,
}

/// Diskte saklanan son durum.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Cache {
    pub weather: Option<(u64, Weather)>,
    pub rates: Option<(u64, Rates)>,
    pub location: Option<Location>,
}

#[derive(Clone, Debug)]
pub enum WeatherState {
    Ready(Weather),
    /// Konum bulunamadı: kullanıcıdan şehir istenir.
    NoLocation,
    Failed,
}

pub fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// WMO hava kodu → (Türkçe durum, emoji).
pub fn describe(code: i32, day: bool) -> (&'static str, &'static str) {
    match code {
        0 => ("Açık", if day { "☀️" } else { "🌙" }),
        1 => ("Az bulutlu", if day { "🌤️" } else { "🌙" }),
        2 => ("Parçalı bulutlu", "⛅"),
        3 => ("Kapalı", "☁️"),
        45 | 48 => ("Sisli", "🌫️"),
        51..=57 => ("Çisenti", "🌦️"),
        61..=67 => ("Yağmurlu", "🌧️"),
        71..=77 => ("Karlı", "🌨️"),
        80..=82 => ("Sağanak", "🌧️"),
        85 | 86 => ("Kar sağanağı", "🌨️"),
        95..=99 => ("Gök gürültülü", "⛈️"),
        _ => ("—", "🌡️"),
    }
}

/// Kartın gökyüzü: hava koduna ve gece/gündüze göre gradyan (üst, alt).
pub fn sky(code: i32, day: bool) -> (u32, u32) {
    match (code, day) {
        (0 | 1, true) => (0x5AB2FF, 0x1F6FE0),
        (0 | 1, false) => (0x24345C, 0x0B1430),
        (2, true) => (0x6FA4D8, 0x3F6E9E),
        (2 | 3, false) => (0x343D52, 0x161B27),
        (3, true) => (0x8A9BB0, 0x56677D),
        (45 | 48, _) => (0x9AA4AE, 0x66717C),
        (51..=67 | 80..=82, true) => (0x5E7D9B, 0x2E4762),
        (51..=67 | 80..=82, false) => (0x2C3A4D, 0x121A26),
        (71..=77 | 85 | 86, _) => (0x9DB4CC, 0x627C99),
        (95..=99, _) => (0x4A4C6B, 0x1F2036),
        _ => (0x5AB2FF, 0x1F6FE0),
    }
}

fn json(u: &str) -> Option<Value> {
    net::fetch(u, 256 * 1024).and_then(|b| serde_json::from_slice(&b).ok())
}

/// Windows konum servisi (kapalıysa ya da izin yoksa `None`).
fn windows_location() -> Option<(f64, f64)> {
    use windows::Devices::Geolocation::{Geolocator, PositionAccuracy};
    let locator = Geolocator::new().ok()?;
    let _ = locator.SetDesiredAccuracy(PositionAccuracy::Default);
    let pos = locator.GetGeopositionAsync().ok()?.join().ok()?;
    let p = pos.Coordinate().ok()?.Point().ok()?.Position().ok()?;
    Some((p.Latitude, p.Longitude))
}

/// Şehir adından koordinat ve görünen ad.
fn geocode(city: &str) -> Option<(f64, f64, String)> {
    let v = json(&format!("https://geocoding-api.open-meteo.com/v1/search?name={}&count=1&language=tr", url::encode(city)))?;
    let r = v["results"].get(0)?;
    Some((r["latitude"].as_f64()?, r["longitude"].as_f64()?, r["name"].as_str()?.to_string()))
}

fn place_name(lat: f64, lon: f64) -> String {
    json(&format!("https://api.bigdatacloud.net/data/reverse-geocode-client?latitude={lat:.2}&longitude={lon:.2}&localityLanguage=tr"))
        .and_then(|v| ["city", "locality", "principalSubdivision"].iter().find_map(|k| v[*k].as_str().filter(|s| !s.is_empty()).map(str::to_string)))
        .unwrap_or_default()
}

const WEEKDAYS: [&str; 7] = ["Pzt", "Sal", "Çar", "Per", "Cum", "Cmt", "Paz"];

/// "2026-11-09" → haftanın günü.
fn weekday(date: &str) -> Option<&'static str> {
    let mut it = date.split('-').map(|p| p.parse::<i64>().ok());
    let (mut y, mut m, d) = (it.next()??, it.next()??, it.next()??);
    if m < 3 { m += 12; y -= 1; }
    let h = (d + 13 * (m + 1) / 5 + y + y / 4 - y / 100 + y / 400) % 7; // 0 = Cumartesi
    Some(WEEKDAYS[((h + 5) % 7) as usize])
}

/// Unix saniyesi → "YYYY-AA-GG" (UTC).
fn date_of(secs: i64) -> String {
    let z = secs.div_euclid(86400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

pub fn parse_weather(v: &Value, place: String) -> Option<Weather> {
    let cur = &v["current"];
    let daily = &v["daily"];
    let times = daily["time"].as_array()?;
    let days = times.iter().enumerate().skip(1).take(5).filter_map(|(i, t)| {
        Some((weekday(t.as_str()?)?.to_string(), daily["weather_code"][i].as_i64()? as i32, daily["temperature_2m_max"][i].as_f64()? as f32, daily["temperature_2m_min"][i].as_f64()? as f32))
    }).collect();
    Some(Weather { place, temp: cur["temperature_2m"].as_f64()? as f32, code: cur["weather_code"].as_i64()? as i32, day: cur["is_day"].as_i64() != Some(0), days })
}

pub fn parse_rates(v: &Value) -> Option<Rates> {
    // TRY tabanlı kurlar ters çevrilir (1 USD = x TL); son iki iş günü karşılaştırılır.
    let rates = v["rates"].as_object()?;
    let mut dates: Vec<&String> = rates.keys().collect();
    dates.sort();
    let last = *dates.last()?;
    let value = |date: &str, code: &str| rates.get(date)?.get(code)?.as_f64().filter(|r| *r > 0.0).map(|r| 1.0 / r);
    let items = ["USD", "EUR", "GBP"].iter().filter_map(|code| {
        let series: Vec<f64> = dates.iter().filter_map(|d| value(d, code)).collect();
        let now = *series.last()?;
        let change = series.len().checked_sub(2).map_or(0.0, |i| (now - series[i]) / series[i] * 100.0);
        Some(Rate { code: code.to_string(), value: now, change, series })
    }).collect();
    Some(Rates { date: last.clone(), items })
}

/// Hava durumu: hafızadaki konum tazeyse (6 saat, aynı şehir ayarı) yeniden aranmaz.
pub fn refresh_weather(city: String, known: Option<Location>) {
    std::thread::spawn(move || {
        net::init_thread();
        let _ = unsafe { windows::Win32::System::WinRT::RoInitialize(windows::Win32::System::WinRT::RO_INIT_MULTITHREADED) };
        let city = city.trim().to_string();
        let fresh = known.filter(|l| l.city == city && now().saturating_sub(l.at) < 6 * 3600);
        let location = fresh.or_else(|| {
            let (lat, lon, place) = if city.is_empty() {
                windows_location().map(|(lat, lon)| (lat, lon, place_name(lat, lon)))?
            } else {
                geocode(&city)?
            };
            Some(Location { lat, lon, place, city: city.clone(), at: now() })
        });
        let state = match &location {
            None => WeatherState::NoLocation,
            Some(l) => {
                let u = format!(
                    "https://api.open-meteo.com/v1/forecast?latitude={:.2}&longitude={:.2}&current=temperature_2m,weather_code,is_day&daily=weather_code,temperature_2m_max,temperature_2m_min&timezone=auto&forecast_days=6",
                    l.lat, l.lon
                );
                json(&u).and_then(|v| parse_weather(&v, l.place.clone())).map_or(WeatherState::Failed, WeatherState::Ready)
            }
        };
        engine::push_background(engine::Background::Weather(state, location));
    });
}

pub fn refresh_rates() {
    std::thread::spawn(|| {
        net::init_thread();
        let since = date_of(now() as i64 - 16 * 86400);
        let rates = json(&format!("https://api.frankfurter.dev/v1/{since}..?base=TRY&symbols=USD,EUR,GBP")).and_then(|v| parse_rates(&v));
        engine::push_background(engine::Background::Rates(rates));
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tarih() {
        assert_eq!(date_of(0), "1970-01-01");
        assert_eq!(date_of(1_794_182_400), "2026-11-09");
    }

    #[test]
    fn haftanin_gunu() {
        assert_eq!(weekday("2026-11-09"), Some("Pzt"));
        assert_eq!(weekday("2026-11-15"), Some("Paz"));
        assert_eq!(weekday("2024-02-29"), Some("Per"));
    }

    #[test]
    fn hava_ve_kur_cozumleme() {
        let v: Value = serde_json::from_str(r#"{"current":{"temperature_2m":1.5,"weather_code":0,"is_day":0},
            "daily":{"time":["2026-11-09","2026-11-10"],"weather_code":[0,3],"temperature_2m_max":[7.2,7.3],"temperature_2m_min":[-1.0,0.5]}}"#).unwrap();
        let w = parse_weather(&v, "Malatya".into()).unwrap();
        assert_eq!((w.code, w.day, w.days.len()), (0, false, 1));
        assert_eq!(w.days[0].0, "Sal");
        assert_eq!(describe(3, true).0, "Kapalı");
        assert_ne!(sky(0, true), sky(0, false));
        let r: Value = serde_json::from_str(r#"{"rates":{"2026-11-05":{"USD":0.02,"EUR":0.0175,"GBP":0.015},"2026-11-06":{"USD":0.019939,"EUR":0.017858,"GBP":0.014897}}}"#).unwrap();
        let rates = parse_rates(&r).unwrap();
        assert_eq!(rates.date, "2026-11-06");
        let usd = &rates.items[0];
        assert_eq!(usd.code, "USD");
        assert!((usd.value - 50.153).abs() < 0.01);
        assert!(usd.change > 0.0);
        assert_eq!(usd.series.len(), 2);
    }
}
