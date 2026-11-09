# Değişiklikler

Sqywein'in sürümden sürüme yenilikleri. En yenisi en üstte.

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
