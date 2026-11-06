using System;
using System.IO;
using System.Text.Json;
using Microsoft.Web.WebView2.Core;

namespace Sqywein;

/// <summary>
/// SQYWEIN_LOG bir dosya yolu gösteriyorsa gezinme ve ağ isteklerini oraya yazar.
/// Normal kullanımda kapalıdır, hiçbir şey kaydedilmez ve bellek harcanmaz.
/// </summary>
internal static class Diagnostics
{
    private static readonly string? LogPath = Environment.GetEnvironmentVariable("SQYWEIN_LOG");

    public static void Log(string line)
    {
        if (LogPath != null) File.AppendAllText(LogPath, line + Environment.NewLine);
    }

    /// <summary>Etkinliği alan pencerenin hangi uygulamaya ait olduğunu yazar.</summary>
    public static void LogForeground(string prefix)
    {
        if (LogPath == null) return;
        var window = GetForegroundWindow();
        GetWindowThreadProcessId(window, out var pid);
        string owner;
        try
        {
            using var process = System.Diagnostics.Process.GetProcessById(pid);
            owner = process.ProcessName;
        }
        catch (ArgumentException)
        {
            owner = "?";
        }
        Log($"{prefix} foreground={owner} sameProcess={pid == Environment.ProcessId}");
    }

    [System.Runtime.InteropServices.DllImport("user32.dll")]
    private static extern IntPtr GetForegroundWindow();

    [System.Runtime.InteropServices.DllImport("user32.dll")]
    private static extern uint GetWindowThreadProcessId(IntPtr window, out int pid);

    public static async void WatchNetwork(CoreWebView2 core)
    {
        if (LogPath == null) return;
        core.GetDevToolsProtocolEventReceiver("Network.requestWillBeSent").DevToolsProtocolEventReceived += (_, e) =>
        {
            using var doc = JsonDocument.Parse(e.ParameterObjectAsJson);
            Log("REQ " + doc.RootElement.GetProperty("request").GetProperty("url").GetString());
        };
        core.GetDevToolsProtocolEventReceiver("Network.responseReceived").DevToolsProtocolEventReceived += (_, e) =>
        {
            using var doc = JsonDocument.Parse(e.ParameterObjectAsJson);
            var response = doc.RootElement.GetProperty("response");
            Log($"RESP {response.GetProperty("status").GetInt32()} {response.GetProperty("url").GetString()}");
        };
        core.PermissionRequested += (_, e) => Log($"PERM {e.PermissionKind} {e.State} {e.Uri}");
        core.DownloadStarting += (_, e) => Log("DOWNLOAD " + e.ResultFilePath);
        await core.CallDevToolsProtocolMethodAsync("Network.enable", "{}");
    }
}
