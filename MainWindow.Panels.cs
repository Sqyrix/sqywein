using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Input;
using Microsoft.Web.WebView2.Core;

namespace Sqywein;

/// <summary>Adres önerileri, yan panel (geçmiş, indirmeler, favoriler).</summary>
public partial class MainWindow
{
    private const int HistoryRows = 200;

    // ---------- Adres önerileri ----------

    private TextBox? _suggestBox;

    private bool SuggestionsVisible => SuggestRoot.Visibility == Visibility.Visible;

    /// <summary>
    /// Öneri kutusunun penceresi yazmaya başlamadan önce oluşturulur ve boş bekler.
    /// Yazarken oluşturulsaydı o anda basılan tuş kayboluyordu (ölçüldü: 10 denemede 3).
    /// </summary>
    private void PrepareSuggestions()
    {
        SuggestRoot.Visibility = Visibility.Collapsed;
        SuggestPopup.PlacementTarget = AddressBorder;
        SuggestPopup.IsOpen = true;
    }

    private void UpdateSuggestions(TextBox box, FrameworkElement anchor)
    {
        if (!box.IsKeyboardFocused) return;
        var items = _history.Suggest(box.Text, _favorites, SearchUrl, 6);
        if (items.Count == 0)
        {
            CloseSuggestions();
            return;
        }
        Diagnostics.Log($"SUGGEST open text='{box.Text}' count={items.Count}");
        _suggestBox = box;
        SuggestList.ItemsSource = items;
        SuggestList.SelectedIndex = -1;
        SuggestPopup.PlacementTarget = anchor;
        SuggestRoot.Width = anchor.ActualWidth;
        if (!SuggestionsVisible)
        {
            SuggestRoot.Visibility = Visibility.Visible;
            // Pencere taşınmış ya da boyutu değişmiş olabilir; konum yeniden hesaplansın.
            SuggestPopup.HorizontalOffset = SuggestPopup.HorizontalOffset == 0 ? 0.01 : 0;
        }
    }

    /// <summary>Öneri kutusu tıklansa bile odağı almaz; yazılan tuşlar adres çubuğunda kalır.</summary>
    private static void MakeNonActivating(System.Windows.Media.Visual popupContent)
    {
        const int GwlExStyle = -20;
        const long WsExNoActivate = 0x08000000;
        if (System.Windows.PresentationSource.FromVisual(popupContent) is not System.Windows.Interop.HwndSource source) return;
        var style = GetWindowLongPtr(source.Handle, GwlExStyle).ToInt64();
        SetWindowLongPtr(source.Handle, GwlExStyle, new IntPtr(style | WsExNoActivate));
    }

    [System.Runtime.InteropServices.DllImport("user32.dll", EntryPoint = "GetWindowLongPtrW")]
    private static extern IntPtr GetWindowLongPtr(IntPtr hwnd, int index);

    [System.Runtime.InteropServices.DllImport("user32.dll", EntryPoint = "SetWindowLongPtrW")]
    private static extern IntPtr SetWindowLongPtr(IntPtr hwnd, int index, IntPtr value);

    private void CloseSuggestions()
    {
        SuggestRoot.Visibility = Visibility.Collapsed;
        _suggestBox = null;
    }

    /// <summary>Oklar öneride gezer, Enter seçileni ya da yazılanı açar, Esc kapatır.</summary>
    private bool HandleEntryKey(TextBox box, KeyEventArgs e)
    {
        if (SuggestionsVisible && _suggestBox == box && (e.Key == Key.Down || e.Key == Key.Up))
        {
            var count = SuggestList.Items.Count;
            var step = e.Key == Key.Down ? 1 : -1;
            // -1 "hiçbiri seçili değil" durumudur; oklar yazılan metne de dönebilir.
            var next = SuggestList.SelectedIndex + step;
            if (next < -1) next = count - 1;
            if (next >= count) next = -1;
            SuggestList.SelectedIndex = next;
            return true;
        }
        if (e.Key == Key.Enter)
        {
            var target = SuggestionsVisible && SuggestList.SelectedItem is Suggestion s ? s.Url : box.Text;
            Keyboard.ClearFocus();
            Navigate(target);
            return true;
        }
        if (e.Key == Key.Escape && SuggestionsVisible)
        {
            CloseSuggestions();
            return true;
        }
        return false;
    }

    private void OnAddressPreviewKeyDown(object sender, KeyEventArgs e)
    {
        if (HandleEntryKey(AddressBar, e)) { e.Handled = true; return; }
        if (e.Key == Key.Escape)
        {
            Keyboard.ClearFocus();
            RefreshToolbar();
            _active?.View?.Focus();
            e.Handled = true;
        }
    }

    private void OnStartPreviewKeyDown(object sender, KeyEventArgs e)
    {
        if (HandleEntryKey(StartSearch, e)) e.Handled = true;
    }

    private void OnAddressTextChanged(object sender, TextChangedEventArgs e) =>
        UpdateSuggestions(AddressBar, AddressBorder);

    private void OnEntryLostFocus(object sender, KeyboardFocusChangedEventArgs e)
    {
        Diagnostics.Log($"FOCUS lost text='{((TextBox)sender).Text}' new={e.NewFocus?.GetType().Name}");
        CloseSuggestions();
    }

    private void OnAddressFocus(object sender, KeyboardFocusChangedEventArgs e)
    {
        Diagnostics.Log($"FOCUS got text='{AddressBar.Text}' old={e.OldFocus?.GetType().Name}");
        AddressBar.SelectAll();
    }

    private void OnAddressMouseDown(object sender, MouseButtonEventArgs e)
    {
        if (AddressBar.IsKeyboardFocusWithin) return;
        AddressBar.Focus();
        e.Handled = true; // İlk tıklama seçimi bozmasın.
    }

    private void OnSuggestionClick(object sender, MouseButtonEventArgs e)
    {
        if (e.OriginalSource is FrameworkElement { DataContext: Suggestion s })
        {
            Keyboard.ClearFocus();
            Navigate(s.Url);
        }
    }

    // ---------- Yan panel ----------

    private void OnShowHistory(object sender, RoutedEventArgs e) => TogglePanel(HistorySection);
    private void OnShowDownloads(object sender, RoutedEventArgs e) => TogglePanel(DownloadsSection);
    private void OnShowFavorites(object sender, RoutedEventArgs e) => TogglePanel(FavoritesSection);
    private void OnClosePanel(object sender, RoutedEventArgs e) => ClosePanel();

    private void TogglePanel(FrameworkElement section)
    {
        if (SidePanel.Visibility == Visibility.Visible && section.Visibility == Visibility.Visible)
        {
            ClosePanel();
            return;
        }
        ShowPanel(section);
    }

    private void ShowPanel(FrameworkElement section)
    {
        HistorySection.Visibility = section == HistorySection ? Visibility.Visible : Visibility.Collapsed;
        DownloadsSection.Visibility = section == DownloadsSection ? Visibility.Visible : Visibility.Collapsed;
        FavoritesSection.Visibility = section == FavoritesSection ? Visibility.Visible : Visibility.Collapsed;
        PanelTitle.Text = section == HistorySection ? "Geçmiş"
            : section == DownloadsSection ? "İndirmeler"
            : "Favoriler";
        SidePanel.Visibility = Visibility.Visible;
        RefreshPanelEmptyStates();
        if (section == HistorySection)
        {
            HistorySearch.Text = "";
            RefreshHistoryPanel();
            HistorySearch.Focus();
        }
    }

    private void ClosePanel()
    {
        SidePanel.Visibility = Visibility.Collapsed;
        HistorySection.Visibility = Visibility.Collapsed;
        DownloadsSection.Visibility = Visibility.Collapsed;
        FavoritesSection.Visibility = Visibility.Collapsed;
        // Panel açıkken liste bellekte durmasın.
        HistoryList.ItemsSource = null;
        _active?.View?.Focus();
    }

    private void RefreshPanelEmptyStates()
    {
        DownloadsEmpty.Visibility = _downloads.Count == 0 ? Visibility.Visible : Visibility.Collapsed;
        FavoritesEmpty.Visibility = _favorites.Count == 0 ? Visibility.Visible : Visibility.Collapsed;
    }

    // ---------- Geçmiş ----------

    private void RefreshHistoryPanel()
    {
        var rows = _history.Recent(HistorySearch.Text, HistoryRows);
        HistoryList.ItemsSource = rows;
        HistoryEmpty.Text = HistorySearch.Text.Length > 0
            ? "Bu aramayla eşleşen bir sayfa yok."
            : "Ziyaret ettiğin sayfalar burada görünür.";
        HistoryEmpty.Visibility = rows.Count == 0 ? Visibility.Visible : Visibility.Collapsed;
    }

    private void OnHistorySearchChanged(object sender, TextChangedEventArgs e)
    {
        HistoryHint.Visibility = HistorySearch.Text.Length == 0 ? Visibility.Visible : Visibility.Collapsed;
        if (HistorySection.Visibility == Visibility.Visible) RefreshHistoryPanel();
    }

    private void OnHistoryRowClick(object sender, MouseButtonEventArgs e)
    {
        if (((FrameworkElement)sender).Tag is HistoryEntry entry) OpenUrl(entry.Url);
    }

    private void OnHistoryRemove(object sender, RoutedEventArgs e)
    {
        if (((FrameworkElement)sender).Tag is not HistoryEntry entry) return;
        _history.Remove(entry.Url);
        RefreshHistoryPanel();
    }

    private void OnHistoryClear(object sender, RoutedEventArgs e)
    {
        var answer = MessageBox.Show(this, "Bütün geçmiş silinsin mi? Bu geri alınamaz.", "Sqywein",
            MessageBoxButton.YesNo, MessageBoxImage.Question, MessageBoxResult.No);
        if (answer != MessageBoxResult.Yes) return;
        _history.Clear();
        RefreshHistoryPanel();
    }

    // ---------- İndirmeler ----------

    private void OnDownloadStarting(BrowserTab tab, CoreWebView2DownloadStartingEventArgs e)
    {
        var operation = e.DownloadOperation;
        var item = new DownloadItem
        {
            Name = Path.GetFileName(e.ResultFilePath),
            FilePath = e.ResultFilePath,
            IsActive = true,
            Owner = tab.View,
            Status = "Başlıyor…",
            Cancel = operation.Cancel,
        };
        e.Handled = true; // Motorun kendi indirme balonu yerine panelimiz görünür.
        _downloads.Insert(0, item);

        operation.BytesReceivedChanged += (_, _) => UpdateDownload(item, operation);
        operation.StateChanged += (_, _) => UpdateDownload(item, operation);
        // Yeni sekmede başlayan indirmede sekme kapanınca olaylar kesilebilir; düzenli olarak da bakılır.
        item.Refresh = () => PollDownload(item, operation);
        ShowPanel(DownloadsSection);

        // Sayfa yüklemeden indirmeye dönen açılır sekme boş kalmasın.
        if (tab.Opener != null && !tab.HasLoaded)
            Dispatcher.BeginInvoke(() => CloseTab(tab, remember: false));
    }

    private void UpdateDownload(DownloadItem item, CoreWebView2DownloadOperation operation)
    {
        switch (operation.State)
        {
            case CoreWebView2DownloadState.InProgress:
                var received = operation.BytesReceived;
                var total = operation.TotalBytesToReceive;
                item.ExpectedBytes = total > 0 ? (long)total.Value : 0;
                item.ShowBar = total > 0;
                item.Progress = total > 0 ? (double)received / total.Value : 0;
                item.Status = total > 0 ? $"{Size(received)} / {Size((long)total.Value)}" : Size(received);
                return;
            case CoreWebView2DownloadState.Completed:
                item.Status = $"Tamamlandı, {Size(operation.BytesReceived)}";
                break;
            default:
                item.Status = operation.InterruptReason == CoreWebView2DownloadInterruptReason.UserCanceled
                    ? "İptal edildi"
                    : "İndirilemedi";
                break;
        }
        FinishDownload(item);
    }

    private void FinishDownload(DownloadItem item)
    {
        item.IsActive = false;
        item.ShowBar = false;
        item.Cancel = null;
        item.Refresh = null;
        _stateDirty = true;
        ReleaseLingeringViews();
    }

    private void RefreshDownloads()
    {
        foreach (var item in _downloads.Where(d => d.IsActive).ToList()) item.Refresh?.Invoke();
    }

    private void PollDownload(DownloadItem item, CoreWebView2DownloadOperation operation)
    {
        try
        {
            UpdateDownload(item, operation);
        }
        catch (Exception e) when (e is System.Runtime.InteropServices.COMException or InvalidOperationException)
        {
            item.Status = "İndirmenin durumu alınamadı";
            FinishDownload(item);
            return;
        }
        if (!item.IsActive) return;

        // Motor olay göndermeyi kesse de dosya büyümeye devam edebilir; ilerleme dosyadan okunur.
        var file = new FileInfo(item.FilePath);
        if (!file.Exists || item.ExpectedBytes <= 0) return;
        if (file.Length >= item.ExpectedBytes)
        {
            item.Status = $"Tamamlandı, {Size(file.Length)}";
            FinishDownload(item);
        }
        else
        {
            item.Progress = (double)file.Length / item.ExpectedBytes;
            item.Status = $"{Size(file.Length)} / {Size(item.ExpectedBytes)}";
        }
    }

    private static string Size(long bytes)
    {
        if (bytes < 1024 * 1024) return $"{bytes / 1024.0:0} KB";
        if (bytes < 1024L * 1024 * 1024) return $"{bytes / (1024.0 * 1024):0.0} MB";
        return $"{bytes / (1024.0 * 1024 * 1024):0.00} GB";
    }

    private void OnDownloadOpen(object sender, MouseButtonEventArgs e)
    {
        if (((FrameworkElement)sender).Tag is not DownloadItem item || item.IsActive) return;
        if (!File.Exists(item.FilePath))
        {
            item.Status = "Dosya taşınmış ya da silinmiş";
            return;
        }
        try
        {
            Process.Start(new ProcessStartInfo(item.FilePath) { UseShellExecute = true });
        }
        catch (Win32Exception)
        {
            item.Status = "Bu dosyayı açacak bir uygulama bulunamadı";
        }
    }

    private void OnDownloadFolder(object sender, RoutedEventArgs e)
    {
        if (((FrameworkElement)sender).Tag is not DownloadItem item) return;
        if (File.Exists(item.FilePath))
            Process.Start("explorer.exe", $"/select,\"{item.FilePath}\"");
        else if (Directory.Exists(Path.GetDirectoryName(item.FilePath)))
            Process.Start("explorer.exe", $"\"{Path.GetDirectoryName(item.FilePath)}\"");
        else
            item.Status = "Klasör bulunamadı";
    }

    /// <summary>Sürüyorsa iptal eder, bittiyse yalnızca listeden çıkarır; dosya silinmez.</summary>
    private void OnDownloadDismiss(object sender, RoutedEventArgs e)
    {
        if (((FrameworkElement)sender).Tag is not DownloadItem item) return;
        if (!item.IsActive)
        {
            _downloads.Remove(item);
            return;
        }
        try
        {
            item.Cancel?.Invoke();
        }
        catch (Exception ex) when (ex is System.Runtime.InteropServices.COMException or InvalidOperationException)
        {
            item.Status = "İndirmenin durumu alınamadı";
            FinishDownload(item);
        }
    }

    // ---------- Favoriler ----------

    private void OnFavoriteOpen(object sender, RoutedEventArgs e)
    {
        if (((FrameworkElement)sender).Tag is Favorite favorite) OpenUrl(favorite.Url);
    }

    private void OnFavoriteRemove(object sender, RoutedEventArgs e)
    {
        if (((FrameworkElement)sender).Tag is not Favorite favorite) return;
        _favorites.Remove(favorite);
        RefreshToolbar();
    }

    private void OnFavoriteUp(object sender, RoutedEventArgs e) => MoveFavorite(sender, -1);
    private void OnFavoriteDown(object sender, RoutedEventArgs e) => MoveFavorite(sender, 1);

    private void MoveFavorite(object sender, int step)
    {
        if (((FrameworkElement)sender).Tag is not Favorite favorite) return;
        var from = _favorites.IndexOf(favorite);
        var to = from + step;
        if (from >= 0 && to >= 0 && to < _favorites.Count) _favorites.Move(from, to);
    }

    private void OnFavoriteRenamed(object sender, TextChangedEventArgs e) => _stateDirty = true;

    private void OnFavoriteTitleKeyDown(object sender, KeyEventArgs e)
    {
        if (e.Key == Key.Enter || e.Key == Key.Escape) Keyboard.ClearFocus();
    }
}
