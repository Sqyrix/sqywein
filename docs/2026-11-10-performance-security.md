# Sqywein: performans, pencere ve güvenlik incelemesi

Tarih: 10 Kasım 2026. İncelenen taban: v0.4.3; yerel düzeltmeler: v0.4.4.

## Bu değişiklikte

- Özel pencere çerçevesi Windows'un pasif çerçeve çizimini bastırır. Odak kaybedildiğinde cam yerine tema zemini kullanılır; DWM kenar rengi temadan alınır.
- Büyütme sınırları monitörün çalışma alanından alınır. Bu sınırların içinden ikinci kez sistem çerçeve kalınlığı çıkarılmaz. F11 tam ekran davranışı ayrıdır.
- Sekmedeki yükleme halkası adres çubuğunun sistem mavisini kullanır.
- Bildirim, kamera, mikrofon, konum ve pano izinleri site kökenini gösteren yerel panelle cevaplanır. Karar anında sekme, gezinme nesli ve gerçek WebView adresi yeniden doğrulanır. Sekme/gezinme değişince istek tamamlanır; gizli sekmelerde karar profile kaydedilmez. Panelin peş peşe açılmasını engelleyen 250 ms menü bastırması izin paneline uygulanmaz.

Bildirim testi ayrı yerel profilde gerçek WebView2 `Notification.requestPermission()` çağrısını hem reddederek hem kabul ederek doğrular. Bu, WhatsApp hesabında uçtan uca mesaj/Windows bildirimi testi değildir. Çerçeve düzeltmesinin farklı Windows/DPI/çoklu ekran kombinasyonlarında görsel doğrulaması ayrıca yapılmalıdır.

## Doğrulama

- `cargo test --locked`: 59 başarılı, 7 isteğe bağlı test atlandı.
- `cargo clippy --locked --all-targets -- -D warnings`: başarılı.
- `cargo build --locked --release`: başarılı; yerel paket sürümü 0.4.4.
- `ui/test-native.cjs`: son release exe üzerinde 35 kontrol başarılı; gerçek izin kabul/ret akışı ve OAuth popup regresyonları dahil.
- `ui/test-security.cjs`: 29 kontrol başarılı.
- `ui/test-ui.cjs`: altı iç sayfada iki pencere genişliğinde 201 kontrol başarılı.

Derleme/paket çıktıları kaynak deposunun dışında `C:/Users/sqyri/AppData/Local/sqywein-build` altında tutulur. Bu turda GitHub'a yayın yapılmadı.

## Sonraki öncelikler

1. **Ölçümlü performans bütçesi.** Aynı içerik, aynı uzantılar ve aynı bekleme süresiyle tüm süreç ağacının CPU, çalışma kümesi ve özel belleğini ölçmek; sekme geçişi ve giriş gecikmesinin p95 değerini kaydetmek. Projede zaten `TrySuspend` ve düşük bellek hedefi var; bunları yeni özellik gibi tekrar eklemek yerine arka plan zamanlayıcılarını ve süreçler arası mesajları profillemek gerekir. Rust arayüzü web sayfasının Chromium maliyetini ortadan kaldırmaz. Bu değişiklik için yeni RAM tasarrufu ölçümü yapılmadı.
2. **Boştayken gereksiz uyanmaları azaltmak.** Çeviri etkinliğini izleyen döngü, arka planda çevrilmiş sekme varken ana zamanlayıcıyı açık tutabiliyor. Çeviri, hava/kur yenilemesi ve animasyon döngülerinin görünürlük/odak durumunda çalışması ayrı bir profil çalışmasının hedefi olmalı; video ve çağrı sekmelerinin sürekliliği korunmalı.
3. **Pencere kalitesi kontrol matrisi.** Aktif/pasif, cam açık/kapalı, açık/koyu/Sqywein, %100/%150/%200 DPI, ikinci monitör, görev çubuğu üst/yan/alt, büyüt/geri al ve F11 geçişleri. Bu turda çalışma alanı hesabı için negatif koordinatlı ikinci monitör birim testi eklendi.
4. **Runtime ve izin güvenliği.** Evergreen güncellemelerinin çalışır durumda olması ve runtime sürümünün teşhiste gösterilmesi; her host mesajında köken ve gezinme bağının incelenmesi. Önizleme pencerelerinin izin engelleri korunmalı. Mevcut kaynakta güvenliği kapatan Chromium bayrakları tespit edilmedi; bu ifade kapsamlı güvenlik denetimi anlamına gelmez.
5. **Yayın kaynağının doğrulanması.** Mevcut SHA-256 kontrolüne ek olarak GitHub artifact attestation ve Windows kod imzası değerlendirilmesi. İndirme ile aynı kanaldaki özet tek başına yayıncı kimliğinin garantisi değildir. Sertifika veya imzalama anahtarı oluşturulmadı.

## Birincil kaynaklar

- [Microsoft: WebView2 performansı](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/performance)
- [Microsoft: WebView2 güvenliği](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/security)
- [Microsoft: Evergreen dağıtımı](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/distribution)
- [Microsoft: WM_NCACTIVATE ve çerçevenin yeniden çizilmesini bastırma](https://learn.microsoft.com/en-us/windows/win32/winmsg/wm-ncactivate)
- [Microsoft: WM_GETMINMAXINFO](https://learn.microsoft.com/en-us/windows/win32/winmsg/wm-getminmaxinfo)
- [GitHub: Artifact attestations](https://docs.github.com/en/actions/how-tos/secure-your-work/use-artifact-attestations/use-artifact-attestations)
