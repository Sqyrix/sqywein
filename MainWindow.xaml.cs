using System;
using System.Collections.Generic;
using System.Collections.ObjectModel;
using System.ComponentModel;
using System.IO;
using System.Linq;
using System.Runtime.InteropServices;
using System.Threading.Tasks;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Input;
using System.Windows.Interop;
using System.Windows.Media.Imaging;
using System.Windows.Threading;
using Microsoft.Web.WebView2.Core;
using Microsoft.Web.WebView2.Wpf;

namespace Sqywein;

public partial class MainWindow : Window
{
    /// <summary>Arka plandaki sekme bu süre sonunda düşük bellek hedefine alınır ve dondurulur.</summary>
    private static readonly TimeSpan DozeAfter = TimeSpan.FromSeconds(30);

    /// <summary>Bu süre sonunda sekmenin süreci tamamen kapatılır, tıklanınca yeniden yüklenir.</summary>
    private static readonly TimeSpan SleepAfter = TimeSpan.FromMinutes(5);

    private const string SearchUrl = "https://www.google.com/search?q=";
    private static readonly string GlyphStop = ((char)0xE711).ToString();
    private static readonly string GlyphReload = ((char)0xE72C).ToString();
    private static readonly string GlyphLock = ((char)0xE72E).ToString();
    private static readonly string GlyphSearch = ((char)0xE721).ToString();
    private static readonly string GlyphStarFilled = ((char)0xE735).ToString();
    private static readonly string GlyphStar = ((char)0xE734).ToString();
    private static readonly string GlyphRestore = ((char)0xE923).ToString();
    private static readonly string GlyphMaximize = ((char)0xE922).ToString();

    private readonly ObservableCollection<BrowserTab> _tabs = new();
    private readonly ObservableCollection<Favorite> _favorites;
    private readonly ObservableCollection<DownloadItem> _downloads;
    private readonly History _history;
    private readonly Stack<string> _closedUrls = new();
    private readonly List<WebView2> _lingering = new();
    private readonly AppState _state;
    private readonly DispatcherTimer _housekeeping;
    private readonly bool _restEnabled;
    private CoreWebView2Environment? _env;
    private Task<CoreWebView2Environment>? _envTask;
    private BrowserTab? _active;
    private BrowserTab? _dragTab;
    private Point _dragStart;
    private bool _dragging;
    private bool _stateDirty;

    public MainWindow()
    {
        InitializeComponent();
        _state = Storage.Load();
        _history = new History(Storage.LoadHistory());
        _favorites = new ObservableCollection<Favorite>(_state.Favorites);
        _downloads = new ObservableCollection<DownloadItem>(_state.Downloads);
        TabStrip.ItemsSource = _tabs;
        FavoritesList.ItemsSource = _favorites;
        FavoritesEditor.ItemsSource = _favorites;
        DownloadsList.ItemsSource = _downloads;

        SourceInitialized += (_, _) => ApplyWindowFrame();
        SizeChanged += (_, _) => FitTabs();
        Deactivated += (_, _) => { Diagnostics.LogForeground("WINDOW deactivated"); CloseSuggestions(); };
        Activated += (_, _) => Diagnostics.Log("WINDOW activated");
        LocationChanged += (_, _) => CloseSuggestions();
        SuggestPopup.Opened += (_, _) => MakeNonActivating(SuggestRoot);
        ContentRendered += (_, _) => PrepareSuggestions();
        _tabs.CollectionChanged += (_, _) => { FitTabs(); _stateDirty = true; };
        _favorites.CollectionChanged += (_, _) => { _stateDirty = true; RefreshPanelEmptyStates(); };
        _downloads.CollectionChanged += (_, _) => { _stateDirty = true; RefreshPanelEmptyStates(); };

        // Oturum geri yüklenirken hiçbir sekme motor başlatmaz; yalnızca etkin olan uyanır.
        foreach (var saved in _state.Tabs)
            _tabs.Add(new BrowserTab { Title = saved.Title, Url = saved.Url });

        // Komut satırından gelen adresler yeni sekmede açılır: Sqywein.exe https://ornek.com
        var args = Environment.GetCommandLineArgs().Skip(1).ToList();
        var wakeAll = args.Remove("--wake-all"); // RAM karşılaştırması için: tüm sekmeler yüklensin.
        _restEnabled = !args.Remove("--no-rest"); // Karşılaştırma için: otomatik uyutma kapalı.
        var opened = args.Select(ToUrl).OfType<string>().ToList();
        foreach (var url in opened) _tabs.Add(new BrowserTab { Url = url });
        if (wakeAll)
        {
            Loaded += async (_, _) =>
            {
                foreach (var tab in _tabs.ToList())
                {
                    Activate(tab);
                    await Task.Delay(1500);
                }
            };
        }

        if (_tabs.Count == 0) _tabs.Add(new BrowserTab());
        Activate(opened.Count > 0
            ? _tabs[^1]
            : _tabs[Math.Clamp(_state.ActiveIndex, 0, _tabs.Count - 1)]);

        _housekeeping = new DispatcherTimer { Interval = TimeSpan.FromSeconds(5) };
        _housekeeping.Tick += (_, _) => { RestUnusedTabs(); UpdateRam(); RefreshDownloads(); SaveIfDirty(); };
        _housekeeping.Start();
        UpdateRam();
        _stateDirty = false; // Açılışta değişen bir şey yok; gereksiz kayıt yapılmasın.
    }

    // ---------- Motor ----------

    private Task<CoreWebView2Environment> GetEnvironment()
    {
        return _envTask ??= Create();

        async Task<CoreWebView2Environment> Create()
        {
            var options = new CoreWebView2EnvironmentOptions
            {
                // Aynı siteyi paylaşan sekmeler aynı süreci kullanabilsin.
                AdditionalBrowserArguments = "--process-per-site",
            };
            var env = await CoreWebView2Environment.CreateAsync(
                null, Path.Combine(Storage.Root, "profile"), options);
            _env = env;

            // Son görünüm kapanınca motor da kapanır. Sonraki site yeni bir ortam ister.
            env.BrowserProcessExited += (_, _) =>
            {
                if (!ReferenceEquals(_env, env)) return;
                _env = null;
                _envTask = null;
            };
            return env;
        }
    }

    private Task EnsureView(BrowserTab tab)
    {
        if (tab.View?.CoreWebView2 != null) return Task.CompletedTask;
        return tab.ViewTask ??= CreateView(tab);
    }

    private async Task CreateView(BrowserTab tab)
    {
        try
        {
            // Eski motor kapanırken yeni motor başlatılırsa profil kısa süre meşgul olabilir.
            for (var attempt = 0; attempt < 4; attempt++)
            {
                if (attempt > 0) await Task.Delay(700);
                if (!_tabs.Contains(tab)) return;

                var view = new WebView2 { Visibility = Visibility.Collapsed };
                tab.View = view;
                ContentHost.Children.Add(view);
                try
                {
                    var env = await GetEnvironment();
                    await view.EnsureCoreWebView2Async(env);
                    if (tab.View != view) return; // Sekme bu sırada kapatıldı.
                    ConfigureCore(tab, view.CoreWebView2);
                    tab.Memory = TabMemory.Awake;
                    return;
                }
                catch (Exception e) when (e is COMException or InvalidOperationException or ArgumentException)
                {
                    if (tab.View != view) return;
                    ContentHost.Children.Remove(view);
                    view.Dispose();
                    tab.View = null;
                    _env = null;
                    _envTask = null;
                }
            }

            tab.Error = "Tarayıcı motoru başlatılamadı. Yeniden dene, olmazsa Sqywein'i kapatıp aç.";
            if (tab == _active) ShowActivePage();
        }
        finally
        {
            tab.ViewTask = null;
        }
    }

    private void ConfigureCore(BrowserTab tab, CoreWebView2 core)
    {
        core.Settings.IsStatusBarEnabled = true;
        core.Settings.IsPasswordAutosaveEnabled = true;
        core.Settings.IsGeneralAutofillEnabled = true;

        // Motorun yerleşik izleyici koruması ek bellek istemez.
        core.Profile.PreferredTrackingPreventionLevel = CoreWebView2TrackingPreventionLevel.Strict;
        AttachBlocker(core);
        Diagnostics.WatchNetwork(core);

        // İndirmeleri kendi panelimiz gösterir; motorun kutusu açılırsa hemen kapatılır.
        core.IsDefaultDownloadDialogOpenChanged += (_, _) =>
        {
            if (core.IsDefaultDownloadDialogOpen) core.CloseDefaultDownloadDialog();
        };

        core.SourceChanged += (_, e) =>
        {
            tab.Url = core.Source;
            _stateDirty = true;
            // Tek sayfalık uygulamalar yeni belge yüklemeden adres değiştirir.
            if (!e.IsNewDocument) RecordVisit(core.Source, core.DocumentTitle);
            if (tab == _active) RefreshToolbar();
        };
        core.DocumentTitleChanged += (_, _) =>
        {
            tab.Title = core.DocumentTitle;
            _stateDirty = true;
            _history.UpdateTitle(core.Source, core.DocumentTitle);
            if (tab == _active) RefreshToolbar();
        };
        core.NavigationStarting += (_, _) =>
        {
            tab.IsLoading = true;
            if (tab.Error != null)
            {
                tab.Error = null;
                if (tab == _active) ShowActivePage();
            }
            if (tab == _active) RefreshToolbar();
        };
        core.NavigationCompleted += (_, e) =>
        {
            Diagnostics.Log($"NAV success={e.IsSuccess} status={e.WebErrorStatus} http={e.HttpStatusCode} source={core.Source} wanted={tab.Url}");
            tab.IsLoading = false;
            if (e.IsSuccess)
            {
                tab.HasLoaded = true;
                RecordVisit(core.Source, core.DocumentTitle);
            }
            else if (DescribeError(e.WebErrorStatus) is { } message)
            {
                tab.Error = message;
                if (tab == _active) ShowActivePage();
            }
            else if (!tab.StoppedByUser && e.HttpStatusCode == 0 && core.Source == tab.Url)
            {
                // Bağlantı reddedilince motor durumu "bilinmiyor" bildirir ama adres istenen adrestir.
                // İndirmede ise adres önceki sayfada kalır, o yüzden aşağıdaki dala düşer.
                tab.Error = "Siteye bağlanılamadı. Site kapalı olabilir.";
                if (tab == _active) ShowActivePage();
            }
            else if (core.Source.StartsWith("http", StringComparison.OrdinalIgnoreCase))
            {
                // İndirme ya da iptal edilen yüklemede sayfa değişmez; adres çubuğu da geri dönmeli.
                tab.Url = core.Source;
            }
            tab.StoppedByUser = false;
            if (tab == _active) RefreshToolbar();
        };
        core.ProcessFailed += (_, e) =>
        {
            if (e.ProcessFailedKind == CoreWebView2ProcessFailedKind.BrowserProcessExited)
            {
                RecoverFromEngineCrash();
            }
            else if (e.ProcessFailedKind == CoreWebView2ProcessFailedKind.RenderProcessExited)
            {
                tab.IsLoading = false;
                tab.Error = "Bu sayfa beklenmedik şekilde kapandı.";
                if (tab == _active) ShowActivePage();
            }
        };
        core.HistoryChanged += (_, _) => { if (tab == _active) RefreshToolbar(); };
        core.IsDocumentPlayingAudioChanged += (_, _) => tab.PlaysAudio = core.IsDocumentPlayingAudio;
        core.FaviconChanged += async (_, _) => await LoadFavicon(tab, core);
        core.WindowCloseRequested += (_, _) => CloseTab(tab);
        core.DownloadStarting += (_, e) => OnDownloadStarting(tab, e);
        core.NewWindowRequested += async (_, e) =>
        {
            // window.opener korunur; aksi halde "Google ile giriş yap" pencereleri bozulur.
            var deferral = e.GetDeferral();
            var child = new BrowserTab { Title = "Yükleniyor…", Url = e.Uri, PendingLoad = false, Opener = tab };
            _tabs.Insert(_tabs.IndexOf(tab) + 1, child);
            await EnsureView(child);
            if (child.View?.CoreWebView2 != null)
            {
                child.Memory = TabMemory.Awake;
                e.NewWindow = child.View.CoreWebView2;
                e.Handled = true;
                deferral.Complete();
                Activate(child);
            }
            else
            {
                deferral.Complete();
                Activate(child); // Hata ekranı motorun açılamadığını söyler.
            }
        };
    }

    private void RecordVisit(string url, string title)
    {
        _history.Record(url, title);
        if (HistorySection.Visibility == Visibility.Visible) RefreshHistoryPanel();
    }

    /// <summary>Kullanıcıya gösterilecek hata; null ise ekran değişmez (ör. iptal edilen yükleme, indirme).</summary>
    private static string? DescribeError(CoreWebView2WebErrorStatus status) => status switch
    {
        CoreWebView2WebErrorStatus.Unknown or
        CoreWebView2WebErrorStatus.OperationCanceled or
        CoreWebView2WebErrorStatus.ConnectionAborted or
        CoreWebView2WebErrorStatus.ValidAuthenticationCredentialsRequired or
        CoreWebView2WebErrorStatus.ValidProxyAuthenticationRequired => null,

        CoreWebView2WebErrorStatus.HostNameNotResolved =>
            "Bu adres bulunamadı. Yazımını kontrol et.",
        CoreWebView2WebErrorStatus.Disconnected =>
            "İnternet bağlantısı yok. Bağlantını kontrol edip yeniden dene.",
        CoreWebView2WebErrorStatus.Timeout =>
            "Site zamanında yanıt vermedi.",
        CoreWebView2WebErrorStatus.CannotConnect or
        CoreWebView2WebErrorStatus.ConnectionReset or
        CoreWebView2WebErrorStatus.ServerUnreachable =>
            "Siteye bağlanılamadı. Site kapalı olabilir.",
        CoreWebView2WebErrorStatus.CertificateCommonNameIsIncorrect or
        CoreWebView2WebErrorStatus.CertificateExpired or
        CoreWebView2WebErrorStatus.ClientCertificateContainsErrors or
        CoreWebView2WebErrorStatus.CertificateRevoked or
        CoreWebView2WebErrorStatus.CertificateIsInvalid =>
            "Bu sitenin güvenlik sertifikası geçersiz, bu yüzden site açılmadı.",
        _ => "Sayfa yüklenirken bir sorun oluştu.",
    };

    /// <summary>Motor çökerse sekmeler kaybolmaz: görünümler atılır, etkin sekme yeniden açılır.</summary>
    private void RecoverFromEngineCrash()
    {
        _env = null;
        _envTask = null;
        Dispatcher.BeginInvoke(() =>
        {
            foreach (var tab in _tabs) DestroyView(tab);
            if (_active != null) Activate(_active);
        });
    }

    private static void AttachBlocker(CoreWebView2 core)
    {
        foreach (var host in BlockList.Hosts)
        {
            core.AddWebResourceRequestedFilter($"*://*.{host}/*", CoreWebView2WebResourceContext.All,
                CoreWebView2WebResourceRequestSourceKinds.All);
            core.AddWebResourceRequestedFilter($"*://{host}/*", CoreWebView2WebResourceContext.All,
                CoreWebView2WebResourceRequestSourceKinds.All);
        }
        core.WebResourceRequested += (_, e) =>
        {
            Diagnostics.Log("BLOCK " + e.Request.Uri);
            e.Response = core.Environment.CreateWebResourceResponse(null, 403, "Blocked", "");
        };
    }

    private static async Task LoadFavicon(BrowserTab tab, CoreWebView2 core)
    {
        try
        {
            using var stream = await core.GetFaviconAsync(CoreWebView2FaviconImageFormat.Png);
            if (stream == null || stream.Length == 0) { tab.Icon = null; return; }
            var image = new BitmapImage();
            image.BeginInit();
            image.CacheOption = BitmapCacheOption.OnLoad;
            image.DecodePixelWidth = 32; // Büyük simgeleri bellekte tam boy tutma.
            image.StreamSource = stream;
            image.EndInit();
            image.Freeze();
            tab.Icon = image;
        }
        catch (Exception e) when (e is COMException or NotSupportedException or InvalidOperationException)
        {
            tab.Icon = null; // Simge bozuksa sekme noktayla görünmeye devam eder.
        }
    }

    // ---------- Sekmeler ----------

    private void Activate(BrowserTab tab)
    {
        if (_active != null && _active != tab)
        {
            _active.IsActive = false;
            _active.LastActive = DateTime.UtcNow;
            if (_active.View != null) _active.View.Visibility = Visibility.Collapsed;
        }

        _active = tab;
        tab.IsActive = true;
        tab.LastActive = DateTime.UtcNow;
        _stateDirty = true;

        ShowActivePage();
        RefreshToolbar();
        if (tab.Url.Length == 0)
        {
            StartSearch.Text = "";
            StartSearch.Focus();
            return;
        }
        if (tab.Error == null) _ = Wake(tab);
    }

    /// <summary>Başlangıç, hata ve web sayfasından hangisinin görüneceğine tek yerden karar verir.</summary>
    private void ShowActivePage()
    {
        var tab = _active;
        if (tab == null) return;
        var isStart = tab.Url.Length == 0;
        var isError = !isStart && tab.Error != null;

        StartPage.Visibility = isStart ? Visibility.Visible : Visibility.Collapsed;
        ErrorPage.Visibility = isError ? Visibility.Visible : Visibility.Collapsed;
        if (isError)
        {
            ErrorMessage.Text = tab.Error!;
            ErrorUrl.Text = History.Shorten(tab.Url);
        }
        // Web görünümü her zaman WPF'nin üstünde çizilir; hata ekranı için gizlenmesi gerekir.
        if (tab.View?.CoreWebView2 != null)
            tab.View.Visibility = isStart || isError ? Visibility.Collapsed : Visibility.Visible;
    }

    private async Task Wake(BrowserTab tab)
    {
        await EnsureView(tab);
        if (tab != _active || tab.View?.CoreWebView2 == null) return;

        var core = tab.View.CoreWebView2;
        tab.Memory = TabMemory.Awake;
        ApplyMemoryLevels();
        if (core.IsSuspended) core.Resume();
        if (tab.PendingLoad)
        {
            tab.PendingLoad = false;
            core.Navigate(tab.Url);
        }
        ShowActivePage();
        Diagnostics.Log("WAKE focus to view");
        tab.View.Focus();
        RefreshToolbar();
    }

    private void NewTab(string url = "")
    {
        var tab = new BrowserTab { Url = url };
        _tabs.Add(tab);
        Activate(tab);
    }

    /// <summary>Başlangıç sayfasındaysan orada, değilsen yeni sekmede açar.</summary>
    private void OpenUrl(string url)
    {
        if (_active != null && _active.Url.Length == 0) Navigate(url);
        else NewTab(url);
    }

    private void CloseTab(BrowserTab tab, bool remember = true)
    {
        var index = _tabs.IndexOf(tab);
        if (index < 0) return;

        if (remember && tab.Url.Length > 0) _closedUrls.Push(tab.Url);
        DestroyView(tab);
        _tabs.RemoveAt(index);

        if (_tabs.Count == 0)
        {
            Close();
            return;
        }
        if (tab == _active)
        {
            _active = null;
            var next = tab.Opener != null && _tabs.Contains(tab.Opener)
                ? tab.Opener
                : _tabs[Math.Min(index, _tabs.Count - 1)];
            Activate(next);
        }
    }

    private void DestroyView(BrowserTab tab)
    {
        if (tab.View == null) return;
        var view = tab.View;
        tab.View = null;
        tab.Memory = TabMemory.Asleep;
        tab.PendingLoad = true;
        tab.IsLoading = false;
        tab.PlaysAudio = false;

        // Görünüm kapanırsa başlattığı indirme de yarıda kalabilir; indirme bitene kadar beklet.
        if (_downloads.Any(d => d.IsActive && ReferenceEquals(d.Owner, view)))
        {
            view.Visibility = Visibility.Collapsed;
            _lingering.Add(view);
            return;
        }
        ContentHost.Children.Remove(view);
        view.Dispose();
    }

    private void ReleaseLingeringViews()
    {
        foreach (var view in _lingering.ToList())
        {
            if (_downloads.Any(d => d.IsActive && ReferenceEquals(d.Owner, view))) continue;
            _lingering.Remove(view);
            ContentHost.Children.Remove(view);
            view.Dispose();
        }
    }

    /// <summary>Etkin olmayan, ses çalmayan sekmeleri önce dondurur, sonra sürecini kapatır.</summary>
    private void RestUnusedTabs()
    {
        if (!_restEnabled) return;
        var now = DateTime.UtcNow;
        foreach (var tab in _tabs)
        {
            if (tab == _active || tab.View?.CoreWebView2 == null || tab.PlaysAudio) continue;
            // İndirme yapan görünüm dondurulursa indirme de durabilir.
            if (_downloads.Any(d => d.IsActive && ReferenceEquals(d.Owner, tab.View))) continue;
            var idle = now - tab.LastActive;
            if (idle >= SleepAfter) DestroyView(tab);
            else if (idle >= DozeAfter && tab.Memory == TabMemory.Awake) _ = Doze(tab);
        }
    }

    /// <summary>
    /// Pencere küçültülünce tüm sekmeler düşük bellek hedefine alınır. Dondurulmazlar,
    /// yani müzik ve indirmeler sürer. Pencere açılınca dondurulmamış sekmeler normale döner.
    /// </summary>
    private void ApplyMemoryLevels()
    {
        var minimized = WindowState == WindowState.Minimized;
        foreach (var tab in _tabs)
        {
            var core = tab.View?.CoreWebView2;
            if (core == null) continue;
            core.MemoryUsageTargetLevel = minimized || tab.Memory == TabMemory.Dozing
                ? CoreWebView2MemoryUsageTargetLevel.Low
                : CoreWebView2MemoryUsageTargetLevel.Normal;
        }
    }

    private static async Task Doze(BrowserTab tab)
    {
        tab.Memory = TabMemory.Dozing;
        var core = tab.View?.CoreWebView2;
        if (core == null) return;
        core.MemoryUsageTargetLevel = CoreWebView2MemoryUsageTargetLevel.Low;
        try
        {
            await core.TrySuspendAsync();
        }
        catch (Exception e) when (e is COMException or InvalidOperationException)
        {
            // Sekme o sırada kapanmış olabilir; ilk uyku turunda yeniden denenmez.
        }
    }

    private void Navigate(string text)
    {
        CloseSuggestions();
        var url = ToUrl(text);
        if (url == null || _active == null) return;
        _active.Url = url;
        _active.Error = null;
        ShowActivePage();
        if (_active.View?.CoreWebView2 != null)
        {
            _active.PendingLoad = false;
            _active.View.CoreWebView2.Navigate(url);
            _active.View.Focus();
        }
        else
        {
            _active.PendingLoad = true;
            _ = Wake(_active);
        }
        RefreshToolbar();
    }

    internal static string? ToUrl(string text)
    {
        text = text.Trim();
        if (text.Length == 0) return null;
        if (text.Contains("://") && Uri.TryCreate(text, UriKind.Absolute, out var abs)) return abs.AbsoluteUri;
        var looksLikeHost = !text.Contains(' ') &&
            (text.Contains('.') || text.StartsWith("localhost", StringComparison.OrdinalIgnoreCase));
        if (looksLikeHost && Uri.TryCreate("https://" + text, UriKind.Absolute, out var host))
            return text.StartsWith("localhost", StringComparison.OrdinalIgnoreCase)
                ? "http://" + text
                : host.AbsoluteUri;
        return SearchUrl + Uri.EscapeDataString(text);
    }

    // ---------- Arayüz durumu ----------

    /// <summary>Sekmeler sığmayınca daralır; şerit başlık düğmelerinin altına taşmaz.</summary>
    private void FitTabs()
    {
        const double reserved = 138 + 10 + 28 + 48 + 34 + 2; // başlık düğmeleri, kenar, logo, sürükleme alanı, + düğmesi
        var available = Math.Max(0, ActualWidth - reserved);
        TabStrip.Tag = Math.Clamp(available / Math.Max(1, _tabs.Count) - 2, 36, 196);
    }

    private void RefreshToolbar()
    {
        var core = _active?.View?.CoreWebView2;
        BackButton.IsEnabled = core?.CanGoBack == true;
        ForwardButton.IsEnabled = core?.CanGoForward == true;
        ReloadButton.IsEnabled = core != null;
        ReloadButton.Content = _active?.IsLoading == true ? GlyphStop : GlyphReload;

        var url = _active?.Url ?? "";
        if (!AddressBar.IsKeyboardFocused) AddressBar.Text = url;
        LockIcon.Text = url.StartsWith("https://", StringComparison.OrdinalIgnoreCase) ? GlyphLock : GlyphSearch;
        var saved = _favorites.Any(f => f.Url == url);
        StarButton.Content = saved ? GlyphStarFilled : GlyphStar;
        StarButton.Foreground = (System.Windows.Media.Brush)FindResource(saved ? "Accent" : "Muted");
        StarButton.IsEnabled = url.Length > 0;
        Title = _active == null || _active.Url.Length == 0 ? "Sqywein" : $"{_active.Title} - Sqywein";
    }

    private void UpdateRam()
    {
        var pids = new List<int> { Environment.ProcessId };
        // Canlı görünüm yoksa motor da yoktur: başlangıç sayfasında ölçülecek süreç kalmaz.
        var hasEngine = _env != null && (_lingering.Count > 0 || _tabs.Any(t => t.View?.CoreWebView2 != null));
        if (hasEngine)
        {
            try
            {
                pids.AddRange(_env!.GetProcessInfos().Select(p => p.ProcessId));
            }
            catch (Exception e) when (e is COMException or InvalidOperationException)
            {
                // Motor o an kapanıyor olabilir; gösterge bir sonraki turda düzelir.
            }
        }

        var mb = MemoryMeter.Total(pids) / (1024 * 1024);
        RamText.Text = $"{mb} MB";
        var asleep = _tabs.Count(t => t.IsAsleep);
        RamText.ToolTip = $"{pids.Count} süreç, {_tabs.Count} sekme, {asleep} sekme uyuyor";
        AwakeMascot.Visibility = asleep > 0 ? Visibility.Collapsed : Visibility.Visible;
        SleepingMascot.Visibility = asleep > 0 ? Visibility.Visible : Visibility.Collapsed;
        StartRam.Text = asleep > 0
            ? $"Şu an {mb} MB kullanıyor, {asleep} sekme uyuyor"
            : $"Şu an {mb} MB kullanıyor";
    }

    private void ApplyWindowFrame()
    {
        var hwnd = new WindowInteropHelper(this).Handle;
        var dark = 1;
        DwmSetWindowAttribute(hwnd, 20, ref dark, sizeof(int)); // DWMWA_USE_IMMERSIVE_DARK_MODE
        var round = 2;
        DwmSetWindowAttribute(hwnd, 33, ref round, sizeof(int)); // DWMWA_WINDOW_CORNER_PREFERENCE
    }

    [DllImport("dwmapi.dll")]
    private static extern int DwmSetWindowAttribute(IntPtr hwnd, int attribute, ref int value, int size);

    // ---------- Olaylar ----------

    private void OnNewTab(object sender, RoutedEventArgs e) => NewTab();
    private void OnBack(object sender, RoutedEventArgs e) => _active?.View?.CoreWebView2?.GoBack();
    private void OnForward(object sender, RoutedEventArgs e) => _active?.View?.CoreWebView2?.GoForward();

    private void OnReload(object sender, RoutedEventArgs e)
    {
        var core = _active?.View?.CoreWebView2;
        if (core == null) return;
        if (_active!.IsLoading)
        {
            _active.StoppedByUser = true;
            core.Stop();
        }
        else
        {
            core.Reload();
        }
    }

    private void OnRetry(object sender, RoutedEventArgs e)
    {
        if (_active == null) return;
        var url = _active.Url;
        _active.Error = null;
        // Motor açılamadıysa görünüm yoktur; Navigate bu durumda yeniden başlatır.
        Navigate(url);
    }

    private void OnTabMouseDown(object sender, MouseButtonEventArgs e)
    {
        if (((FrameworkElement)sender).Tag is not BrowserTab tab) return;
        Activate(tab);
        _dragTab = tab;
        _dragStart = e.GetPosition(TabStrip);
        _dragging = false;
        TabStrip.CaptureMouse();
    }

    private void OnTabStripMouseMove(object sender, MouseEventArgs e)
    {
        if (_dragTab == null) return;
        var position = e.GetPosition(TabStrip);
        if (!_dragging && Math.Abs(position.X - _dragStart.X) < 6) return;
        _dragging = true;

        var slot = (double)TabStrip.Tag + 2;
        var target = Math.Clamp((int)(position.X / slot), 0, _tabs.Count - 1);
        var current = _tabs.IndexOf(_dragTab);
        if (current >= 0 && current != target) _tabs.Move(current, target);
    }

    private void OnTabStripMouseUp(object sender, MouseButtonEventArgs e)
    {
        if (e.ChangedButton != MouseButton.Left || _dragTab == null) return;
        _dragTab = null;
        _dragging = false;
        TabStrip.ReleaseMouseCapture();
        // Sekmenin sayfası odağı geri alsın.
        _active?.View?.Focus();
    }

    private void OnTabStripLostCapture(object sender, MouseEventArgs e)
    {
        _dragTab = null;
        _dragging = false;
    }

    private void OnTabMouseUp(object sender, MouseButtonEventArgs e)
    {
        if (e.ChangedButton == MouseButton.Middle && ((FrameworkElement)sender).Tag is BrowserTab tab) CloseTab(tab);
    }

    private void OnCloseTabClick(object sender, RoutedEventArgs e)
    {
        if (((FrameworkElement)sender).Tag is BrowserTab tab) CloseTab(tab);
    }

    private static BrowserTab? TabOfMenu(object sender) => ((FrameworkElement)sender).DataContext as BrowserTab;

    private void OnCloseTabMenu(object sender, RoutedEventArgs e)
    {
        if (TabOfMenu(sender) is { } tab) CloseTab(tab);
    }

    private void OnSleepTabMenu(object sender, RoutedEventArgs e)
    {
        if (TabOfMenu(sender) is { } tab && tab != _active) { DestroyView(tab); UpdateRam(); }
    }

    private void OnSleepOthersMenu(object sender, RoutedEventArgs e)
    {
        foreach (var tab in _tabs.Where(t => t != _active && !t.PlaysAudio)) DestroyView(tab);
        UpdateRam();
    }

    private void OnStartTextChanged(object sender, TextChangedEventArgs e)
    {
        StartHint.Visibility = StartSearch.Text.Length == 0 ? Visibility.Visible : Visibility.Collapsed;
        UpdateSuggestions(StartSearch, StartBorder);
    }

    private void OnFavoriteClick(object sender, MouseButtonEventArgs e)
    {
        if (((FrameworkElement)sender).Tag is Favorite favorite) Navigate(favorite.Url);
    }

    private void OnToggleFavorite(object sender, RoutedEventArgs e) => ToggleFavorite();

    private void ToggleFavorite()
    {
        if (_active == null || _active.Url.Length == 0) return;
        var existing = _favorites.FirstOrDefault(f => f.Url == _active.Url);
        if (existing != null) _favorites.Remove(existing);
        else _favorites.Add(new Favorite { Title = _active.Title, Url = _active.Url });
        RefreshToolbar();
    }

    private void OnPreviewKeyDown(object sender, KeyEventArgs e)
    {
        var key = e.Key == Key.System ? e.SystemKey : e.Key;
        var ctrl = Keyboard.Modifiers.HasFlag(ModifierKeys.Control);
        var shift = Keyboard.Modifiers.HasFlag(ModifierKeys.Shift);
        var alt = Keyboard.Modifiers.HasFlag(ModifierKeys.Alt);
        var core = _active?.View?.CoreWebView2;
        var handled = true;

        if (ctrl && shift && key == Key.T) { if (_closedUrls.Count > 0) NewTab(_closedUrls.Pop()); }
        else if (ctrl && shift && key == Key.O) TogglePanel(FavoritesSection);
        else if (ctrl && key == Key.T) NewTab();
        else if (ctrl && key == Key.W) { if (_active != null) CloseTab(_active); }
        else if (ctrl && key == Key.L) { AddressBar.Focus(); AddressBar.SelectAll(); }
        else if (ctrl && key == Key.D) ToggleFavorite();
        else if (ctrl && key == Key.H) TogglePanel(HistorySection);
        else if (ctrl && key == Key.J) TogglePanel(DownloadsSection);
        else if (ctrl && key == Key.Tab) CycleTab(shift ? -1 : 1);
        else if (ctrl && key is >= Key.D1 and <= Key.D9)
        {
            var index = key == Key.D9 ? _tabs.Count - 1 : key - Key.D1;
            if (index < _tabs.Count) Activate(_tabs[index]);
        }
        else if (alt && key == Key.Left) core?.GoBack();
        else if (alt && key == Key.Right) core?.GoForward();
        else if (key == Key.F5 || (ctrl && key == Key.R)) core?.Reload();
        else if (key == Key.F12) core?.OpenDevToolsWindow();
        else handled = false;

        e.Handled = handled;
    }

    private void CycleTab(int step)
    {
        if (_active == null || _tabs.Count < 2) return;
        var index = (_tabs.IndexOf(_active) + step + _tabs.Count) % _tabs.Count;
        Activate(_tabs[index]);
    }

    private void OnMinimize(object sender, RoutedEventArgs e) => WindowState = WindowState.Minimized;

    private void OnMaximize(object sender, RoutedEventArgs e) =>
        WindowState = WindowState == WindowState.Maximized ? WindowState.Normal : WindowState.Maximized;

    private void OnCloseWindow(object sender, RoutedEventArgs e) => Close();

    private void OnStateChanged(object? sender, EventArgs e)
    {
        var maximized = WindowState == WindowState.Maximized;
        // Ekranı kaplayınca Windows pencereyi kenarlardan taşırır; içerik kesilmesin.
        RootGrid.Margin = maximized ? SystemParameters.WindowResizeBorderThickness : new Thickness(0);
        MaxButton.Content = maximized ? GlyphRestore : GlyphMaximize;
        ApplyMemoryLevels();
    }

    private void SaveIfDirty()
    {
        try
        {
            if (_stateDirty)
            {
                SaveState();
                _stateDirty = false;
            }
            if (_history.IsDirty) _history.Save();
        }
        catch (Exception ex) when (ex is IOException or UnauthorizedAccessException)
        {
            // Sonraki turda yeniden denenir; tarayıcı kullanımı engellenmez.
        }
    }

    private void SaveState()
    {
        var saved = _tabs.Where(t => t.Url.Length > 0).ToList();
        _state.Tabs = saved.Select(t => new SavedTab { Title = t.Title, Url = t.Url }).ToList();
        _state.ActiveIndex = _active == null ? 0 : Math.Max(0, saved.IndexOf(_active));
        _state.Favorites = _favorites.ToList();
        _state.Downloads = _downloads.Where(d => !d.IsActive).Take(30).ToList();
        Storage.Save(_state);
    }

    private void OnClosing(object? sender, CancelEventArgs e)
    {
        var running = _downloads.Count(d => d.IsActive);
        if (running > 0)
        {
            var answer = MessageBox.Show(this,
                $"{running} indirme sürüyor. Kapatırsan yarıda kalır. Yine de kapatılsın mı?",
                "Sqywein", MessageBoxButton.YesNo, MessageBoxImage.Warning, MessageBoxResult.No);
            if (answer != MessageBoxResult.Yes)
            {
                e.Cancel = true;
                return;
            }
        }

        _housekeeping.Stop();
        _stateDirty = true;
        SaveIfDirty();
    }
}
