using System;
using System.Collections.Generic;
using System.IO;
using System.Text.Json;

namespace Sqywein;

public sealed class Favorite
{
    public string Title { get; set; } = "";
    public string Url { get; set; } = "";
    public string Letter => string.IsNullOrEmpty(Title) ? "?" : Title.Substring(0, 1).ToUpperInvariant();
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

    public static AppState Load()
    {
        try
        {
            return JsonSerializer.Deserialize<AppState>(File.ReadAllText(StatePath)) ?? new AppState();
        }
        catch (Exception e) when (e is IOException or JsonException or UnauthorizedAccessException)
        {
            return new AppState(); // İlk açılış ya da bozuk dosya.
        }
    }

    public static void Save(AppState state)
    {
        Directory.CreateDirectory(Root);
        var temp = StatePath + ".tmp";
        File.WriteAllText(temp, JsonSerializer.Serialize(state));
        File.Move(temp, StatePath, overwrite: true);
    }
}
