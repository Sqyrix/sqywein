using System;
using System.Collections.Generic;
using System.Linq;
using System.Text.Json.Serialization;

namespace Sqywein;

public sealed class HistoryEntry
{
    public string Url { get; set; } = "";
    public string Title { get; set; } = "";
    public DateTime LastVisit { get; set; }
    public int Visits { get; set; }

    [JsonIgnore]
    public string DisplayUrl => History.Shorten(Url);

    [JsonIgnore]
    public string When
    {
        get
        {
            var local = LastVisit.ToLocalTime();
            return local.ToString(local.Date == DateTime.Today ? "HH:mm" : "dd.MM.yyyy HH:mm");
        }
    }
}

public sealed class Suggestion
{
    public string Title { get; init; } = "";
    public string Url { get; init; } = "";
    public string DisplayUrl => History.Shorten(Url);
}

/// <summary>Bellekte küçük kalması için en yeni 5.000 adresi tutar. Veriler yalnızca bu bilgisayarda durur.</summary>
public sealed class History
{
    private const int MaxEntries = 5000;
    private readonly Dictionary<string, HistoryEntry> _entries;

    public bool IsDirty { get; private set; }

    public History(IEnumerable<HistoryEntry> entries)
    {
        _entries = new Dictionary<string, HistoryEntry>(StringComparer.Ordinal);
        foreach (var entry in entries) _entries[entry.Url] = entry;
    }

    public void Record(string url, string title)
    {
        if (!url.StartsWith("http://", StringComparison.OrdinalIgnoreCase) &&
            !url.StartsWith("https://", StringComparison.OrdinalIgnoreCase)) return;

        if (!_entries.TryGetValue(url, out var entry))
        {
            entry = new HistoryEntry { Url = url };
            _entries[url] = entry;
        }
        entry.LastVisit = DateTime.UtcNow;
        entry.Visits++;
        if (title.Length > 0) entry.Title = title;
        IsDirty = true;
    }

    public void UpdateTitle(string url, string title)
    {
        if (title.Length == 0 || !_entries.TryGetValue(url, out var entry) || entry.Title == title) return;
        entry.Title = title;
        IsDirty = true;
    }

    public void Remove(string url) => IsDirty |= _entries.Remove(url);

    public void Clear()
    {
        _entries.Clear();
        IsDirty = true;
    }

    public List<HistoryEntry> Recent(string filter, int limit)
    {
        var tokens = Tokens(filter);
        return _entries.Values
            .Where(e => tokens.Length == 0 || MatchesAll(e.Url, e.Title, tokens))
            .OrderByDescending(e => e.LastVisit)
            .Take(limit)
            .ToList();
    }

    public List<Suggestion> Suggest(string query, IEnumerable<Favorite> favorites, string skipPrefix, int limit)
    {
        var tokens = Tokens(query);
        if (tokens.Length == 0) return new List<Suggestion>();

        var scored = new Dictionary<string, (double Score, string Title)>(StringComparer.Ordinal);

        foreach (var f in favorites)
        {
            if (MatchesAll(f.Url, f.Title, tokens))
                scored[f.Url] = (1000 + TextScore(f.Url, f.Title, tokens), f.Title);
        }

        var now = DateTime.UtcNow;
        foreach (var e in _entries.Values)
        {
            // Arama sonucu sayfaları öneride gürültü yapar; geçmiş panelinde yine görünür.
            if (e.Url.StartsWith(skipPrefix, StringComparison.Ordinal) || !MatchesAll(e.Url, e.Title, tokens)) continue;
            var age = now - e.LastVisit;
            var score = TextScore(e.Url, e.Title, tokens)
                        + Math.Min(e.Visits, 20) * 3
                        + (age.TotalDays < 1 ? 30 : age.TotalDays < 7 ? 15 : 0);
            if (scored.TryGetValue(e.Url, out var existing)) score += existing.Score;
            scored[e.Url] = (score, e.Title.Length > 0 ? e.Title : scored.GetValueOrDefault(e.Url).Title ?? "");
        }

        return scored.OrderByDescending(p => p.Value.Score)
            .Take(limit)
            .Select(p => new Suggestion { Url = p.Key, Title = p.Value.Title.Length > 0 ? p.Value.Title : Shorten(p.Key) })
            .ToList();
    }

    /// <summary>Kaydeder; en eskiler sınırı aşınca düşer.</summary>
    public void Save()
    {
        if (_entries.Count > MaxEntries)
        {
            foreach (var old in _entries.Values.OrderByDescending(e => e.LastVisit).Skip(MaxEntries).ToList())
                _entries.Remove(old.Url);
        }
        Storage.SaveHistory(_entries.Values);
        IsDirty = false;
    }

    public static string Shorten(string url)
    {
        var s = url;
        foreach (var prefix in new[] { "https://www.", "http://www.", "https://", "http://" })
        {
            if (s.StartsWith(prefix, StringComparison.OrdinalIgnoreCase)) { s = s.Substring(prefix.Length); break; }
        }
        return s.EndsWith('/') ? s.Substring(0, s.Length - 1) : s;
    }

    private static string[] Tokens(string text) =>
        Fold(text).Split(' ', StringSplitOptions.RemoveEmptyEntries);

    private static bool MatchesAll(string url, string title, string[] tokens)
    {
        var u = Fold(url);
        var t = Fold(title);
        return tokens.All(k => u.Contains(k, StringComparison.Ordinal) || t.Contains(k, StringComparison.Ordinal));
    }

    private static double TextScore(string url, string title, string[] tokens)
    {
        var shortUrl = Fold(Shorten(url));
        var foldedTitle = Fold(title);
        double score = 0;
        foreach (var k in tokens)
        {
            if (shortUrl.StartsWith(k, StringComparison.Ordinal)) score += 200;
            else if (foldedTitle.StartsWith(k, StringComparison.Ordinal) ||
                     foldedTitle.Contains(" " + k, StringComparison.Ordinal)) score += 60;
            else score += 20;
        }
        return score;
    }

    /// <summary>Türkçe i/ı ayrımı aramayı bozmasın: "istanbul" yazınca "İstanbul" da bulunur.</summary>
    private static string Fold(string text) =>
        text.Replace('İ', 'i').Replace('I', 'i').Replace('ı', 'i').ToLowerInvariant();
}
