# Sqywein → Rust geçiş planı

Hedef: **inanılmaz hafif ama tam işlevli**, kendine has görünen bir tarayıcı.
Sıra: **1) motor ve Rust'a tam geçiş → 2) adblock → 3) Sqywein tasarımı.**

## Ölçülen başlangıç noktası (C# / WPF sürümü)

| Senaryo | Kabuk | Motor | Toplam |
|---|---|---|---|
| Boş başlangıç | 47,6 MB | 0 | 47,6 MB |
| Tek site | 32,3 MB | 124,7 MB | 157 MB |
| 4 site uyanık | 36,8 MB | 560,8 MB | 597,6 MB |

Rust sürümünün kabuk hedefi: **boşta < 10 MB**, soğuk açılış **< 150 ms** (pencere görünene kadar).

## Durum (10 Kasım 2026)

- **Faz 1-2 (motor, Rust'a geçiş, eşitlik):** tamam. C# sürümü kaldırıldı.
- **Faz 3 (reklam engelleyici):** tamam. Ağ + kozmetik + YouTube modülü, Kapalı/Temel/İdeal/Tam modları.
- **Faz 4 (tasarım):** Safari esintili düzen, açık/koyu/Sqywein/sistem temaları, komut paleti, sekme ızgarası ve yerel yardımcı paneller mevcut. Dikey sekmeler plan aşamasında.
- **Faz 5 (paketleme):** kurulum, otomatik güncelleme ve GitHub Releases mevcut. Son yerel aday 0.4.5: şifreli oturum/geçmiş, sağlam yedek, site bilgi paneli, UI Automation ve otomatik pil politikası. Ayrıntılar [çekirdek incelemesinde](docs/2026-11-10-core-hardening.md).

## Performans (7 Kasım 2026, Rust sürümü ile C# sürümü)

Toplam = kabuk + motorun tüm süreçleri (özel çalışma kümesi). Açılış = pencerenin görünmesi.

| Senaryo | C# açılış | Rust açılış | C# toplam | Rust toplam | Rust kabuk |
|---|---|---|---|---|---|
| Boş başlangıç | 1132 ms | 290 ms | 35,9 MB | 20,3 MB | 20,3 MB |
| Habertürk (reklamlı site) | 836 ms | 105 ms | 404 MB | 289 MB | 20,3 MB |
| 8 siteli oturumu geri yükleme | 887 ms | 110 ms | 918 MB | 166 MB | 22,0 MB |
| 8 site hepsi açık | 938 ms | 111 ms | 1321 MB | 1030 MB | 22,5 MB |

- Oturum geri yüklemede Rust yalnızca açık sekmeyi uyandırıyor; bu testte C# sürümü sekmelerin hepsini yükledi.
- Reklamlı sitelerdeki farkın çoğu engelleyiciden: reklam betikleri hiç yüklenmiyor.
- Rust kabuğu ilk sürümde 13 MB'tı; reklam motoru, şifre kasası ve simge önbelleğiyle 20-22 MB.

## Mimari kararlar

| Konu | Karar | Neden |
|---|---|---|
| Dil | Rust (stable), tek crate, repo kökünde | C# sürümü 7 Kasım 2026'da kaldırıldı; tüm özellikleri Rust'ta |
| Pencere | Saf Win32 (`windows` crate), kendi başlık çubuğumuz (DWM + `WM_NCCALCSIZE`) | En hafif yol; tasarımda tam özgürlük |
| Arayüz çizimi | Direct2D + DirectWrite, kendi küçük widget katmanımız | HTML arayüz ayrı renderer süreci (~30 MB) demek; kendi çizimimizle Zen/Arc tarzı özgün tasarım yine mümkün |
| Motor | WebView2 (`webview2-com`), tek `CoreWebView2Environment`, sekme başına bir controller | Chromium tabanlı; giriş, DRM ve işletim sistemi entegrasyonları ayrıca doğrulanır |
| Açılır pencereler (öneriler, menüler) | Ayrı `WS_POPUP` katmanlı pencereler | WebView'in üstünde görünebilsinler (airspace sorunu) |
| Metin girişi | İlk aşamada stillendirilmiş yerel `EDIT` kontrolü, tasarım fazında kendi kutumuz | IME, pano, seçim bedavaya gelir |
| Veri | `serde_json`, **aynı** `%LOCALAPPDATA%\Sqywein` klasörü ve aynı `state.json` / `history.json` biçimi | Mevcut sekmeler, geçmiş, favoriler kaybolmaz |
| Adblock | Brave'in `adblock` crate'i (MPL-2.0) | uBlock Origin söz dizimi, scriptlet desteği, hızlı |

## Faz 1 — Motor ve çekirdek (şimdi)

1. Ortam: Rust araç zinciri, `rust/Cargo.toml`, release profili (`opt-level="s"`, LTO, `panic=abort`, strip).
2. Pencere: özel başlık çubuğu, sürükleme, büyüt/küçült/kapat, Mica/koyu tema, DPI farkındalığı (Per-Monitor v2).
3. WebView2 ortamı: profil klasörü, `--process-per-site`, Strict izleyici koruması.
4. Sekmeler: aç/kapat/geç, sekme şeridi çizimi, favicon, başlık, yükleniyor göstergesi, Ctrl+Tab, Ctrl+1..9, Ctrl+Shift+T.
5. Gezinme: adres çubuğu (URL mi arama mı?), geri/ileri/yenile/durdur, F5, Alt+←/→, F12.
6. Bellek: 30 sn sonra *uyuklama* (`MemoryUsageTargetLevel::Low` + `TrySuspend`), 5 dk sonra *uyku* (controller kapatılır), simge durumunda düşük hedef.
7. Dayanıklılık: renderer / browser süreci çökerse kurtarma, hata sayfası.
8. `NewWindowRequested` → yeni sekme, açan sekmeye dönüş.

## Faz 2 — Eşitlik (C# sürümünü emekliye ayırmak için)

- Oturum kaydet/geri yükle (yalnızca etkin sekme uyanır), `--wake-all`, `--no-rest` bayrakları.
- Geçmiş + adres çubuğu önerileri (aynı puanlama), geçmiş paneli.
- Favoriler (yıldız, Ctrl+D, panel, sıralama, yeniden adlandırma).
- İndirmeler paneli (ilerleme, iptal, aç, klasörde göster).
- RAM göstergesi (özel çalışma kümesi, süreç ağacı).
- Ölçüm: aynı `measure.ps1` senaryoları; C# ile yan yana tablo.
- Bitince: C# dosyaları silinir, Rust projesi köke taşınır.

## Faz 3 — Adblock (YouTube dahil)

1. **Ağ katmanı:** `AddWebResourceRequestedFilter("*")` + `adblock::Engine::check_network_request`. Rust'ta geri çağrı ucuz; yine de belge dışı türler için kısa yol önbelleği.
2. **Listeler:** EasyList, EasyPrivacy, uBlock filters (+ badware, privacy, unbreak), Türkçe liste. Derlenmiş motor diske serileştirilir → açılışta ms düzeyinde yükleme. Haftalık arka plan güncellemesi.
3. **Kozmetik:** `AddScriptToExecuteOnDocumentCreated` ile küçük bir enjeksiyon betiği; gizleme seçicileri ve scriptlet'ler sayfa başına motordan alınır (`url_cosmetic_resources`).
4. **YouTube:** ağ engeli yetmez (reklamlar aynı alan adından gelir). uBlock'un scriptlet kaynakları (`json-prune`, `set-constant`, `trusted-replace-xhr-response` vb.) yüklenir; `ytInitialPlayerResponse` / `/youtubei/v1/player` yanıtlarından `adPlacements`, `playerAds` budanır. Uyarı: YouTube sürekli değişiyor, liste güncellemesi bu yüzden şart.
5. Sekme başına "engellenen" sayacı, site bazında kapat/aç.
6. Testler: bilinen reklam URL'leri, kozmetik test sayfaları, YouTube'da elle kontrol listesi.

## Faz 4 — Sqywein tasarımı

Kimlik çıkış noktası: mevcut mor gece paleti (`#14121C` taban, `#AF85FC` vurgu) ve maskot (normal / soru / uyuyan).

- Tasarım token'ları (renk, köşe, boşluk, hareket) tek dosyada; açık/koyu tema.
- Fikir havuzu (mockup'larla birlikte seçilecek):
  - Uyuyan sekmeler maskotun uyuyan haliyle gösterilir — tarayıcının "hafiflik" felsefesinin görünür hali.
  - Dikey sekme modu + kompakt mod (Zen tarzı), araç çubuğunun gizlenmesi.
  - Ctrl+K komut paleti: sekme, geçmiş, favori, komut tek yerden.
  - Başlangıç sayfası: büyük arama kutusu + maskot + kısayollar (Perplexity sadeliği).
  - Yumuşak, kısa animasyonlar (sekme açılış, panel kayma), DirectComposition ile.
- Simge seti, yazı tipi, uygulama ikonu.

## Faz 5 — Paketleme

- Tek exe + `WebView2Loader` statik bağlı; MSIX veya basit kurulum.
- Otomatik güncelleme (GitHub Releases).
- CI: `cargo clippy`, `cargo test`, release derlemesi.

## Riskler

- **Rust arayüz katmanını sıfırdan yazmak** en büyük iş kalemi; Faz 1–2 birkaç hafta sürebilir.
- **YouTube engellemesi** kalıcı bir yarış; liste güncellemesi olmadan bozulur.
- **MSVC derleme araçları** gerekli (Rust'ın Windows hedefi bağlayıcı olarak `link.exe` kullanır).
