# Yerel kaynak karşılaştırması — 7 Kasım 2026

Bu tablo otomatik kaynak yönetimi eklenmeden önce alınan ölçümdür. Yeni RAM baskısı yönetimi, sınırlı önbellekler, kozmetik kural birleştirme ve kaynak döngüsü kontrolleri `QA.md` içinde açıklanır; aşağıdaki rakamlar yeni sürüm için tekrar ölçülmüş sonuç gibi kullanılmamalıdır.

Aynı Windows bilgisayarda, görünür 1200×850 pencerelerde, 1100×700 içerik görünümüyle üç tur ölçüldü. Her tarayıcı her turda ayrı temiz profil kullandı. Uzantılar ve Sqywein reklam engellemesi kapalıydı; GPU açıktı. Çalışma kümeleri zorla budanmadı. Test sayfası 150 kart ve paylaşılan SVG görseli içerir; hareketli aşamada her 100 ms'de 150 metin güncellenir ve sayfa kaydırılır. Yükün gerçekten çalıştığı sayaçla doğrulandı. Tur sırası dönüşümlüydü.

RAM seçilen uygulamanın **tüm alt süreçlerinin özel çalışma kümesi toplamıdır**. CPU tüm mantıksal işlemcilere göre normalize edilmiştir. Her örnek yaklaşık 5 saniyedir; aşağıdakiler üç turun ortanca değerleridir.

| Tarayıcı | Boşta RAM | Hareketli RAM | Boşta CPU | Hareketli CPU |
| --- | ---: | ---: | ---: | ---: |
| Sqywein | 94,5 MB | 150,0 MB | %0,09 | %0,46 |
| Edge | 207,5 MB | 211,7 MB | %0,29 | %2,65 |
| Chrome | 162,7 MB | 198,2 MB | %0,09 | %1,58 |

Sqywein bu yerel testin hareketli aşamasında Edge'den yaklaşık %29, Chrome'dan %24 daha az RAM kullandı. Boşta fark sırasıyla yaklaşık %54 ve %42 idi. Sqywein RAM aralığı boşta 90,7–98,2 MB, hareketli aşamada 148,1–153,0 MB oldu. CPU kısa örneklerde değişkendir; özellikle diğer tarayıcılardaki başlangıç işleri sonucu etkileyebilir. Soğuk başlangıç süreleri ölçülmedi. Sqywein'in normal varsayılanındaki açık reklam engelleyici bu sonuçlara dahil değildir.

Edge/WebView2 sürümü 156.0.4314.58; Chrome sürümü 156.0.8078.49. Brave ve Opera bu bilgisayarda kurulu bulunmadı; onlar için sonuç yoktur.

Bu sayfa canlı Instagram, video, oturum açılmış hesap, uzantı, uzun kullanım veya çok sekmeli test değildir. Gerçek Instagram'daki 450–700 MB tüketimin çözüldüğü ya da tüm sitelerde diğer tarayıcılardan hafif olunduğu sonucunu vermez. Yerel sonuç, küçük yerel arayüz ve mevcut WebView2 yapısıyla düşük ek yükün mümkün olduğunu gösterir. Instagram'da aynı hesap, içerik, kaydırma ve video durumu ile ayrıca karşılaştırma gerekir.

## Değişiklikler

- Kalkan kapalı veya site için devre dışıyken dış ağ istekleri Rust olay köprüsünden geçmez. İç sayfaların dar filtresi sürekli kayıtlıdır. Kalkan açıldığında tüm kaynak türleri yeniden filtrelenir.
- Kozmetik DOM toplama 4 ms / 256 öğelik kısa adımlara bölündü. Sınıf ve kimlik geçmişleri ayrı ayrı 16000 kayıtla sınırlı; kapatıldığında bekleyen işler ve kayıtlar bırakılır.
- Mesajlar 2000 adlık partilere ayrılır. Önceden 2000'den sonraki adların tek mesajda kaybolması giderildi. Yeniden etkinleştirme gereksiz tam tarama başlatmaz.
- Ölçüm aracı Edge'in başlangıçta süreç devretmesini doğru ele alır; CPU hesabında kesirli işlemci saniyeleri korunur ve son örnekleme süresi hesaba katılır.

Microsoft'un [WebView2 performans rehberi](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/performance) de gereksiz süreçler arası iletişimi azaltmayı, GPU hızlandırmasını korumayı ve aynı içeriği Edge ile karşılaştırmayı önerir.

## Tekrarlama ve ham veri

`scripts/benchmark.cjs` üç tur çalıştırır. Playwright `PLAYWRIGHT_MODULE` ile seçilebilir. Ek tarayıcılar için `BRAVE_EXE` / `OPERA_EXE` yolları verilebilir. Kullanıcının profilleri ve açık tarayıcıları kullanılmaz; yalnızca testin başlattığı uygulamalar kapatılır. Aynı anda başka test/derleme çalıştırılmamalıdır.

Ham süreç kırılımları, sürümler ve tur sonuçları: `ui/.qa/benchmark-1794056655879/results.json`; ortancalar: aynı klasörde `summary.json`. Bu çalışma makinesinde saklanır; `.qa` Git'e dahil edilmez.

Kontroller: 42 Rust birim testi başarılı, 5 elle çalıştırılan test atlandı; 21 güvenlik/betik kontrolü, 127 iç sayfa kontrolü ve 21 gerçek Rust/WebView2 entegrasyon kontrolü başarılı. Clippy tüm hedeflerde uyarısız geçti.

## Son paketin önce/sonra karşılaştırması

Otomatik kaynak yönetiminden önce yedeklenen `sqywein.before-auto-resources-20261107-164048.exe` ve yeni release, yukarıdaki aynı tek sekmeli yerel fixture üzerinde üçer kez ölçüldü. Temiz ayrı profiller, görünür eşit pencereler, açık GPU ve kapalı reklam engelleme kullanıldı; çalıştırma sırası dönüşümlüydü. Ortancalar:

| Ölçüm | Önce | Sonra |
| --- | ---: | ---: |
| Boşta toplam RAM | 92,1 MB | 91,8 MB |
| Hareketli toplam RAM | 149,1 MB | 150,5 MB |
| Boşta CPU | %0,09 | %0,11 |
| Hareketli CPU | %0,51 | %0,36 |

Tek aktif sekmede belirgin bir RAM kazancı gösterilemedi: boşta -0,3 MB, hareketli aşamada +1,4 MB. Hareketli RAM aralıkları önce 148,5–151,4 MB, sonra 149,3–151,4 MB idi ve çakışıyor. CPU ortancası düşük çıkmasına rağmen yeni sürümde örnekler %0,19–%1,26 arasında değişti; bu kısa ölçümlerden güvenilir bir yüzde hızlanma sonucu çıkarılamaz.

Bu karşılaştırma, kalkan açık kozmetik filtrelemeyi, sınırlı önbelleklerin uzun kullanım etkisini veya RAM baskısındaki otomatik boşaltmayı ölçmez. Yeni işlevlerin doğruluk/kaynak döngüsü testleri `QA.md` içindedir; onların geçmesi, bu paketin toplam RAM'i belirli bir yüzde azalttığını kanıtlamaz.

Ham veri: `ui/.qa/benchmark-1794060125899/results.json`, ortancalar aynı dizindeki `summary.json`. Tekrarlamak için `BENCH_BEFORE_EXE` yedek exe yoluna ayarlanıp `node scripts/benchmark.cjs` çalıştırılır. Araç yedeği kendi test klasörüne `sqywein.exe` adıyla kopyalar; kullanıcının profilini veya kurulu sürümünü değiştirmez.
