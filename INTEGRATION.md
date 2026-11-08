# Windows bütünleşmesi ve temalar

## Yapılanlar

- Kullanıcıya ait `RegisteredApplications`, tarayıcı yetenekleri, HTTP/HTTPS ProgID'leri ve HTML/PDF/görsel dosya adayları kaydedilir. `UserChoice` veya mevcut uzantı varsayılanları değiştirilmez. Windows kabuğuna ilişkilendirmelerin güncellendiği bildirilir.
- Normal ilk açılışta aday kaydı yapılır. İlk varsayılan tarayıcı önerisi yerel bir şerittir; WebView2 veya ek arka plan motoru açmaz, gezinmeyi engellemez. "Varsayılan tarayıcı yap" Windows'un Sqywein seçim ekranını açar; "Şimdi değil" öneriyi kapatır. Öneri gösterildiği bilgisi hemen kaydedilir ve sonraki açılışta tekrarlanmaz. Ayarlar → Sistem içindeki manuel düğme korunur. F6 ile önerinin native düğmelerine odaklanılabilir; Enter, Space, Tab ve Escape desteklenir.
- Windows'tan veya başka bir programdan gelen URL, çalışan aynı tarayıcıya `WM_COPYDATA` üzerinden aktarılır. Ayrı veri klasörleri ayrı örnek adları/pencere işaretleri kullanır; test penceresi kullanıcı penceresiyle karışmaz. URL query/fragment ve OAuth `state` içeriği değiştirilmez. Açılış komutundaki `--` arama sekmesine dönüşmez.
- Dosya yolları `file:///` adresine Windows API'siyle dönüştürülür. Boşluk, Unicode ve `#` bulunan HTML dosyası, SVG ve PDF'nin gerçek uygulamada açılması doğrulandı.
- Yazılan özel/loopback IP, `.local`, localhost ve IPv6 loopback adresleri varsayılan olarak HTTP kullanır. Açıkça verilen HTTP/HTTPS adresi korunur. `localhost.evil.com` yerel adres sayılmaz. Javascript/vbscript/data adres çubuğu girişleri kabul edilmez.
- Windows'un HTTP captive-portal yönlendirmesi normal gezinme yolundan açılır. Menüdeki "Ağ giriş sayfasını aç", Windows'un algılaması kullanıcıyı yönlendirmediğinde `http://www.msftconnecttest.com/redirect` adresini açar. Sürekli ağ taraması eklenmedi; sertifika doğrulaması gevşetilmedi.
- WebView2'nin harici uygulama protokolü onayı korunur. Başka programa dönüşün iptal edilen web gezinmesi olarak raporlanması, sahte hata ekranına dönüştürülmez. "Başka tarayıcıda aç" aktif HTTP/HTTPS sayfasını kurulu Edge/Chrome ile açabilir; kendimizi sistem varsayılanına tekrar yollayan bir döngü oluşturmaz. Bu işlem diğer tarayıcıdaki oturumla devam eder, Sqywein'e oturum çerezi taşımaz.

Windows'un [varsayılan uygulama ekranı bağlantısı](https://learn.microsoft.com/en-us/windows/uwp/launch-resume/launch-default-apps-settings) ve [uygulama kayıt yapısı](https://learn.microsoft.com/en-us/windows/win32/shell/default-programs) esas alındı. Modern Windows'ta varsayılan seçimi kullanıcıya aittir. Captive-portal algılaması [Windows NCSI](https://learn.microsoft.com/en-us/windows-server/networking/ncsi/ncsi-frequently-asked-questions) tarafından yapılır; tarayıcı Windows'un gönderdiği sayfayı işler.

## Dört tema

Ayarlar → Görünüm → Tema: **Açık / Koyu / Sqywein / Sistem**. Varsayılan `Sqywein`, mevcut morumsu palettir. Koyu nötr koyu, Açık nötr açık renkler kullanır. Sistem, Windows'un `AppsUseLightTheme` tercihini izler; `WM_SETTINGCHANGE` / `WM_THEMECHANGED` ve tekrar etkinleşme olaylarında güncellenir. Tema için periyodik sorgulama eklenmedi.

Renkler native sekme/araç çubuklarına, arama/adres alanlarına, menülere, iç sayfalara ve şifre önerisi görünümüne uygulanır. Açık sayfalara renk mesajı gönderilir; yeniden yükleme yapılmaz. WebView2 profilinin tercih edilen renk şeması da güncellenir: destekleyen siteler `prefers-color-scheme` ile kendi açık/koyu tasarımını seçebilir. Sitelerin CSS'i zorla yeniden renklendirilmez. Açık temada vurgu renklerinin metin kontrastı için daha koyu karşılıkları kullanılır.

## Doğrulama ve sınırlar

- 50 Rust testi başarılı, 5 isteğe bağlı test normal koşuda atlandı; Clippy uyarısız.
- 201 iç sayfa/UI kontrolü; altı sayfa, 600/1280 genişlik, üç sabit tema ve canlı Sistem önizlemesi. 28 betik/güvenlik kontrolü.
- 29 gerçek Rust/WebView2 kontrolü; tema tercihleri ve profilin gerçek `prefers-color-scheme` çıktısı dahil. Test aracı Playwright'ın renk şeması emülasyonunu kaldırarak native tercihi ölçer.
- `scripts/test-integration.cjs`: 14 kontrol. Soğuk program başlatması, açık tarayıcıya yeni işlemden bağlantı aktarılması, taklit kimlik sağlayıcıdan loopback callback'e kod/state dönüşü, portal benzeri 302/POST/303/çerez zinciri ve yerel HTML/SVG/PDF açılması.
- Native ilk öneri ve dört tema görsel olarak da incelendi. Öneriyi kapatma ve görüldü kaydının kalıcılığı doğrulandı. Windows'un tema ayarı test için değiştirilmedi.

Gerçek Google hesabıyla giriş, fiziksel Wi-Fi portalı, kurumsal proxy/istemci sertifikası ve kurulu üçüncü taraf programa gerçek özel-protokol dönüşü bu testlerin kapsamı değildir. [Google OAuth politikası](https://developers.google.com/identity/protocols/oauth2/policies) gömülü kullanıcı ajanlarını reddedebilir; doğru Windows ilişkilendirmesi, WebView2'nin Google tarafından her akışta kabul edileceği garantisini vermez. Yerel callback testini gerçek Google uyumluluğu diye değerlendirmemek gerekir. [Masaüstü OAuth akışında](https://developers.google.com/identity/protocols/oauth2/native-app) uygulamanın callback dinleyicisi ve PKCE/state doğrulaması uygulamanın sorumluluğundadır.

Bu sürüm için PWA kurulum/yönetimi, Chrome Sync, kapsamlı UI Automation ve bütün cihaz/kurum giriş biçimlerinde uyumluluk garantisi eklenmedi. Bunlar genel tarayıcı olgunluk değerlendirmesinde ayrı kalemlerdir.
