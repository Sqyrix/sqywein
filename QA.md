# İşlev ve güvenlik düzeltmeleri — 7 Kasım 2026

Ayarlar mevcut görsel dil korunarak kategorilere ayrıldı ve aranabilir hale getirildi. Görsel tasarım turu sonraki çalışma için ayrıdır.

## Değişen davranışlar

- Yer işaretleri çubuğu, taşma menüsü ve yer işaretleri sayfası normal tıklamada yeni sekme açar.
- Şifre kasası yalnızca tam origin eşleştirir (şema, alan adı ve port). Alt alanlar arasında otomatik şifre paylaşılmaz. Windows Hello beklenirken sayfa değişmesi işlemi geçersiz kılar; iframe doldurma da belge kimliğiyle sınırlıdır.
- Gizli sekmelerin indirme kayıtları oturum dosyasına yazılmaz. İndirilen dosyalar cihazda kalır.
- Geçmiş profillere ayrıdır. Eski history.json varsayılan profilin geçmişi olarak korunur. Çerez anlık görüntülerinin profil sahibi ve geçerlilik numarası vardır; veri temizleme eski görüntülerin geri gelmesini engeller.
- Dondurma durumu ancak WebView2 başarılı sonuç bildirince etkin olur. Düzenlenen formlar, devam eden indirmeler/yüklemeler, medya, yakalama ve görüşme kullanan sayfalar korunur. İç çerçevelerin durumu yalnızca uyutma gerektiğinde mesajlaşmayla kontrol edilir; yanıt vermeyen veya meşgul çerçeve varsa sayfa korunur. Hiç uyutulmayacak siteler ayarlardan seçilebilir.
- Arama önerileri 300 ms bekler ve aynı anda tek istek çalıştırır. Eski yanıtlar kullanılmaz. Ağ erişiminin süre ve boyut sınırları vardır.
- Kozmetik tarayıcı yalnızca gereken sayfalarda çalışır; kapatılınca gözlemci ayrılır. Engelleyici kapalıysa motoru yüklenmez. Boş başlangıç web motoru veya gizli hesap sayfası açmaz.
- Arka plana geçen görünüm hemen düşük bellek hedefine alınır. Kurulumu tamamlanan mağaza sayfaları arka planda 30 saniye sonra güvenlik kontrolünden geçerek uyutulur.
- Sekmeler sabitlenebilir ve sessize alınabilir. Sabitleme oturuma kaydedilir; toplu kapatma sabitlenen sekmeleri korur.
- Gerçek WebView2 izin deposundan kamera, mikrofon, konum, bildirim ve pano izinleri yönetilebilir. Menüdeki “Bu sitenin izinleri” ayarlara yönlendirir.
- Chrome/Edge HTML yer işaretleri içe aktarılabilir; yinelenen ve web dışı adresler eklenmez. Şifre CSV içe aktarma mevcut kasada korunur.
- İndirme klasörü değiştirilebilir; tercihler veriler silinmeden sıfırlanabilir.
- Varsayılan tarayıcı düğmesi HKCU uygulama kaydını oluşturur ve Windows seçimini açar. Windows'un korunan varsayılan seçimi uygulama tarafından zorlanmaz. Normal açılışta yeni bağlantılar mevcut pencereye iletilir.
- Mağaza uzantıları kullanıcı isteğiyle güncellenebilir. Yeni paket ayrı dizine açılır; CRX public key ile mağaza kimliği eşleştirilip manifest.key korunur. Kurulum/güncellemeden önce gerçek izinler gösterilir. Paket boyutu ve dosya sayısı sınırlıdır.
- Uygulama güncellemesi tam yeni kopya hazır olmadan eski exe'yi değiştirmez; yeniden başlatma parametresi arama sekmesine dönüşmez.

## Doğrulama

```powershell
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo build --locked --release
$env:PLAYWRIGHT_MODULE = '<playwright modülünün tam yolu>'
node ui/test-ui.cjs
node ui/test-security.cjs
node ui/test-native.cjs
```

`test-native.cjs` gerçek release uygulamasını ui/.qa altında ayrı SQYWEIN_DATA profiliyle açar; yalnızca o test sürecinde loopback CDP kullanır. Normal uygulamaya debugging ayarı eklenmemiştir. Testler kullanıcının gerçek profiline veya varsayılan tarayıcı seçimine dokunmaz.

Canlı öneri ve mağaza paketi indirme testleri ağ gerektirdiği için standart test koşusunda atlanır. İstenirse `cargo test --locked canli_oneriler -- --ignored` ve `cargo test --locked magazadan_indir -- --ignored` ile çalışır.

## Bu turdaki sonuçlar

- Rust birim testleri: 42 başarılı, 5 standart koşuda atlanan test.
- Clippy: bütün hedeflerde `-D warnings` ile başarılı.
- Arayüz: 127 kontrol; altı sayfa, 1280 ve 600 piksel; konsol hatası veya harici istek yok.
- Güvenlik/uyutma betikleri: 18 kontrol; medya ve yakalama durumları simüle edilmiştir.
- Gerçek Rust/WebView2 uygulaması: ayrı profilde 21 entegrasyon kontrolü. Normal ve önce about:blank açılan pencereler için opener/postMessage/window.close akışları dahil.
- Canlı arama önerisi ve gerçek mağaza CRX kimlik kontrolü: başarılı.
- Windows CI dosyası eklendi; uzak GitHub koşusu bu çalışma sırasında başlatılmadı.

## Giriş penceresi ve hareket düzeltmeleri

NewWindowRequested artık yalnızca URL kopyalayarak sekme açmaz. Deferral ve SetNewWindow ile gerçek WindowProxy bağlanır; window.opener ilişkisi korunur. Uzantı penceresi bir giriş çocuğu açtığında hemen yok edilmez; geçici olarak gizli tutulur ve işlem sonrası bırakılır. Google hesabıyla gerçek Claude girişi otomatik denenmemiştir. Anthropic, Claude in Chrome için diğer Chromium tarayıcılarını desteklemediğini açıklar: https://support.claude.com/en/articles/12012173-get-started-with-claude-in-chrome

Sekme seçimi hareketli bir yüzeyle geçiş yapar; araç çubuğu hover/basılma geri bildirimi ve menü giriş/çıkışı kısa, kesilebilir geçişler kullanır. Kısa etkileşimlerde 16 ms, uzun süren yükleme göstergelerinde 33 ms zamanlama kullanılır; hareket bitince veya uygulama küçültülünce ana animasyon zamanlayıcısı durur. Kullanıcı tercihi ve Windows azaltılmış hareket ayarı desteklenir. Yerel test penceresinde sekme geçişi ve menü açma/kapatma görsel olarak da kontrol edildi; kare süresi için profiler ölçümü yapılmadı.

## Akış sayfalarında gereksiz form taraması

Şifre yöneticisinin MutationObserver'ı artık ilgisiz akış öğelerinin sınıf/stil değişimlerinde bütün belgeyi yeniden taramaz. Eklenen giriş alanları, alan özellikleri ve şifre alanı içeren kapsayıcıların görünürlük değişimleri taranır. Testte 100 ilgisiz akış değişimi tam form taraması başlatmadı; yeni şifre alanı eklenmesi taramayı başlattı. Bu CPU işini azaltır; Instagram ve GPU belleği için ölçülmüş bir toplam RAM düşüşü iddiası değildir.

## Bellek ölçümünü tekrarlama

```powershell
./scripts/measure.ps1 -BrowserId <sqywein-süreç-kimliği> -Seconds 3
```

Ölçüm seçilen tarayıcının tüm alt süreçlerini toplar; diğer uygulamaların WebView2 süreçlerini dahil etmez. Sqywein, Edge, Chrome, Brave, Opera ve Firefox süreç kimliklerini kabul eder. RAM, özel çalışma kümesidir; CPU örnek süresindeki toplam işlemci zamanı farkından hesaplanır ve tüm mantıksal işlemcilere bölünür. Süre, son süreç ağacı/CPU örneklemesini de içerir. Engelleyici kapalı yeni boş başlangıçta WebView2 alt süreci açılmadığı ayrıca doğrulandı.

Tekrarlanabilir karşılaştırma:

```powershell
$env:PLAYWRIGHT_MODULE='<playwright modülünün tam yolu>'
node scripts/benchmark.cjs
```

Üç tur, görünür pencereler (1200×850), eşit içerik görünümü (1100×700), ayrı temiz profiller, uzantısız ve reklam engelleme kapalı yerel 150 kartlık akış kullanır. Her tur boşta ve 100 ms aralıklı metin güncelleme/kaydırma yükünü ölçer. GPU açıktır; çalışma kümesi zorla budanmaz. `BRAVE_EXE` ve `OPERA_EXE` değişkenleriyle ek tarayıcılar seçilebilir; kurulu olmayanlar raporda belirtilir. Raporlar `ui/.qa/benchmark-*/` altındadır. Gerçek Instagram, oturum açılmış siteler, video ve çok sekmeli kullanım için ayrıca ölçüm gerekir. Aynı benchmark sırasında başka test/derleme çalıştırılmamalıdır. DevTools bağlantısı bütün tarayıcılarda açıktır.

## Kalkanın ek yükü

Kalkan kapalıyken dış ağ istekleri Rust köprüsünden geçmez. İç sayfaların dar URL filtresi sürekli kayıtlı kalır; gezinme sırasında kaldırılması ilk iç isteği kaçırdığı için gerçek uygulama testinde doğrulanarak düzeltildi. Kalkan açıkken tüm istek kaynakları filtrelenmeye devam eder. Kozmetik toplama en çok 4 ms / 256 öğelik adımlarla çalışır; sınıf/kimlik geçmişi ayrı ayrı 16000 adla sınırlıdır ve toplama kapatılınca bırakılır. 2000 adlık mesajlar halinde tüm adlar iletilir; önceki tek mesajdaki taşma kaybı giderildi. 2400 benzersiz sınıflı test ve yeniden etkinleştirme/iptal kontrolleri dahil 21 güvenlik/betik kontrolü geçti.

## Kapsam sınırları

Bu değişiklik Chrome'un bütün özellikleriyle eşitlik veya ölçülmüş performans garantisi değildir. Chrome Sync/Google bulut senkronizasyonu, PWA yönetimi ve özel çizilmiş yerel kontroller için tam UI Automation desteği bu turda eklenmedi. Açık tema, dikey sekmeler ve kapsamlı görsel yenileme sonraki tasarım turuna bırakıldı. Uzantı güncellemeleri kullanıcı isteğiyle yapılır; otomatik arka plan taraması eklenmedi.

PLAN.md içindeki eski C# karşılaştırmaları tarihsel ölçümlerdir. Güncel Chrome karşılaştırması, aynı siteler ve aynı uyutma koşullarıyla ayrıca ölçülmelidir. Windows Hello donanımı, gerçek kamera/mikrofon görüşmeleri ve Windows varsayılan uygulama seçimi fiziksel kullanım doğrulaması gerektirir; simüle edilen medya testleri bunların yerine geçmez.

## Otomatik kaynak yönetimi

Yeni ayar eklenmedi. Varsayılan açık `freezeTabs`, otomatik yönetimi etkinleştirir. Mevcut kapatma tercihi korunur. Her 5 saniyelik bakımda Windows'un kullanılabilir fiziksel RAM ve commit alanı okunur; eşik çevresinde sürekli mod değiştirmemek için ayrı toparlanma eşikleri kullanılır.

- Normal durumda: 30 saniyelik pasif sekme dondurulabilir; otomatik görünüm boşaltma yapılmaz.
- Yüksek baskıda: dondurma eşiği 15 saniye, güvenli otomatik boşaltma eşiği 3 dakika.
- Kritik baskıda: eşikler 10 ve 60 saniye. Her bakımda en fazla 4 sekme, diğer durumlarda en fazla 2 sekme incelenir; eski kullanılanlar önce gelir.
- RAM baskısına bağlı boşaltma; sabitlenmiş/gizli sekmeleri ve geri/ileri geçmişi olan sayfaları kapsamaz. WebView2 görünümü yeniden oluşturulurken bu geçmiş geri yüklenemediği için korunur. Bu sekmeler güvenliyse yalnızca dondurulur.
- Etkin sekme, giriş pencereleri ve açan sayfaları; yükleme/indirme, oynayan medya, form değişikliği, görüşme/yakalama ve açık WebSocket/EventSource bağlantıları korunur. Alt çerçeveler de mevcut güvenlik denetiminden geçer. Güvenli bulunmayan bir sayfa en erken 30 saniye sonra tekrar incelenir.
- Eşikler fiziksel boş RAM için en az 512 MB/%5 (kritik), 1 GB/%12 (yüksek); commit için %5/%10'dur. Toparlanma daha yüksek boş alan ister. Bunlar gerçek sistem durumuna göre otomatik uygulanır.

Site simgeleri ve PNG URL önbellekleri 128, dosya simgeleri 64, Direct2D bitmap önbelleği 192 öğeyle sınırlıdır. Kullanılmayan kayıtlar çıkarılır; baskı yükselince ek küçültme yapılır. Tamamlanan simge indirmeleri bekleyen listesinden çıkarılır; aynı anda en fazla 4 yedek simge indirmesi yapılır. Son kapatılan sekmelerin listesi 50 kayıtla sınırlıdır. Kapatılan görünümün belgeye ait reklam betikleri/istisnaları bırakılır ve geç gelen betik kayıtları görünüm nesline bağlanır.

Bakım artık koşulsuz yeniden çizim istemez; küçültülmüş pencerede RAM satırı için süreç sorgulamaz. Oturum çerezleri aynıysa tekrar DPAPI şifrelemesi/disk yazımı yapılmaz; başarısız çerez sorgusu eski anlık görüntüyü boş veriyle değiştirmez. Kozmetik filtreler tek stil öğesinde benzersiz CSS kuralları kullanır; genel filtrelemeye ihtiyacı olmayan sayfalarda DOM toplayıcısı çalışmaz. Yeni motor yüklenince açık sayfaların kozmetikleri güncellenir. Uzantı uygulamasında değişiklik yapılmadı.

Kontroller: 46 Rust testi başarılı, 5 isteğe bağlı test normal koşuda atlandı; betik dışa aktarma testi ayrıca geçti. Clippy uyarısız. 28 güvenlik/betik kontrolü ve 21 gerçek Rust/WebView2 entegrasyon kontrolü geçti. `scripts/test-resources.cjs` 90 aç/kapat döngüsü, 10/30 eşzamanlı yerel sekme, gerçek dondurma, form/canlı peer/oynayan video koruması ve değişmeyen çerez dosyası için 16 kontrol içerir. Yerel medya yapay bir canvas akışıdır; fiziksel kamera/uzak görüşme testi değildir. Windows'un belleğini yapay biçimde tüketmek yerine baskı geçişleri ve korunan sekme kararları Rust testlerinde doğrulanır.

Kaynak testi ayrı profil ve `SQYWEIN_LOG` kullanır. `PLAYWRIGHT_MODULE` ayarlandıktan sonra `node scripts/test-resources.cjs` ile çalıştırılır; sonuçlar `ui/.qa/resources-*/results.json` altındadır. 30 sekme aynı yerel origin üzerindedir; bu bir gerçek siteler karşılaştırması değildir. Saatler/günler süren dayanıklılık ve gerçek cihaz görüşme doğrulaması bu turda yapılmadı.

İki tam kaynak koşusu başarılı: `resources-1794058402373` (debug) ve `resources-1794058602611` (release). Release koşusunda döngüler sonrası kabuk 10,7/10,8/10,9 MB; tüm süreç toplamı 93,1/95,2/102,9 MB idi. Motor önbellekleri nedeniyle toplamın sabit kalması beklenmez. 10 ve 30 yerel sekme için 167,4 ve 275 MB kaydedildi; bunlar basit aynı-origin fixture değerleridir, genel web performansı iddiası değildir.

Geliştirme sırasında iki koşu kontrol hedefinin kapanmasıyla kesildi; kesin neden belirlenmedi ve son iki tam koşuda yeniden görülmedi. Sonraki tanı için yalnızca `SQYWEIN_LOG` açıkken panic, pencere kapatma ve bellek modu geçiş kayıtları eklendi. Bunlar normal kullanımda diske yazılmaz. Kaynak testinin sonundaki süreç sonlandırması yalnızca kendi oluşturduğu test uygulamasına uygulanır.


## 8 Kasım 2026 — macOS araçları, çeviri paneli ve yeni logo

- Kaynak/hedef dil sekmeleri, Windows görüntüleme dilini izleyen hedef, dil/site istisnaları ve kaynak dili düzeltmesi eklendi. Dahili sayfalar, düzenlenebilir taslaklar ve kod gönderilmez. İptal/gezinme sonuçları belge ve çeviri nesliyle korunur; ağ ve DOM işi sınırlıdır.
- Ctrl+Space Hızlı erişim; Ctrl+Shift+A küçük resimli sekme ızgarası; Shift+tık bağlantı önizlemesi; indirmelerde basılı tutma/Hızlı Bakış; resim sağ tık menüsünde yerel Windows OCR eklendi.
- PiP gerçek kullanıcı hareketiyle başlatılır; başarı/hata beklenir. Yerel video akışında açma ve sayfaya dönme gerçek WebView2 üzerinde doğrulandı.
- Paneller kapanınca WebView'ları kapatılır. Küçük resimler sırayla ve 400 piksel sınırıyla alınır; gizli denetleyicilerde CapturePreview beklemesi yerine CDP yakalaması kullanılır. Yakalama için uyuyan sekmeler uyandırılmaz. Dosya okuma ve resim çözme arayüz dışındadır; iptal edilen önizleme sonuçları uygulanmaz.
- Kullanıcının verdiği mavi yörünge PNG'si `Assets/logo-source.png` olarak birebir korundu. Başlangıç/README PNG'leri ve 16–256 piksel arasında dokuz boyutlu Windows ICO üretildi. Build betiği ICO değişikliklerini izler. `scripts/prepare-icons.py` yalnızca geliştirme zamanı dönüştürücüsüdür; çalışma zamanına bağımlılık eklemez.

Bu turda standart Rust testleri **54 başarılı / 7 atlanan**, Clippy bütün hedeflerde `-D warnings` ile başarılıdır. Ortama bağlı gerçek Windows OCR görüntü testi ve sabit metinli canlı Google çeviri testi ayrıca çalıştırıldı ve geçti. İç sayfalar **201**, güvenlik/uyutma betikleri **28**, yeni araçlar gerçek native uygulamada **25** kontrolü geçti. Kaynak testi **90 görünüm çevrimi + 30 sekmede 16** kontrolü geçti; yerel hafif test sayfaları Chrome/Edge performans karşılaştırması değildir.

Native araç testi: `node ui/test-extras.cjs`. `SQYWEIN_TEST_EXE` ile exe yolu verilebilir. Detaylı kullanım ve sınırlar [UTILITIES.md](UTILITIES.md) içindedir.

Windows Computer Use istemcisi test penceresini yeniden bağlarken aynı uygulama kimliği için sahiplik hatası verdi; bu araçla native menünün son görsel denetimi tamamlanamadı. WebView panelleri CDP üzerinden görüntülendi ve sınandı; gerçek kullanıcı hesabında Google/Claude girişi denenmedi.

Optimize release derlemesinde ayrıca 29 native uygulama ve 14 Windows/yönlendirme kontrolü başarılı oldu. Yeni panel testleri animasyonları kapatma tercihini de doğrular.


## 9 Kasım 2026 — 0.4.2 düzeltmeleri

YouTube enforcement penceresi CSS ile gizlenmez; çerçevenin kendi kapatma akışı kullanılarak karartma, odak ve kaydırma kilidi bırakılır. Yerel senaryolarda 18 YouTube kontrolü, 29 güvenlik/otomatik doldurma kontrolü, 201 iç sayfa kontrolü ve 58 Rust testi geçti. Clippy tüm hedeflerde başarılıdır. Gerçek kullanıcı hesabı veya oturum çerezleri kullanılmadı; canlı YouTube reklam sunumunun her varyantı için garanti verilmez.

Başlıkta Windows'un kendi caption bölümü kaldırıldı; özel küçült/büyüt/kapat düğmeleri tek kez çizilir. Native test, gerçek pencerenin stilini okuyarak caption bulunmadığını ve yeniden boyutlandırma/min/max yeteneklerinin korunduğunu denetler.

Son otomatik doldurma değişikliğindeki deepActive sonsuz çağrısı düzeltildi; Shadow DOM odak regresyonu eklendi. Önceki logo/kurulum değişiklikleri korunurken eski simge temizliği sadece sürüm numaralı uygulama simgeleriyle sınırlandı. Taşınabilir exe, kurulu sürümün kayıtlarını kendiliğinden değiştirmez.

Bearbyte için ui/.qa ve .bearbyte yok sayma kuralları eklendi. Yayın doğrulandıktan sonra yalnızca yerel target, ui/.qa ve dist çıktıları temizlenir; kaynaklar, varlıklar, Cargo.lock, Git/Bearbyte proje metaverisi korunur.
