# Sqywein 0.4.5: çekirdek işlevler ve veri koruması

10 Kasım 2026. Bu çalışma uzantı sistemini değiştirmez. Paket ve derleme çıktıları kaynak deposunun dışındadır.

## Saklama ve token/çerez sınırları

Şifre kasası ve oturum çerezlerinin yedeği önceden de Windows CurrentUser DPAPI ile şifreliydi. Artık `state.json` ve `history*.json` dosyalarının içeriği de JSON yerine sürümlü şifreli kayıttır. Dosya adları uyumluluk için korunur. URL sorgularındaki OAuth kodları ve fragment tokenları bu dosyalarda düz metin kalmaz. Eski JSON/geçmiş ve eski DPAPI kasa/çerez biçimi otomatik taşınır; yeni yazımda düz metne geri dönüş yoktur.

Yeni DPAPI kayıtları kullanım/dosya adına bağlı ek bağlam kullanır; bir profilin kasası veya çerez yedeği başka profil dosyasına kopyalanarak açılamaz. Bu bağlam gizli anahtar değildir. Çözülmüş ara tamponlar, bırakılan parola/çerez değerleri mümkün olan yerlerde sıfırlanır; bunun süreç belleğinin tamamını koruduğu iddia edilmez.

Yazımlar dosya senkronizasyonu ve Windows write-through atomik değiştirme kullanır. Son sağlam şifreli kayıt yedeklenir; bozuk ana kayıtta yedek denenir. İkisi de açılamıyorsa kasa/geçmiş/oturum salt okunur güvenli duruma geçer ve boş veriyle üzerine yazılmaz. Tüm geçmişi veya site çerezlerini temizleme işlemleri eski yedeğin geri gelmesini de önler. Profil kimliklerinde dizin dışına çıkabilen karakterler kabul edilmez.

**Sınırlar:** CurrentUser DPAPI, aynı Windows hesabıyla çalışan zararlı yazılımı güvenilir bir uygulamadan ayıramaz. Token çalınmasının bütünüyle önlendiği veya donanıma/uygulamaya bağlı bir kasa yapıldığı iddia edilmez. WebView2'nin kendi kalıcı çerez, önbellek, IndexedDB ve localStorage depoları motor tarafından yönetilir; bunların tamamını uygulama düzeyinde yeniden şifrelemek girişleri bozabileceği için yapılmadı. Özellikle localStorage kullanan sitelerin tokenları için “diskteki her şey şifreli” garantisi yoktur. Yetkili CSV dışa aktarma bilerek düz metin üretir ve Windows Hello kapısından geçmeye devam eder.

Yeni şifreli dosyalar aynı Windows hesabını ve yeni sürümü gerektirir; eski Sqywein sürümleri bu biçimi okuyamaz. Eski exe ile aynı veri klasörüne geri dönülmemelidir. Güvenlik testleri gerçek kullanıcı dosyalarını değil ayrı `SQYWEIN_DATA` profillerini kullanır.

## Giriş, yönlendirme ve kurtarma

Google/Microsoft hesapları veya gerçek Wi-Fi ağı kullanılmadan yerel OAuth popup/opener/postMessage dönüşleri, başka süreçten URL iletimi, HTTP captive-portal yönlendirmesi ve yerel PDF açılışı kontrol edilir. Bu testler gerçek sağlayıcının giriş politikasını veya ağ geçidini taklit ederek sertifika doğrulamasını kapatmaz.

Normal sekmelerin kaydırma konumu, başlığı, URL'si, sabitlenmesi ve seçili sekmesi kaydedilir. Sayfa yüklenince konum sınırlı tekrarlarla uygulanır. Gizli sekmeler kalıcı oturuma alınmaz. Form içerikleri, şifreler ve kart bilgileri bu kurtarma mekanizmasıyla toplanmaz. Canlı formlar, çağrılar ve medya mevcut dinlenme korumasındadır. Geri/ileri geçmişini kaybettirecek controller kapatma yerine dondurma seçilir. Tam süreç çökmesinden sonra tüm sitelerin bellek içi uygulama durumunu/POST isteğini geri getirme garantisi yoktur.

Host nesneleri kapalıdır. Mesajlar boyut sınırından geçer; iç sayfanın bildirdiği kaynak ile gerçek WebView adresi, olay alındığında ve uygulamada işlendiğinde kontrol edilir. Şifre doldurma etkin sekme ve belgeye bağlanır. Alt alan adları arasında öneri devam eder, ancak farklı kökenli şifre doldururken bir dakikalık Hello doğrulaması tekrar kullanılmaz ve hedef köken doğrulama metninde gösterilir. Arka plan sayfası şifre doldurma/kaydetme akışını başlatamaz.

## Erişilebilirlik ve site paneli

Özel çizilen arayüz Windows UI Automation isim, rol, durum, sınır ve Invoke işlevleri sunar. Küçük bir native child erişilebilirlik sağlayıcısını barındırır; ana pencerenin varsayılan native çocuk ağacı ve WebView2 sayfa sağlayıcısı korunur. Sağlayıcı istemci tarafından istenene kadar ağaç güncellemesi çalışmaz. F6/Shift+F6 araç çubuğu, sekme şeridi ve sayfa arasında geçer; ok/Tab gezinmesi, Enter/Space işlemi ve Escape sayfaya dönüş vardır. Windows yüksek kontrast paleti takip edilir. Bu, Narrator/NVDA ile tüm gerçek sitelerde tamamlanmış bir kullanılabilirlik sertifikası değildir.

Adres alanının solundaki site simgesi ve Alt+I, bağlantı bilgisi panelini açar. Sertifika sahibi, veren kuruluş, süreler, protokol ve şifreleme bilgileri motorun Security olaylarından gelir. Desteklenmeyen veya henüz doğrulanmayan durumda güvenli olduğu söylenmez; HTTP açıkça şifresiz gösterilir. TLS doğrulaması veya sertifika hata engeli aşılmaz.

Panel yalnızca açıldığı sekme/gezinme/kökene işlem yapabilir; gezinme veya sekme değişince kapanır. Kamera, mikrofon, konum, bildirim ve pano izinleri burada yönetilir. Gizli sekmede bu panelden kalıcı izin yazılmaz. Site temizleme, o kökenin motor depolarına uygulanır ve eski çerez snapshotlarının yeniden yazılmasını engeller. Alan adıyla paylaşılan çerezlerden dolayı aynı alan adı ailesindeki oturumlar etkilenebilir; arayüz bunu temizlemeden önce açıklar.

## Otomatik kaynak yönetimi

- Ön planda bakım 5 saniyede, pilde veya pasif pencerede 10 saniyede, küçültülünce 15 saniyede bir çalışır.
- Çerez checkpoint ve güncelleme denetimi, bakım sayısına değil geçen süreye bağlıdır; aralık değişince zaman hesabı bozulmaz.
- Pilde cam efekti kapanır; kabuk animasyon zamanlayıcısı yaklaşık 30 FPS hedefler. Web sayfasının video/animasyon hızına müdahale edilmez.
- Çeviri DOM taraması pasif veya küçültülmüş pencerede durur; etkin sayfaya dönünce başlar. Arka planda çevrilmiş sekme bulunması tek başına tarama zamanlayıcısını açık tutmaz.
- İndirme, ses, görüşme, form, pin ve gezinme geçmişi korumaları sürer. Chromium sandbox veya GPU hızlandırması güvenlik/performans bahanesiyle kapatılmaz.

Bu değişiklik için karşılaştırmalı yeni RAM/CPU tasarruf yüzdesi ölçülmedi. Zamanlayıcı aralıkları ölçülmüş toplam kaynak kazancı olarak sunulmamalıdır.

## Doğrulama komutları

`cargo test --locked`, `cargo clippy --locked --all-targets -- -D warnings`, `cargo build --locked --release`.

Playwright modülü `PLAYWRIGHT_MODULE`, derlenen exe `SQYWEIN_TEST_EXE` ile seçilir. Yerel testler: `node ui/test-core.cjs`, `node ui/test-native.cjs`, `node ui/test-extras.cjs`, `node scripts/test-integration.cjs`, `node scripts/test-resources.cjs`. Core testi erişilebilirlik ağacını gerçek Windows UI Automation ile denetler; yalnızca kendisinin açtığı pencereyi odağı almadan görünür yapar ve iş bitince kapatır. Gerçek hesaplar, varsayılan ilişkilendirmeler ve Wi-Fi ağı değiştirilmez.

## Sonuçlar

- Rust: 63 test başarılı, 7 isteğe bağlı/ağ veya Windows bileşeni gerektiren test atlandı. Clippy tüm hedeflerde uyarısız; release derlemesi başarılı.
- İç sayfalar: 201 kontrol; güvenlik/dinlenme betikleri: 29; YouTube fixture regresyonları: 18.
- Gerçek WebView2 çekirdek testi: 22 kontrol (UIA sekme seçimi ve sayfa ağacına erişim dahil).
- Native entegrasyon: 35; Windows URL/OAuth/captive portal/PDF entegrasyonu: 14; yardımcı yüzeyler ve çeviri: 25.
- Kaynak yaşam döngüsü: 16 kontrol, 90 aç/kapat ve 30 eşzamanlı sekme. Yetim hedef kalmadı; değişmeyen çerez snapshotı yeniden yazılmadı; form, video ve peer bağlantıları korundu.
- Site paneli 680 ve 360 piksel genişlikte render edilip görsel olarak incelendi.
- Testler ayrı profillerde çalıştı. Gerçek Google/Microsoft hesapları, WhatsApp mesaj teslimi, fiziksel kamera/mikrofon ve gerçek captive Wi-Fi ağı için uçtan uca kullanıcı testi yapılmadı. Windows yüksek kontrastının farklı temaları ve Narrator/NVDA kullanımı ayrıca elle doğrulanmalı.

Yerel paket: `C:/Users/sqyri/AppData/Local/sqywein-build/packages/0.4.5`. Bu çalışma GitHub'a gönderilmedi/yayınlanmadı. Sonraki özellik rotası bu çalışma kapsamına eklenmedi.

## Dayanaklar

- [Microsoft: WebView2 güvenliği](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/security)
- [Microsoft: WebView2 performansı](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/performance)
- [Microsoft: UI Automation sağlayıcıları](https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-serversideprovider)
- [Microsoft: DPAPI CryptProtectData](https://learn.microsoft.com/en-us/windows/win32/api/dpapi/nf-dpapi-cryptprotectdata)
