using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.IO;
using System.Runtime.CompilerServices;
using System.Text.Json;
using System.Text.Json.Serialization;
using System.Text.Json.Serialization.Metadata;

namespace Sqywein;

public sealed class Favorite : INotifyPropertyChanged
{
    private string _title = "";

    public string Title
    {
        get => _title;
        set
        {
            _title = value;
            PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(nameof(Title)));
            PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(nameof(Letter)));
        }
    }

    public string Url { get; set; } = "";

    [JsonIgnore]
    public string Letter => string.IsNullOrEmpty(Title) ? "?" : Title.Substring(0, 1).ToUpperInvariant();

    public event PropertyChangedEventHandler? PropertyChanged;
}

public sealed class SavedTab
{
    public string Title { get; set; } = "";
    public string Url { get; set; } = "";
}

public sealed class AppState
{
    public List<SavedTab> Tabs { get; set; } = new();
    public int ActiveIndex { get; set; }
    public List<DownloadItem> Downloads { get; set; } = new();
    public List<Favorite> Favorites { get; set; } = new()
    {
        new Favorite { Title = "Google", Url = "https://www.google.com/" },
        new Favorite { Title = "YouTube", Url = "https://www.youtube.com/" },
        new Favorite { Title = "GitHub", Url = "https://github.com/" },
        new Favorite { Title = "Wikipedia", Url = "https://tr.wikipedia.org/" },
    };
}

public static class Storage
{
    /// <summary>SQYWEIN_DATA ayarlıysa veriler oraya yazılır; testler gerçek profile dokunmaz.</summary>
    public static readonly string Root = Environment.GetEnvironmentVariable("SQYWEIN_DATA") is { Length: > 0 } custom
        ? custom
        : Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "Sqywein");

    private static readonly string StatePath = Path.Combine(Root, "state.json");
    private static readonly string HistoryPath = Path.Combine(Root, "history.json");

    public static AppState Load() => Read(StatePath, StorageJson.Default.AppState) ?? new AppState();
    public static void Save(AppState state) => Write(StatePath, state, StorageJson.Default.AppState);

    public static List<HistoryEntry> LoadHistory() =>
        Read(HistoryPath, StorageJson.Default.ListHistoryEntry) ?? new();

    public static void SaveHistory(IEnumerable<HistoryEntry> entries) =>
        Write(HistoryPath, new List<HistoryEntry>(entries), StorageJson.Default.ListHistoryEntry);

    private static T? Read<T>(string path, JsonTypeInfo<T> info) where T : class
    {
        if (!File.Exists(path)) return null; // İlk açılış.
        try
        {
            return JsonSerializer.Deserialize(File.ReadAllText(path), info);
        }
        catch (Exception e) when (e is IOException or JsonException or UnauthorizedAccessException)
        {
            return null; // Bozuk dosya: sıfırdan başlanır.
        }
    }

    private static void Write<T>(string path, T value, JsonTypeInfo<T> info)
    {
        Directory.CreateDirectory(Root);
        var temp = path + ".tmp";
        File.WriteAllText(temp, JsonSerializer.Serialize(value, info));
        File.Move(temp, path, overwrite: true);
    }
}

/// <summary>Derlemede üretilen JSON kodu: çalışırken yansıma kullanmaz, bellekte daha az yer tutar.</summary>
[JsonSerializable(typeof(AppState))]
[JsonSerializable(typeof(List<HistoryEntry>))]
internal partial class StorageJson : JsonSerializerContext
{
}

public sealed class DownloadItem : INotifyPropertyChanged
{
    private string _status = "";
    private double _progress;
    private bool _isActive, _showBar;

    public string Name { get; set; } = "";
    public string FilePath { get; set; } = "";

    public string Status { get => _status; set => Set(ref _status, value); }

    [JsonIgnore] public double Progress { get => _progress; set => Set(ref _progress, value); }
    [JsonIgnore] public bool IsActive { get => _isActive; set => Set(ref _isActive, value); }
    [JsonIgnore] public bool ShowBar { get => _showBar; set => Set(ref _showBar, value); }

    /// <summary>İndirmeyi başlatan görünüm; indirme bitmeden kapatılmaz.</summary>
    [JsonIgnore] public object? Owner { get; set; }
    [JsonIgnore] public Action? Cancel { get; set; }
    [JsonIgnore] public Action? Refresh { get; set; }
    [JsonIgnore] public long ExpectedBytes { get; set; }

    public event PropertyChangedEventHandler? PropertyChanged;

    private void Set<T>(ref T field, T value, [CallerMemberName] string name = "")
    {
        if (Equals(field, value)) return;
        field = value;
        PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(name));
    }
}
