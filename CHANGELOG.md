# Değişiklikler

Sqywein'in sürümden sürüme yenilikleri. En yenisi en üstte.

## 0.5.2

- Arayüzde tek vurgu rengi mavi oldu; eski renk seçimleri veri kaybı olmadan maviye taşınır.
- Sqywein teması, profil simgeleri ve otomatik doldurma dahil mor zemin/vurgular kaldırıldı.

## 0.5.1

- Eski oturum kayıtlarındaki `SessionClean: null` ve `ReadingList: null` değerleri için veri kaybetmeden okuma ve kayıt normalleştirme. Şifreli yedekleri koruyan `--repair-session` denetimi.
- Sol üstte trafik ışığı pencere düğmeleri, oval adres alanı, kenar çubuğu ve sağda paylaş/yeni sekme/genel bakış grubu.
- Sayfa alanında filtrelenebilir sekme önizlemeleri; sekme kapatma ve yeni sekme kutucuğu.
- Pencere düğmelerinde tona uygun vektör simgeler ve ince gölge; simgeli kütüphane/sekme bölümleri. Motorun sağ tık komutları korunarak yerel, temaya uygun menüde çizilir.
- DirectWrite ekran ayarları opak metin yüzeyine uygulanır; README görselleri gerçek repo ve Rust sayfası çekimleriyle yenilendi.
- Tarayıcı menüsü görev odaklı gruplara ayrıldı; yan yana açılan yerel alt menüler, klavye ve UI Automation desteği.
- Görev yöneticisi; Alt+F, Shift+Esc, Ctrl+Shift+Delete ve Ctrl+Shift+B kısayolları.
- Normal başlangıç sayfasındaki ikinci arama alanı kaldırıldı; yeni sekmeler doğrudan adres alanına odaklanır.
- Kamuya açık özellik, gizlilik ve ölçüm açıklamaları doğrulanabilir kapsam ve bilinen sınırlarla güncellendi.

## 0.5.0

### Safari menüleri ve çoklu pencere
- Ana menü Safari'nin menü çubuğuna göre yeniden kuruldu: üstte Yeni sekme / Yeni pencere / Yeni gizli pencere, altında **Dosya, Düzen, Görünüm, Geçmiş, Yer İşaretleri, Geliştir, Pencere, Yardım** alt menüleri. Alt menüler aynı panelde açılır, "‹ Geri" ile dönülür.
- **Yeni pencere (Ctrl+N)** ve **yeni gizli pencere (Ctrl+Shift+N)**. Her pencere kendi sekmelerini tutar; ayarlar, yer işaretleri, geçmiş ve şifreler pencereler arasında çakışmadan birleştirilir. "Sqywein'den çık" bütün pencereleri kapatır.
- **Okuyucu (Ctrl+Shift+R):** makalenin metnini reklamsız, sade ve okunaklı bir görünümde açar; Esc ile kapanır.
- **Okuma Listesi (Ctrl+Shift+D):** sayfayı sonra okumak üzere kaydet; Yer İşaretleri menüsünden aç veya kaldır.
- **Farklı kaydet (Ctrl+S)**, **Sayfa kaynağı (Ctrl+U)**, **Tam ekran (F11)**, **Tüm sekmeleri yer işaretlerine ekle**, Geçmiş menüsünde son ziyaret edilen siteler, Düzen menüsünde Geri al / Kes / Kopyala / Yapıştır / Tümünü seç.
- Safari'de olmayan menü öğeleri kaldırıldı (ağ giriş sayfası, başka tarayıcıda aç, gizli ↔ normal geçişi ve hızlı erişim menüden çıktı; kısayolları çalışmaya devam eder).

### Otomatik doldurma
- Kullanıcı adı alanı tespiti yeniden yazıldı: `autocomplete`, alan türü, etiket/aria adı/yer tutucu birlikte puanlanır. T.C. kimlik (sayı alanı), telefon ve müşteri numarasıyla girişler tanınır; arama kutuları, doğrulama kodu, kupon ve kart alanları atlanır.
- Açık Shadow DOM içindeki girişler (web bileşenli iki adımlı girişler) bulunur; tek kayıtlı hesap varsa yalnızca kullanıcı adı hemen yazılır, şifre her zaman seçip Windows Hello ile onaylayınca dolar.
- Bülten, iletişim ve kayıt formlarındaki e-posta alanları artık giriş sanılmaz.

## 0.4.5

- Sekme/ayar ve geçmiş dosyaları da DPAPI ile şifrelenir. Eski kayıtlar otomatik taşınır; dosya/profil bağı, diske kalıcı atomik yazım ve şifreli son sağlam yedek eklenir. Bozuk kayıt ve yedek çözülemiyorsa üzerine yazılmaz.
- Oturum kaydırma konumu kaydedilir ve yeniden açılışta geri getirilir. Geri/ileri geçmişi bulunan sekmeler bellekten atılmak yerine dondurulur. Gizli sekmelerin konumları diske yazılmaz.
- Site bilgi paneli bağlantı/sertifika ayrıntılarını, site izinlerini ve siteye özel veri temizlemeyi birleştirir. Adres alanındaki simgeden veya Alt+I ile açılır.
- Özel sekme/araç çubuğu ve menü düğmeleri Windows UI Automation üzerinden erişilebilir. Sayfanın kendi erişilebilirlik ağacı korunur; F6/Shift+F6, oklar, Tab ve Enter ile kabuk kullanılabilir. Windows yüksek kontrast renkleri izlenir.
- Pilde cam efekti durur ve kabuk animasyonları 30 FPS hedefiyle çalışır. Pasif/küçültülmüş pencerede bakım uyanmaları seyrekleşir; çeviri taraması yalnızca etkin/görünür sekmede yapılır. Çağrı, video ve indirme korumaları korunur.
- Host nesneleri kapatılır; iç sayfa mesajlarında güncel kaynak adresi tekrar doğrulanır. Şifre doldurma etkin sekmeye bağlanır; farklı kökene ait şifre için Hello doğrulaması yeniden istenir. Debug kayıtlarında URL yolu/sorgusu/fragmenti ve URL kullanıcı bilgileri tutulmaz.

## 0.4.4

- Pasif pencerede Windows'un beyaz sistem çerçevesi çizmesi engellenir; cam yerine opak tema zemini ve temayla uyumlu kenar rengi kullanılır.
- Büyütme, görev çubuğunu dışarıda bırakan monitör çalışma alanıyla sınırlandırılır; yan/üst görev çubukları ve ikinci ekran desteklenir.
- Sekme yükleme halkası adres alanıyla aynı mavi rengi kullanır.
- Site izinleri adres alanına bağlı native panelden verilir/engellenir. Bildirim, kamera, mikrofon, konum ve pano istekleri desteklenir; gezinme/sekme değişmesi bekleyen isteği iptal eder. Gizli sekmede kalıcı izin yazılmaz.

## 0.4.3

- Windows’un başlık bölgesini yeniden eklemesi başlangıçta ve tam ekrandan dönüşte engellendi. Yeniden boyutlandırma, küçültme ve büyütme korunur.

## 0.4.2

- YouTube uyarısının gizlenip tıklamayı engelleyen karartma katmanını açık bırakması düzeltildi. Reklam sonrası ses durumu geri yüklenir; site engellemesi kapatılınca YouTube modülü de durur.
- Cam başlık çubuğundaki çift küçült/büyüt/kapat simgeleri düzeltildi.
- Yerel test profilleri Bearbyte taramasından çıkarıldı; derleme/test çıktıları kaynak deposuna dahil edilmez.

- Otomatik doldurmada odak tespitinin kendini tekrar çağırıp alanları kilitlemesi düzeltildi.

## 0.4.1

- Yeni logo her yerde: güncellemeden sonraki ilk açılışta simge dosyası, Başlat menüsü, masaüstü ve görev çubuğu kısayolları ile "Yüklü uygulamalar" kendiliğinden yenilenir (Windows'un eski simgeyi göstermesi engellendi).
- Simge küçük boyutlarda daha net (logonun boş kenarları kırpıldı).
- Adres alanında ve başlangıç sayfasında yazarken titreme giderildi.
- Menüler ve öneriler ClearType ile çizilir; yazılar daha net.

## 0.4.0

### Çeviri
- Sayfa çevirisi: adres alanındaki **文A** düğmesi sayfayı yerinde, oturumu bozmadan çevirir. Sonradan yüklenen içerik de çevrilir.
- Bilgisayarın dili dışındaki siteler açılınca **kendiliğinden çevrilir**; düğmeden o sayfa için kapatılabilir, ayarlardan tamamen kapatılabilir.
- Çeviri paneli: hedef dil, "daima çevir", "asla çevirme", site istisnaları, yanlış algılanan dili düzeltme.

### Yeni araçlar (macOS'tan esinlenen)
- **Hızlı erişim** (Ctrl+Space): sekmeler, geçmiş, yer işaretleri ve komutlar tek kutuda.
- **Sekmelere genel bakış** (Ctrl+Shift+A): küçük resimli sekme ızgarası, yazarak filtreleme.
- **Bağlantı önizlemesi** (Shift+tık): sayfayı küçük pencerede açar; "Sekmede aç" deyince pencere büyüyerek sekmeye dönüşür.
- **Hızlı Bakış**: indirmeye basılı tutunca resim, metin ve PDF önizlemesi.
- **Canlı Metin**: resme sağ tık → resimdeki yazıyı Windows'un kendi metin tanımasıyla seç.
- **Görüntü içinde görüntü**: videoyu her şeyin üstünde küçük pencerede oynat.
- **Odak modu** (Ctrl+Shift+F): araç çubuğu ve sekmeler gizlenir, fare üste gelince geri gelir.
- **Paylaşım paneli**: QR kod, bağlantıyı kopyala, Windows ile paylaş, e-postayla gönder.

### Başlangıç sayfası
- Safari tarzı yeni düzen; en az 10 kez girilen siteler **Favoriler**'e kendiliğinden eklenir (silinen site geri gelmez).
- Hava durumu ve döviz yan yana iki karemsi kart.
- Başlangıç bloğu ekranda dikeyde ortalanır.
- **Hava durumu** kartı: arka planı havaya ve gece/gündüze göre değişen Liquid Glass kart, 5 günlük tahmin.
- **Döviz** kartı: Dolar, Euro, Sterlin; günlük değişim ve son günlerin küçük grafiği.
- Veriler kaydedilir; kartlar açılışta anında görünür.

### Sekmeler ve gezinme
- **Gizli ↔ normal geçiş**: sekme şeridinin solundaki düğme ya da Ctrl+Shift+P. Gizli moddayken yalnızca gizli sekmeler görünür.
- Sekme sürükleme: tutulan sekme en üstte, kartıyla birlikte hareket eder; yerine hızlıca oturur.
- **Ana sayfa düğmesi** (geri/ileri'nin yanında).
- Adres önerilerinin altında doğrudan site bağlantıları (geçmişten ve sık kullanılan sitelerden).

### Görünüm
- Pencere düğmeleri (küçült/büyüt/kapat) araç çubuğuyla aynı hizada; tam ekranda da çalışır.
- Daha akıcı animasyonlar; mavi, akıcı yükleme çizgisi; sürüklenebilir reklam engelleyici kaydırıcısı.
- Simetrik düğmeler, yumuşak kenarlı cam efektleri.

## 0.3.0

- Safari tarzı cam (Acrylic) tasarım, yeni logo.
- `SqyweinSetup.exe` kurulum dosyası, Yüklü uygulamalar kaydı, görev çubuğuna sabitleme.
- Şifreler aynı sitenin alt alan adlarında da önerilir.

## 0.2.0

- GitHub'dan otomatik güncelleme.
- Chrome gibi önce soran, Windows Hello ile dolduran şifre yöneticisi.
