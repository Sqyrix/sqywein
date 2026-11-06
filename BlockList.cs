namespace Sqywein;

/// <summary>
/// Reklam ve izleyici alan adları. Yalnızca bu desenlere uyan istekler uygulamaya
/// uğrar, yani her ağ isteğinde yönetilen koda geçiş maliyeti ödenmez.
/// </summary>
public static class BlockList
{
    public static readonly string[] Hosts =
    {
        "doubleclick.net", "googlesyndication.com", "googleadservices.com",
        "google-analytics.com", "googletagmanager.com", "googletagservices.com",
        "adservice.google.com", "2mdn.net", "adnxs.com", "adsrvr.org",
        "amazon-adsystem.com", "taboola.com", "outbrain.com", "criteo.com",
        "criteo.net", "rubiconproject.com", "pubmatic.com", "openx.net",
        "casalemedia.com", "smartadserver.com", "scorecardresearch.com",
        "quantserve.com", "moatads.com", "hotjar.com", "mouseflow.com",
        "yandexadexchange.net", "mc.yandex.ru", "adform.net", "bidswitch.net",
        "sharethrough.com", "3lift.com", "connect.facebook.net", "ads-twitter.com",
        "analytics.tiktok.com", "ads.linkedin.com", "admatic.com.tr", "adcolony.com",
        "medyanetads.com", "pghub.io", "btloader.com",
    };
}
