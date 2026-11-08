# Hızlı araçlar

Bu araçlar mevcut macOS/Safari görünümünü ve tema renklerini kullanır. Yardımcı panel kapatıldığında ona ait WebView kapatılır; sürekli açık bir yardımcı sayfa tutulmaz.

## Çeviri

Adres alanındaki **文A** düğmesi sayfayı çevirir ve kaynak/hedef dil sekmelerini açar. Kaynak sekmesi orijinal metni, hedef sekmesi çeviriyi gösterir. Üç nokta menüsünde:

- Başka hedef dil seçimi; boş tercih Windows'un **görüntüleme dilini** izler.
- Kaynak dili daima çevir / asla çevirme; bu tercihler birbirini dışlar.
- Tam site adına göre asla çevirme; alt alanlar kendiliğinden eklenmez.
- Yanlış bildirilen kaynak dilini değiştirme ve dil ayarlarını açma.

İlk kullanımda sayfalar kendiliğinden Google'a gönderilmez. Otomatik çeviri yalnızca kullanıcının “daima” listesine eklediği yabancı dillerde çalışır; dil/site engelleri önceliklidir. Kullanıcının doğrudan çeviri isteği istisnaları geçici olarak aşabilir. Bildirilmeyen dilde kaynak seçici veya Google'ın otomatik algılaması kullanılır. Listede bulunmayan Windows dili için hedef İngilizcedir.

Çeviri mevcut, resmî olmayan Google Çeviri uç noktasını kullanır: **çevrilecek sayfa metni Google'a gider**. Bu servisin süreklilik garantisi yoktur. Hata ve yeniden deneme durumu panelde gösterilir. İç ayar/şifre sayfaları, form alanları, düzenlenebilir metin, kod ve `translate=no` alanları gönderilmez.

Çeviri etkin sekmede devam eder; uyuyan sekmeler bu iş için uyandırılmaz. Tur başına en çok 80 düğüm, aynı anda iki ağ işçisi vardır. İptal/gezinme eski sonuçları ve kuyruktaki istekleri geçersiz kılar. Bir belge için 10.000 düğüm / 500.000 kaynak karakter saklama sınırı, tek düğüm için 16.000 karakter sınırı vardır; sınıra gelindiğinde kısmi çeviri bildirimi gösterilir. Yeni eklenen veya değiştirilen metin tekrar toplanır. Orijinale dönüş, sitenin daha sonra değiştirdiği metni ezmez.

## Paneller

| Araç | Açma | Davranış |
|---|---|---|
| Hızlı erişim | Ctrl+Space veya menü | Sekmeler, geçmiş, yer işaretleri ve komutlar; oklar, Enter ve Esc |
| Sekmelere genel bakış | Ctrl+Shift+A veya menü | Yazarak filtrele, sekmeye tıklayarak geç |
| Bağlantı önizlemesi | Shift+tık | İstek yalnızca bu hareketle başlar; “Sekmede aç” ve Esc |
| Hızlı Bakış | İndirmeye 500 ms basılı tut veya düğmesi | Dosya harici uygulamada çalıştırılmaz; indirme menüsünde seçip Space de kullanılabilir |
| Canlı Metin | Resme sağ tık → Canlı Metin | Görünür resim bölümünü yerel Windows OCR ile oku; çıkan metni seç veya kopyala |

Gizli sekmede Hızlı erişim geçmişi listelemez; sekme listesi normal/gizli oturumları karıştırmaz. Sekme küçük resimleri panel açılınca, en fazla 24 uyanık görünümden sırayla alınır; aynı anda tek yakalama çalışır. Uyuyan sekmelerde yer tutucu gösterilir; önizleme uğruna yeniden yüklenmezler.

Bağlantı önizlemesi normal profilin site oturumunu kullanır. Önizlemede yerel komut köprüsü, otomatik form doldurma, indirme, yeni pencere ve harici uygulama yönlendirmeleri kapalıdır. İzin istekleri reddedilirken profilin kalıcı izinleri değiştirilmez. HTTP(S) dışına gezinme engellenir; PDF önizlemesinde yalnızca seçilen yerel PDF'ye izin verilir.

Hızlı Bakış dosya okumayı ve resim çözmeyi arayüz iş parçacığından ayrı, sırayla yapar. Resimleri 1600 piksele küçültür; metin/kod dosyalarının ilk 512 KB'ını düz metin gösterir; PDF motorun kendi görüntüleyicisinde açılır. Dosya sınırı 16 MB'dır. Desteklenmeyen tür veya silinmiş dosya için açıklama gösterilir. HTML dosyası metin olarak gösterilir, çalıştırılmaz.

Canlı Metin ağa görüntü göndermez ve panoya kendiliğinden yazmaz. Windows OCR dil paketi gerekir. Ekran dışında kalan resim parçaları tanınmaz; önce kaydırarak görünür hale getirilebilir. Dil paketi veya yakalama hatası panelde bildirilir.

PiP komutu gerçek kullanıcı hareketiyle çalıştırılır ve API sonucunu bekler. Oynatılabilir video yoksa veya sayfa/motor izin vermiyorsa hata bildirilir. QR/paylaşım ve odak modu korunmuştur.

## Doğrulama

`node ui/test-extras.cjs` gerçek Rust/WebView2 üzerinde çeviri tercihlerini, gömülü çeviri betiklerini, Spotlight'ı, küçük resimleri, bağlantı önizlemesini, PiP'yi ve indirme önizlemelerini ayrı profilde sınar. `SQYWEIN_TEST_EXE` ile derleme yolu seçilebilir. Görseller `ui/.qa` altındadır.

`cargo test --locked canli_ceviri -- --ignored` yalnızca sabit “Hello world” örneğini canlı servise gönderir. `SQYWEIN_OCR_TEST_IMAGE` değişkenindeki “12345” içeren yerel görüntüyle `cargo test --locked windows_ocr_fixture -- --ignored` gerçek Windows OCR motorunu sınar. Normal test koşusu bu ortama bağlı iki testi atlar.
