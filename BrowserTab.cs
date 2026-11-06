using System;
using System.ComponentModel;
using System.Runtime.CompilerServices;
using System.Threading.Tasks;
using System.Windows.Media;
using Microsoft.Web.WebView2.Wpf;

namespace Sqywein;

public enum TabMemory
{
    /// <summary>Tarayıcı süreci yok, yalnızca adres ve başlık tutulur.</summary>
    Asleep,
    /// <summary>Canlı ama arka planda, düşük bellek hedefinde.</summary>
    Dozing,
    Awake,
}

public sealed class BrowserTab : INotifyPropertyChanged
{
    private string _title = "Yeni sekme";
    private string _url = "";
    private bool _isActive, _isLoading;
    private ImageSource? _icon;
    private TabMemory _memory = TabMemory.Asleep;

    public WebView2? View { get; set; }
    public Task? ViewTask { get; set; }
    public DateTime LastActive { get; set; } = DateTime.UtcNow;
    public bool PlaysAudio { get; set; }

    /// <summary>Yüklemeyi kullanıcı durdurdu; sonuç hata sayılmaz.</summary>
    public bool StoppedByUser { get; set; }

    /// <summary>Doluysa sayfa yerine bu mesajla hata ekranı gösterilir.</summary>
    public string? Error { get; set; }

    /// <summary>Bu sekmeyi açan sekme; kapanınca oraya dönülür.</summary>
    public BrowserTab? Opener { get; set; }

    /// <summary>Bir kez bile sayfa yüklendi mi? Yüklenmeden indirmeye dönen açılır sekmeler kapatılır.</summary>
    public bool HasLoaded { get; set; }

    /// <summary>Görünüm hazır olunca Url yüklenmeli mi? Açılır pencereleri motor kendisi yükler.</summary>
    public bool PendingLoad { get; set; } = true;

    public string Title { get => _title; set => Set(ref _title, value); }
    public string Url { get => _url; set => Set(ref _url, value); }
    public bool IsActive { get => _isActive; set => Set(ref _isActive, value); }
    public bool IsLoading { get => _isLoading; set => Set(ref _isLoading, value); }
    public ImageSource? Icon { get => _icon; set { Set(ref _icon, value); Notify(nameof(HasIcon)); } }
    public bool HasIcon => _icon != null;

    public TabMemory Memory
    {
        get => _memory;
        set { Set(ref _memory, value); Notify(nameof(IsAsleep)); }
    }

    /// <summary>Başlangıç sayfasındaki sekmeler uyuyor sayılmaz: uyandırılacak bir şeyleri yoktur.</summary>
    public bool IsAsleep => _memory == TabMemory.Asleep && _url.Length > 0;

    public event PropertyChangedEventHandler? PropertyChanged;

    private void Set<T>(ref T field, T value, [CallerMemberName] string name = "")
    {
        if (Equals(field, value)) return;
        field = value;
        Notify(name);
        if (name == nameof(Url)) Notify(nameof(IsAsleep));
    }

    private void Notify(string name) => PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(name));
}
