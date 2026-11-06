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

    private readonly ObservableCollection<BrowserTab> _tabs = new();
    private readonly ObservableCollection<Favorite> _favorites;
    private readonly Stack<string> _closedUrls = new();
    private readonly AppState _state;
    private readonly DispatcherTimer _housekeeping;
    private CoreWebView2Environment? _env;
    private Task<CoreWebView2Environment>? _envTask;
    private BrowserTab? _active;
    private readonly bool _restEnabled;
    private bool _profileReady;

    public MainWindow()
    {
        InitializeComponent();
        _state = Storage.Load();
        _favorites = new ObservableCollection<Favorite>(_state.Favorites);
        TabStrip.ItemsSource = _tabs;
        FavoritesList.ItemsSource = _favorites;

        SourceInitialized += (_, _) => ApplyWindowFrame();
        SizeChanged += (_, _) => FitTabs();
        _tabs.CollectionChanged += (_, _) => FitTabs();

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
        _housekeeping.Tick += (_, _) => { RestUnusedTabs(); UpdateRam(); };
        _housekeeping.Start();
        UpdateRam();
    }

    // ---------- Motor ----------

    private Task<CoreWebView2Environment> GetEnvironment()
    {
        return _envTask ??= Create();

        static async Task<CoreWebView2Environment> Create()
        {
            var options = new CoreWebView2EnvironmentOptions
            {
                // Aynı siteyi paylaşan sekmeler aynı süreci kullanabilsin.
                AdditionalBrowserArguments = "--process-per-site",
            };
            return await CoreWebView2Environment.CreateAsync(
                null, Path.Combine(Storage.Root, "profile"), options);
        }
    }

    private async Task EnsureView(BrowserTab tab)
    {
        if (tab.View != null) return;

        var view = new WebView2 { Visibility = Visibility.Collapsed };
        tab.View = view;
        ContentHost.Children.Add(view);

        _env = await GetEnvironment();
        await view.EnsureCoreWebView2Async(_env);

        // Sekme bu bekleme sırasında kapatılmış olabilir.
        if (tab.View != view) return;

        var core = view.CoreWebView2;
        core.Settings.IsStatusBarEnabled = true;
        core.Settings.IsPasswordAutosaveEnabled = true;
        core.Settings.IsGeneralAutofillEnabled = true;

        if (!_profileReady)
        {
            // Profil tüm sekmelerde ortaktır; motorun yerleşik izleyici koruması ek bellek istemez.
            core.Profile.PreferredTrackingPreventionLevel = CoreWebView2TrackingPreventionLevel.Strict;
            _profileReady = true;
        }
        AttachBlocker(core);

        core.SourceChanged += (_, _) =>
        {
            tab.Url = core.Source;
            if (tab == _active) RefreshToolbar();
        };
        core.DocumentTitleChanged += (_, _) => tab.Title = core.DocumentTitle;
        core.NavigationStarting += (_, _) => { tab.IsLoading = true; if (tab == _active) RefreshToolbar(); };
        core.NavigationCompleted += (_, _) => { tab.IsLoading = false; if (tab == _active) RefreshToolbar(); };
        core.HistoryChanged += (_, _) => { if (tab == _active) RefreshToolbar(); };
        core.IsDocumentPlayingAudioChanged += (_, _) => tab.PlaysAudio = core.IsDocumentPlayingAudio;
        core.FaviconChanged += async (_, _) => await LoadFavicon(tab, core);
        core.WindowCloseRequested += (_, _) => CloseTab(tab);
        core.NewWindowRequested += async (_, e) =>
        {
            // window.opener korunur; aksi halde "Google ile giriş yap" pencereleri bozulur.
            var deferral = e.GetDeferral();
            var child = new BrowserTab { Title = "Yükleniyor…", Url = e.Uri, PendingLoad = false };
            _tabs.Insert(_tabs.IndexOf(tab) + 1, child);
            await EnsureView(child);
            child.Memory = TabMemory.Awake;
            e.NewWindow = child.View!.CoreWebView2;
            e.Handled = true;
            deferral.Complete();
            Activate(child);
        };

        tab.Memory = TabMemory.Awake;
    }

    private void AttachBlocker(CoreWebView2 core)
    {
        foreach (var host in BlockList.Hosts)
        {
            core.AddWebResourceRequestedFilter($"*://*.{host}/*", CoreWebView2WebResourceContext.All,
                CoreWebView2WebResourceRequestSourceKinds.All);
            core.AddWebResourceRequestedFilter($"*://{host}/*", CoreWebView2WebResourceContext.All,
                CoreWebView2WebResourceRequestSourceKinds.All);
        }
        core.WebResourceRequested += (_, e) =>
            e.Response = _env!.CreateWebResourceResponse(null, 403, "Blocked", "");
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

        if (tab.Url.Length == 0)
        {
            StartPage.Visibility = Visibility.Visible;
            StartSearch.Text = "";
            RefreshToolbar();
            StartSearch.Focus();
            return;
        }

        StartPage.Visibility = Visibility.Collapsed;
        _ = Wake(tab);
        RefreshToolbar();
    }

    private async Task Wake(BrowserTab tab)
    {
        await EnsureView(tab);
        if (tab != _active || tab.View?.CoreWebView2 == null) return;

        var core = tab.View.CoreWebView2;
        tab.View.Visibility = Visibility.Visible;
        tab.Memory = TabMemory.Awake;
        ApplyMemoryLevels();
        if (core.IsSuspended) core.Resume();
        if (tab.PendingLoad)
        {
            tab.PendingLoad = false;
            core.Navigate(tab.Url);
        }
        tab.View.Focus();
        RefreshToolbar();
    }

    private void NewTab(string url = "")
    {
        var tab = new BrowserTab { Url = url };
        _tabs.Add(tab);
        Activate(tab);
    }

    private void CloseTab(BrowserTab tab)
    {
        var index = _tabs.IndexOf(tab);
        if (index < 0) return;

        if (tab.Url.Length > 0) _closedUrls.Push(tab.Url);
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
            Activate(_tabs[Math.Min(index, _tabs.Count - 1)]);
        }
    }

    private void DestroyView(BrowserTab tab)
    {
        if (tab.View == null) return;
        ContentHost.Children.Remove(tab.View);
        tab.View.Dispose();
        tab.View = null;
        tab.Memory = TabMemory.Asleep;
        tab.PendingLoad = true;
        tab.IsLoading = false;
        tab.PlaysAudio = false;
    }

    /// <summary>Etkin olmayan, ses çalmayan sekmeleri önce dondurur, sonra sürecini kapatır.</summary>
    private void RestUnusedTabs()
    {
        if (!_restEnabled) return;
        var now = DateTime.UtcNow;
        foreach (var tab in _tabs)
        {
            if (tab == _active || tab.View?.CoreWebView2 == null || tab.PlaysAudio) continue;
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
        var url = ToUrl(text);
        if (url == null || _active == null) return;
        _active.Url = url;
        StartPage.Visibility = Visibility.Collapsed;
        if (_active.View?.CoreWebView2 != null)
        {
            _active.PendingLoad = false;
            _active.View.Visibility = Visibility.Visible;
            _active.View.CoreWebView2.Navigate(url);
            _active.View.Focus();
        }
        else
        {
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
        ReloadButton.Content = _active?.IsLoading == true ? "" : "";

        var url = _active?.Url ?? "";
        if (!AddressBar.IsKeyboardFocused) AddressBar.Text = url;
        LockIcon.Text = url.StartsWith("https://", StringComparison.OrdinalIgnoreCase) ? "" : "";
        var saved = _favorites.Any(f => f.Url == url);
        StarButton.Content = saved ? "" : "";
        StarButton.Foreground = (System.Windows.Media.Brush)FindResource(saved ? "Accent" : "Muted");
        StarButton.IsEnabled = url.Length > 0;
        Title = _active == null || _active.Url.Length == 0 ? "Sqywein" : $"{_active.Title} - Sqywein";
    }

    private void UpdateRam()
    {
        var own = Environment.ProcessId;
        var pids = new List<int> { own };
        // Canlı görünüm yoksa motor da yoktur: başlangıç sayfasında ölçülecek süreç kalmaz.
        var hasEngine = _env != null && _tabs.Any(t => t.View?.CoreWebView2 != null);
        var processCount = 1;
        if (hasEngine)
        {
            var infos = _env!.GetProcessInfos();
            pids.AddRange(infos.Select(p => p.ProcessId));
            processCount += infos.Count;
        }

        var total = MemoryMeter.Total(pids);
        var mb = total / (1024 * 1024);
        RamText.Text = $"{mb} MB";
        var asleep = _tabs.Count(t => t.IsAsleep);
        RamText.ToolTip = $"{processCount} süreç, {_tabs.Count} sekme, {asleep} sekme uyuyor";
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
        if (_active!.IsLoading) core.Stop();
        else core.Reload();
    }

    private void OnTabMouseDown(object sender, MouseButtonEventArgs e)
    {
        if (((FrameworkElement)sender).Tag is BrowserTab tab) Activate(tab);
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

    private void OnAddressKeyDown(object sender, KeyEventArgs e)
    {
        if (e.Key == Key.Enter) { var text = AddressBar.Text; Keyboard.ClearFocus(); Navigate(text); }
        else if (e.Key == Key.Escape) { Keyboard.ClearFocus(); RefreshToolbar(); }
    }

    private void OnAddressFocus(object sender, KeyboardFocusChangedEventArgs e) => AddressBar.SelectAll();

    private void OnAddressMouseDown(object sender, MouseButtonEventArgs e)
    {
        if (AddressBar.IsKeyboardFocusWithin) return;
        AddressBar.Focus();
        e.Handled = true; // İlk tıklama seçimi bozmasın.
    }

    private void OnStartKeyDown(object sender, KeyEventArgs e)
    {
        if (e.Key == Key.Enter) Navigate(StartSearch.Text);
    }

    private void OnStartTextChanged(object sender, TextChangedEventArgs e) =>
        StartHint.Visibility = StartSearch.Text.Length == 0 ? Visibility.Visible : Visibility.Collapsed;

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
        else if (ctrl && key == Key.T) NewTab();
        else if (ctrl && key == Key.W) { if (_active != null) CloseTab(_active); }
        else if (ctrl && key == Key.L) { AddressBar.Focus(); AddressBar.SelectAll(); }
        else if (ctrl && key == Key.D) ToggleFavorite();
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
        MaxButton.Content = maximized ? "" : "";
        ApplyMemoryLevels();
    }

    private void OnClosing(object? sender, CancelEventArgs e)
    {
        _housekeeping.Stop();
        _state.Tabs = _tabs.Where(t => t.Url.Length > 0)
            .Select(t => new SavedTab { Title = t.Title, Url = t.Url }).ToList();
        var saved = _tabs.Where(t => t.Url.Length > 0).ToList();
        _state.ActiveIndex = _active == null ? 0 : Math.Max(0, saved.IndexOf(_active));
        _state.Favorites = _favorites.ToList();
        try
        {
            Storage.Save(_state);
        }
        catch (Exception ex) when (ex is IOException or UnauthorizedAccessException)
        {
            // Kapanışı engelleme; en kötü ihtimalle oturum geri yüklenmez.
        }
    }
}
