//! WebView2 motoru: ortam, sekme görünümleri ve olaylar.
//!
//! Motorun geri çağrıları uygulama durumuna doğrudan dokunmaz; olayı kuyruğa
//! koyup ana pencereye mesaj gönderir. Böylece iç içe çağrılar (re-entrancy)
//! ve aynı anda iki yerden durum değiştirme sorunu hiç oluşmaz.

use crate::{image::Image, state::Settings, ui, url};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    sync::{atomic::{AtomicIsize, Ordering}, Mutex},
};
use webview2_com::{Microsoft::Web::WebView2::Win32::*, *};
use windows::{
    core::*,
    Win32::{Foundation::*, System::Com::IStream, UI::{Shell::SHCreateMemStream, WindowsAndMessaging::*}},
};
// webview2_com'un kendi Result türü de var; Windows'unkini kullanıyoruz.
use windows::core::Result;

pub const WM_ENGINE: u32 = WM_APP + 1;

pub enum Event {
    EnvReady(Option<ICoreWebView2Environment>),
    ViewReady(u32, Option<ICoreWebView2Controller>),
    /// Ana çerçevede yeni gezinme başladı (hedef adres).
    NavStarting(u32, String),
    /// Sayfadaki sınıf ve kimlik adları (genel kozmetik kurallar için).
    Cosmetic(u32, Vec<String>, Vec<String>),
    /// Belge oluşurken çalışan betiğin kimliği: (sekme, yuva, kimlik).
    ScriptAdded(u32, u8, String),
    NavCompleted(u32, bool, COREWEBVIEW2_WEB_ERROR_STATUS),
    SourceChanged(u32),
    TitleChanged(u32),
    HistoryChanged(u32),
    NewWindow(u32, String),
    ProcessFailed(u32, COREWEBVIEW2_PROCESS_FAILED_KIND),
    Shortcut(u32),
    FullScreen(u32),
    FaviconChanged(u32),
    /// Site simgesi: çözülmüş resim ve önbelleğe yazılacak PNG baytları.
    Favicon(u32, Option<Image>, Vec<u8>),
    /// Oturum çerezlerinin anlık görüntüsü (kapanırken saklanır).
    Cookies(Vec<CookieData>),
    Zoom(u32),
    /// İç sayfadan gelen mesaj (JSON).
    WebMessage(u32, String),
    /// Mağazadaki "Chrome'a ekle" düğmesine basıldı: uzantının mağaza kimliği.
    StoreInstall(String),
    /// Sayfadaki giriş formundan gelen mesaj: (sekme, belgenin adresi, JSON).
    Password(u32, String, String),
    /// İndirme başladı: (indirmeyi başlatan sekme, işlem).
    DownloadStarted(u32, ICoreWebView2DownloadOperation),
    DownloadChanged(u32),
    /// Gizli hesap görünümünde çalıştırılan betiğin sonucu.
    Script(u32, String),
    Extensions(Vec<ExtensionInfo>),
    ExtensionAdded(Option<String>),
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub struct CookieData {
    pub name: String,
    pub value: String,
    pub domain: String,
    pub path: String,
    pub http_only: bool,
    pub secure: bool,
    pub same_site: i32,
}

pub struct ExtensionInfo {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub ext: ICoreWebView2BrowserExtension,
}

/// Arka plan iş parçacıklarından gelen sonuçlar (indirilen avatar, uzantı paketi).
pub enum Background {
    Avatar(String, Option<Image>),
    ExtensionUnpacked(std::result::Result<std::path::PathBuf, String>),
    /// Hiç ziyaret edilmemiş sitenin simgesi (alan adı, PNG baytları).
    Favicon(String, Vec<u8>),
    /// Arama önerileri (istek sırası, öneriler).
    Suggestions(u64, Vec<String>),
    /// Reklam engelleyici motoru (diskten ya da yeni derlenmiş).
    Adblock(Option<Box<(crate::adblock::Engine, Option<crate::adblock::Engine>)>>, bool),
    /// Windows Hello sonucu (istek kimliği, sonuç).
    Hello(u32, crate::hello::Outcome),
}

static BACKGROUND: Mutex<Vec<Background>> = Mutex::new(Vec::new());
static TARGET_RAW: AtomicIsize = AtomicIsize::new(0);

/// Başka bir iş parçacığından sonuç bırakır ve ana pencereyi uyandırır.
pub fn push_background(b: Background) {
    if let Ok(mut q) = BACKGROUND.lock() {
        q.push(b);
    }
    let hwnd = HWND(TARGET_RAW.load(Ordering::Relaxed) as _);
    unsafe { let _ = PostMessageW(Some(hwnd), WM_ENGINE, WPARAM(0), LPARAM(0)); }
}

pub fn take_background() -> Vec<Background> {
    BACKGROUND.lock().map(|mut q| std::mem::take(&mut *q)).unwrap_or_default()
}

thread_local! {
    static QUEUE: RefCell<VecDeque<Event>> = RefCell::new(VecDeque::new());
    static TARGET: Cell<HWND> = Cell::new(HWND::default());
}

pub fn set_target(hwnd: HWND) {
    TARGET.with(|t| t.set(hwnd));
    TARGET_RAW.store(hwnd.0 as isize, Ordering::Relaxed);
}

pub fn push(e: Event) {
    let first = QUEUE.with(|q| {
        let mut q = q.borrow_mut();
        q.push_back(e);
        q.len() == 1
    });
    if first {
        let hwnd = TARGET.with(|t| t.get());
        unsafe { let _ = PostMessageW(Some(hwnd), WM_ENGINE, WPARAM(0), LPARAM(0)); }
    }
}

pub fn pop() -> Option<Event> {
    QUEUE.with(|q| q.borrow_mut().pop_front())
}

pub fn create_environment(user_data: &str, gpu: bool, shield: bool) -> Result<()> {
    let options = CoreWebView2EnvironmentOptions::default();
    unsafe {
        // Aynı siteyi paylaşan sekmeler aynı süreci kullansın.
        let mut args = String::from("--process-per-site");
        if !gpu {
            args.push_str(" --disable-gpu --disable-gpu-compositing");
        }
        if shield {
            // Üçüncü taraf çerezleri tamamen kapatılır (Chromium'un 3PC kapatma modu).
            // Sitenin kendi çerezleri, yani oturumlar, etkilenmez.
            args.push_str(" --test-third-party-cookie-phaseout --enable-features=TrackingProtection3pcd");
        }
        options.set_additional_browser_arguments(args);
        options.set_are_browser_extensions_enabled(true);
        // İnce, sayfanın üstünde süzülen modern kaydırma çubukları.
        options.set_scroll_bar_style(COREWEBVIEW2_SCROLLBAR_STYLE_FLUENT_OVERLAY);
    }
    let options: ICoreWebView2EnvironmentOptions = options.into();
    let handler = CreateCoreWebView2EnvironmentCompletedHandler::create(Box::new(|result, env| {
        push(Event::EnvReady(if result.is_ok() { env } else { None }));
        Ok(())
    }));
    unsafe {
        CreateCoreWebView2EnvironmentWithOptions(PCWSTR::null(), &HSTRING::from(user_data), &options, &handler)
    }
}

/// `profile`: WebView2 profil klasörü adı; `private`: gizli sekme (iz bırakmaz).
pub fn create_view(env: &ICoreWebView2Environment, parent: HWND, id: u32, profile: &str, private: bool) -> Result<()> {
    let handler = CreateCoreWebView2ControllerCompletedHandler::create(Box::new(move |result, ctrl| {
        push(Event::ViewReady(id, if result.is_ok() { ctrl } else { None }));
        Ok(())
    }));
    unsafe {
        let env10: ICoreWebView2Environment10 = env.cast()?;
        let options = env10.CreateCoreWebView2ControllerOptions()?;
        options.SetProfileName(&HSTRING::from(profile))?;
        options.SetIsInPrivateModeEnabled(private)?;
        env10.CreateCoreWebView2ControllerWithOptions(parent, &options, &handler)
    }
}

/// Yeni görünümün ayarları ve olay bağlantıları.
pub fn configure(ctrl: &ICoreWebView2Controller, id: u32, background: u32, prefs: &Settings) -> Result<ICoreWebView2> {
    unsafe {
        let view = ctrl.CoreWebView2()?;

        // Sayfa yüklenene kadar beyaz parlama olmasın.
        if let Ok(c2) = ctrl.cast::<ICoreWebView2Controller2>() {
            let _ = c2.SetDefaultBackgroundColor(COREWEBVIEW2_COLOR {
                A: 255,
                R: (background >> 16) as u8,
                G: (background >> 8) as u8,
                B: background as u8,
            });
        }

        let settings = view.Settings()?;
        settings.SetIsStatusBarEnabled(true)?;
        // Hatalarda motorun (Edge'in) sayfası değil, Sqywein'in kendi sayfası görünür.
        settings.SetIsBuiltInErrorPageEnabled(false)?;
        if let Ok(s4) = settings.cast::<ICoreWebView2Settings4>() {
            // Şifreleri motorun kendi deposu değil, Sqywein'in şifre yöneticisi saklar.
            let _ = s4.SetIsPasswordAutosaveEnabled(false);
            let _ = s4.SetIsGeneralAutofillEnabled(prefs.autofill);
        }
        apply_tracking(&view, if prefs.privacy_shield { "strict" } else { &prefs.tracking });
        // Sayfadaki sınıf/kimlik adlarını toplayıp genel gizleme kuralları için bildirir.
        let _ = view.AddScriptToExecuteOnDocumentCreated(
            &HSTRING::from(crate::adblock::COLLECTOR_SCRIPT),
            &AddScriptToExecuteOnDocumentCreatedCompletedHandler::create(Box::new(|_, _| Ok(()))),
        );
        // Giriş formlarını tanıyıp şifre yöneticisiyle doldurur.
        let _ = view.AddScriptToExecuteOnDocumentCreated(
            &HSTRING::from(include_str!("../ui/autofill.js")),
            &AddScriptToExecuteOnDocumentCreatedCompletedHandler::create(Box::new(|_, _| Ok(()))),
        );
        // Mağazadaki "Chrome'a ekle" düğmesi bizim kurulumumuza bağlanır.
        let _ = view.AddScriptToExecuteOnDocumentCreated(
            &HSTRING::from(STORE_SCRIPT),
            &AddScriptToExecuteOnDocumentCreatedCompletedHandler::create(Box::new(|_, _| Ok(()))),
        );
        set_download_folder(&view);
        if prefs.privacy_shield {
            // Global Privacy Control ve Do Not Track: sitelere "verimi satma/paylaşma" sinyali.
            let script = "try{Object.defineProperty(Navigator.prototype,'globalPrivacyControl',{get:()=>true,configurable:true});\
                Object.defineProperty(Navigator.prototype,'doNotTrack',{get:()=>'1',configurable:true});}catch(e){}";
            let _ = view.AddScriptToExecuteOnDocumentCreated(
                &HSTRING::from(script),
                &AddScriptToExecuteOnDocumentCreatedCompletedHandler::create(Box::new(|_, _| Ok(()))),
            );
        }

        // İç sayfalar (sqywein://…) bellekten sunulur.
        // Tüm istekler (iframe ve worker'lar dahil) engelleyiciden geçer; iç sayfalar
        // da aynı yoldan bellekten sunulur.
        match view.cast::<ICoreWebView2_22>() {
            Ok(v22) => v22.AddWebResourceRequestedFilterWithRequestSourceKinds(
                &HSTRING::from("*"),
                COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL,
                COREWEBVIEW2_WEB_RESOURCE_REQUEST_SOURCE_KINDS_ALL,
            )?,
            Err(_) => view.AddWebResourceRequestedFilter(&HSTRING::from("*"), COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL)?,
        }
        let env = view.cast::<ICoreWebView2_2>()?.Environment()?;
        let mut token = Default::default();
        view.add_WebResourceRequested(
            &WebResourceRequestedEventHandler::create(Box::new(move |_, args| {
                let Some(args) = args else { return Ok(()) };
                let request = args.Request()?;
                let mut uri = PWSTR::null();
                request.Uri(&mut uri)?;
                let uri = take_pwstr(uri);
                let (status, reason, body, mime) = if url::is_internal(&uri) {
                    match ui::file(&uri) {
                        Some((body, mime)) => (200, "OK", body, mime),
                        None => (404, "Not Found", &b"Bulunamadi"[..], "text/plain"),
                    }
                } else {
                    let mut context = COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL;
                    args.ResourceContext(&mut context)?;
                    let kind = crate::adblock::request_kind(context.0, uri == crate::adblock::page(id));
                    if !crate::adblock::should_block(id, &uri, kind) {
                        return Ok(()); // Dokunulmaz: istek normal yoluna devam eder.
                    }
                    (403, "Blocked", &b""[..], "text/plain")
                };
                let stream: Option<IStream> = SHCreateMemStream(Some(body));
                let headers = format!("Content-Type: {mime}; charset=utf-8\r\nCache-Control: no-store");
                let response = env.CreateWebResourceResponse(stream.as_ref(), status, &HSTRING::from(reason), &HSTRING::from(headers))?;
                args.SetResponse(&response)?;
                Ok(())
            })),
            &mut token,
        )?;
        // Yalnızca iç sayfaların mesajları dinlenir.
        view.add_WebMessageReceived(
            &WebMessageReceivedEventHandler::create(Box::new(move |_, args| {
                let Some(args) = args else { return Ok(()) };
                let mut source = PWSTR::null();
                args.Source(&mut source)?;
                let source = take_pwstr(source);
                let mut json = PWSTR::null();
                args.WebMessageAsJson(&mut json)?;
                let json = take_pwstr(json);
                if url::is_internal(&source) {
                    push(Event::WebMessage(id, json));
                    return Ok(());
                }
                let Ok(v) = serde_json::from_str::<serde_json::Value>(&json) else { return Ok(()) };
                if v["type"].as_str().is_some_and(|t| t.starts_with("pw.")) {
                    push(Event::Password(id, source, json));
                } else if v["type"] == "sq.cosmetic" {
                    // Herhangi bir sayfadan gelebilir: yalnızca sınırlı sayıda metin kabul edilir.
                    let names = |k: &str| -> Vec<String> {
                        v[k].as_array()
                            .map(|a| a.iter().take(2000).filter_map(|x| x.as_str().filter(|s| s.len() < 200).map(str::to_string)).collect())
                            .unwrap_or_default()
                    };
                    push(Event::Cosmetic(id, names("classes"), names("ids")));
                } else if source.starts_with(STORE_ORIGIN) {
                    // Mağazadan yalnızca "kur" isteği kabul edilir.
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&json) {
                        if v["type"] == "cws.install" {
                            if let Some(ext) = v["id"].as_str().filter(|s| s.len() == 32) {
                                push(Event::StoreInstall(ext.to_string()));
                            }
                        }
                    }
                }
                Ok(())
            })),
            &mut token,
        )?;
        if let Ok(v15) = view.cast::<ICoreWebView2_15>() {
            v15.add_FaviconChanged(
                &FaviconChangedEventHandler::create(Box::new(move |_, _| {
                    push(Event::FaviconChanged(id));
                    Ok(())
                })),
                &mut token,
            )?;
        }
        ctrl.add_ZoomFactorChanged(
            &ZoomFactorChangedEventHandler::create(Box::new(move |_, _| {
                push(Event::Zoom(id));
                Ok(())
            })),
            &mut token,
        )?;
        // İndirmeler kendi göstergemizde ve sayfamızda izlenir; motorun kutusu açılmaz.
        if let Ok(v4) = view.cast::<ICoreWebView2_4>() {
            v4.add_DownloadStarting(
                &DownloadStartingEventHandler::create(Box::new(move |_, args| {
                    let Some(args) = args else { return Ok(()) };
                    args.SetHandled(true)?;
                    push(Event::DownloadStarted(id, args.DownloadOperation()?));
                    Ok(())
                })),
                &mut token,
            )?;
        }

        view.add_NavigationStarting(
            &NavigationStartingEventHandler::create(Box::new(move |_, args| {
                let mut uri = PWSTR::null();
                if let Some(args) = args {
                    let _ = args.Uri(&mut uri);
                }
                let uri = take_pwstr(uri);
                // Engelleyici sayacı ve "bu sayfa" bilgisi istekler başlamadan güncellenir.
                crate::adblock::set_page(id, &uri);
                crate::log::line(&format!("GEZİNME #{id} {uri}"));
                push(Event::NavStarting(id, uri));
                Ok(())
            })),
            &mut token,
        )?;
        view.add_NavigationCompleted(
            &NavigationCompletedEventHandler::create(Box::new(move |_, args| {
                let (mut ok, mut status) = (BOOL::default(), COREWEBVIEW2_WEB_ERROR_STATUS_UNKNOWN);
                if let Some(args) = args {
                    let _ = args.IsSuccess(&mut ok);
                    let _ = args.WebErrorStatus(&mut status);
                }
                push(Event::NavCompleted(id, ok.as_bool(), status));
                Ok(())
            })),
            &mut token,
        )?;
        view.add_SourceChanged(
            &SourceChangedEventHandler::create(Box::new(move |_, _| {
                push(Event::SourceChanged(id));
                Ok(())
            })),
            &mut token,
        )?;
        view.add_DocumentTitleChanged(
            &DocumentTitleChangedEventHandler::create(Box::new(move |_, _| {
                push(Event::TitleChanged(id));
                Ok(())
            })),
            &mut token,
        )?;
        view.add_HistoryChanged(
            &HistoryChangedEventHandler::create(Box::new(move |_, _| {
                push(Event::HistoryChanged(id));
                Ok(())
            })),
            &mut token,
        )?;
        view.add_NewWindowRequested(
            &NewWindowRequestedEventHandler::create(Box::new(move |_, args| {
                if let Some(args) = args {
                    let mut uri = PWSTR::null();
                    args.Uri(&mut uri)?;
                    args.SetHandled(true)?;
                    push(Event::NewWindow(id, take_pwstr(uri)));
                }
                Ok(())
            })),
            &mut token,
        )?;
        view.add_ProcessFailed(
            &ProcessFailedEventHandler::create(Box::new(move |_, args| {
                let mut kind = COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED;
                if let Some(args) = args {
                    let _ = args.ProcessFailedKind(&mut kind);
                }
                push(Event::ProcessFailed(id, kind));
                Ok(())
            })),
            &mut token,
        )?;
        view.add_ContainsFullScreenElementChanged(
            &ContainsFullScreenElementChangedEventHandler::create(Box::new(move |_, _| {
                push(Event::FullScreen(id));
                Ok(())
            })),
            &mut token,
        )?;

        // Sayfa odaktayken de Sqywein kısayolları çalışsın.
        ctrl.add_AcceleratorKeyPressed(
            &AcceleratorKeyPressedEventHandler::create(Box::new(move |_, args| {
                let Some(args) = args else { return Ok(()) };
                let mut kind = COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN;
                args.KeyEventKind(&mut kind)?;
                if kind != COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN && kind != COREWEBVIEW2_KEY_EVENT_KIND_SYSTEM_KEY_DOWN {
                    return Ok(());
                }
                let mut vk = 0u32;
                args.VirtualKey(&mut vk)?;
                if crate::keys::command(vk, crate::keys::modifiers(), false).is_some() {
                    args.SetHandled(true)?;
                    push(Event::Shortcut(vk));
                }
                Ok(())
            })),
            &mut token,
        )?;

        Ok(view)
    }
}

pub fn string(f: impl FnOnce(&mut PWSTR) -> Result<()>) -> String {
    let mut p = PWSTR::null();
    match f(&mut p) {
        Ok(()) => take_pwstr(p),
        Err(_) => String::new(),
    }
}

pub fn flag(f: impl FnOnce(&mut BOOL) -> Result<()>) -> bool {
    let mut b = BOOL::default();
    f(&mut b).is_ok() && b.as_bool()
}

/// Arka plandaki sekme: düşük bellek hedefi ve dondurma.
pub fn doze(view: &ICoreWebView2) {
    unsafe {
        if let Ok(v19) = view.cast::<ICoreWebView2_19>() {
            let _ = v19.SetMemoryUsageTargetLevel(COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW);
        }
        if let Ok(v3) = view.cast::<ICoreWebView2_3>() {
            let _ = v3.TrySuspend(&TrySuspendCompletedHandler::create(Box::new(|_, _| Ok(()))));
        }
    }
}

pub fn wake(view: &ICoreWebView2) {
    unsafe {
        if let Ok(v3) = view.cast::<ICoreWebView2_3>() {
            let _ = v3.Resume();
        }
        if let Ok(v19) = view.cast::<ICoreWebView2_19>() {
            let _ = v19.SetMemoryUsageTargetLevel(COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL);
        }
    }
}

pub fn set_low_memory(view: &ICoreWebView2, low: bool) {
    unsafe {
        if let Ok(v19) = view.cast::<ICoreWebView2_19>() {
            let _ = v19.SetMemoryUsageTargetLevel(if low {
                COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW
            } else {
                COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL
            });
        }
    }
}

pub fn plays_audio(view: &ICoreWebView2) -> bool {
    view.cast::<ICoreWebView2_8>()
        .map(|v8| flag(|b| unsafe { v8.IsDocumentPlayingAudio(b) }))
        .unwrap_or(false)
}

#[allow(dead_code)] // RAM göstergesi geri gelince kullanılacak.
/// Motorun tüm süreçlerinin kimlikleri (tarayıcı, sayfa, GPU, yardımcılar).
pub fn process_ids(env: &ICoreWebView2Environment) -> Vec<u32> {
    let mut ids = Vec::new();
    unsafe {
        let Ok(env8) = env.cast::<ICoreWebView2Environment8>() else { return ids };
        let Ok(list) = env8.GetProcessInfos() else { return ids };
        let mut count = 0u32;
        if list.Count(&mut count).is_err() {
            return ids;
        }
        for i in 0..count {
            if let Ok(info) = list.GetValueAtIndex(i) {
                let mut pid = 0i32;
                if info.ProcessId(&mut pid).is_ok() {
                    ids.push(pid as u32);
                }
            }
        }
    }
    ids
}

/// Kullanıcıya gösterilecek hata; `None` ise ekran değişmez (ör. iptal edilen yükleme, indirme).
pub fn describe_error(status: COREWEBVIEW2_WEB_ERROR_STATUS) -> Option<&'static str> {
    Some(match status {
        COREWEBVIEW2_WEB_ERROR_STATUS_OPERATION_CANCELED
        | COREWEBVIEW2_WEB_ERROR_STATUS_CONNECTION_ABORTED
        | COREWEBVIEW2_WEB_ERROR_STATUS_VALID_AUTHENTICATION_CREDENTIALS_REQUIRED
        | COREWEBVIEW2_WEB_ERROR_STATUS_VALID_PROXY_AUTHENTICATION_REQUIRED => return None,
        COREWEBVIEW2_WEB_ERROR_STATUS_HOST_NAME_NOT_RESOLVED => "Bu adres bulunamadı. Yazımını kontrol et.",
        COREWEBVIEW2_WEB_ERROR_STATUS_DISCONNECTED => "İnternet bağlantısı yok. Bağlantını kontrol edip yeniden dene.",
        COREWEBVIEW2_WEB_ERROR_STATUS_TIMEOUT => "Site zamanında yanıt vermedi.",
        COREWEBVIEW2_WEB_ERROR_STATUS_CANNOT_CONNECT
        | COREWEBVIEW2_WEB_ERROR_STATUS_CONNECTION_RESET
        | COREWEBVIEW2_WEB_ERROR_STATUS_SERVER_UNREACHABLE => "Siteye bağlanılamadı. Site kapalı olabilir.",
        COREWEBVIEW2_WEB_ERROR_STATUS_CERTIFICATE_COMMON_NAME_IS_INCORRECT
        | COREWEBVIEW2_WEB_ERROR_STATUS_CERTIFICATE_EXPIRED
        | COREWEBVIEW2_WEB_ERROR_STATUS_CLIENT_CERTIFICATE_CONTAINS_ERRORS
        | COREWEBVIEW2_WEB_ERROR_STATUS_CERTIFICATE_REVOKED
        | COREWEBVIEW2_WEB_ERROR_STATUS_CERTIFICATE_IS_INVALID => {
            "Bu sitenin güvenlik sertifikası geçersiz, bu yüzden site açılmadı."
        }
        _ => "Sayfa yüklenirken bir sorun oluştu.",
    })
}

pub fn apply_tracking(view: &ICoreWebView2, level: &str) {
    let level = match level {
        "basic" => COREWEBVIEW2_TRACKING_PREVENTION_LEVEL_BASIC,
        "strict" => COREWEBVIEW2_TRACKING_PREVENTION_LEVEL_STRICT,
        _ => COREWEBVIEW2_TRACKING_PREVENTION_LEVEL_BALANCED,
    };
    if let Some(p3) = profile(view).and_then(|p| p.cast::<ICoreWebView2Profile3>().ok()) {
        unsafe { let _ = p3.SetPreferredTrackingPreventionLevel(level); }
    }
}

pub fn profile(view: &ICoreWebView2) -> Option<ICoreWebView2Profile> {
    let v13 = view.cast::<ICoreWebView2_13>().ok()?;
    unsafe { v13.Profile().ok() }
}

pub fn request_favicon(view: &ICoreWebView2, id: u32) {
    let Ok(v15) = view.cast::<ICoreWebView2_15>() else { return };
    let handler = GetFaviconCompletedHandler::create(Box::new(move |result, stream| {
        let bytes = match (result.is_ok(), stream) {
            (true, Some(s)) => read_stream(&s),
            _ => Vec::new(),
        };
        let img = crate::image::from_bytes(&bytes, 32);
        push(Event::Favicon(id, img, bytes));
        Ok(())
    }));
    unsafe { let _ = v15.GetFavicon(COREWEBVIEW2_FAVICON_IMAGE_FORMAT_PNG, &handler); }
}

pub fn post_json(view: &ICoreWebView2, json: &str) {
    unsafe { let _ = view.PostWebMessageAsJson(&HSTRING::from(json)); }
}

pub fn run_script(view: &ICoreWebView2, id: u32, script: &str) {
    let handler = ExecuteScriptCompletedHandler::create(Box::new(move |_, result| {
        push(Event::Script(id, result));
        Ok(())
    }));
    unsafe { let _ = view.ExecuteScript(&HSTRING::from(script), &handler); }
}

/// Tarama verilerini temizler: "history", "cookies", "cache".
pub fn clear_data(view: &ICoreWebView2, kinds: &[String]) {
    let mut k = COREWEBVIEW2_BROWSING_DATA_KINDS(0);
    for kind in kinds {
        k.0 |= match kind.as_str() {
            "history" => COREWEBVIEW2_BROWSING_DATA_KINDS_BROWSING_HISTORY.0 | COREWEBVIEW2_BROWSING_DATA_KINDS_DOWNLOAD_HISTORY.0,
            "cookies" => COREWEBVIEW2_BROWSING_DATA_KINDS_COOKIES.0 | COREWEBVIEW2_BROWSING_DATA_KINDS_ALL_DOM_STORAGE.0,
            "cache" => COREWEBVIEW2_BROWSING_DATA_KINDS_DISK_CACHE.0,
            _ => 0,
        };
    }
    if let Some(p2) = profile(view).and_then(|p| p.cast::<ICoreWebView2Profile2>().ok()) {
        let handler = ClearBrowsingDataCompletedHandler::create(Box::new(|_| Ok(())));
        unsafe { let _ = p2.ClearBrowsingData(k, &handler); }
    }
}

// ---------- Uzantılar ----------

pub fn list_extensions(view: &ICoreWebView2) {
    let Some(p7) = profile(view).and_then(|p| p.cast::<ICoreWebView2Profile7>().ok()) else {
        push(Event::Extensions(Vec::new()));
        return;
    };
    let handler = ProfileGetBrowserExtensionsCompletedHandler::create(Box::new(|result, list| {
        let mut out = Vec::new();
        if let (true, Some(list)) = (result.is_ok(), list) {
            let mut count = 0u32;
            unsafe {
                let _ = list.Count(&mut count);
                for i in 0..count {
                    let Ok(ext) = list.GetValueAtIndex(i) else { continue };
                    let id = string(|p| ext.Id(p));
                    let name = string(|p| ext.Name(p));
                    let enabled = flag(|b| ext.IsEnabled(b));
                    out.push(ExtensionInfo { id, name, enabled, ext });
                }
            }
        }
        push(Event::Extensions(out));
        Ok(())
    }));
    unsafe { let _ = p7.GetBrowserExtensions(&handler); }
}

pub fn add_extension(view: &ICoreWebView2, folder: &std::path::Path) {
    let Some(p7) = profile(view).and_then(|p| p.cast::<ICoreWebView2Profile7>().ok()) else {
        push(Event::ExtensionAdded(None));
        return;
    };
    let handler = ProfileAddBrowserExtensionCompletedHandler::create(Box::new(|result, ext| {
        let id = match (result.is_ok(), ext) {
            (true, Some(ext)) => Some(string(|p| unsafe { ext.Id(p) })),
            _ => None,
        };
        push(Event::ExtensionAdded(id));
        Ok(())
    }));
    unsafe { let _ = p7.AddBrowserExtension(&HSTRING::from(folder.as_os_str()), &handler); }
}

pub fn enable_extension(ext: &ICoreWebView2BrowserExtension, on: bool) {
    let handler = BrowserExtensionEnableCompletedHandler::create(Box::new(|_| Ok(())));
    unsafe { let _ = ext.Enable(on, &handler); }
}

pub fn remove_extension(ext: &ICoreWebView2BrowserExtension) {
    let handler = BrowserExtensionRemoveCompletedHandler::create(Box::new(|_| Ok(())));
    unsafe { let _ = ext.Remove(&handler); }
}

// ---------- İndirmeler ----------

pub fn watch_download(op: &ICoreWebView2DownloadOperation, id: u32) {
    let mut token = Default::default();
    unsafe {
        let _ = op.add_BytesReceivedChanged(
            &BytesReceivedChangedEventHandler::create(Box::new(move |_, _| {
                push(Event::DownloadChanged(id));
                Ok(())
            })),
            &mut token,
        );
        let _ = op.add_StateChanged(
            &StateChangedEventHandler::create(Box::new(move |_, _| {
                push(Event::DownloadChanged(id));
                Ok(())
            })),
            &mut token,
        );
    }
}

/// (durum, alınan, toplam)
pub fn download_progress(op: &ICoreWebView2DownloadOperation) -> (&'static str, u64, u64) {
    unsafe {
        let mut state = COREWEBVIEW2_DOWNLOAD_STATE_IN_PROGRESS;
        let _ = op.State(&mut state);
        let (mut got, mut total) = (0i64, 0i64);
        let _ = op.BytesReceived(&mut got);
        let _ = op.TotalBytesToReceive(&mut total);
        let status = match state {
            COREWEBVIEW2_DOWNLOAD_STATE_COMPLETED => "done",
            COREWEBVIEW2_DOWNLOAD_STATE_INTERRUPTED => {
                let mut reason = COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_NONE;
                let _ = op.InterruptReason(&mut reason);
                // Duraklatılan indirme "kesintili ama sürdürülebilir" görünür.
                if reason == COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_USER_CANCELED {
                    "canceled"
                } else if flag(|b| op.CanResume(b)) {
                    "paused"
                } else {
                    "failed"
                }
            }
            _ => "progress",
        };
        (status, got.max(0) as u64, total.max(0) as u64)
    }
}

fn read_stream(stream: &IStream) -> Vec<u8> {
    let mut out = Vec::new();
    let mut buf = [0u8; 8192];
    loop {
        let mut read = 0u32;
        let hr = unsafe { stream.Read(buf.as_mut_ptr() as _, buf.len() as u32, Some(&mut read)) };
        if hr.is_err() || read == 0 || out.len() > 512 * 1024 {
            break;
        }
        out.extend_from_slice(&buf[..read as usize]);
    }
    out
}

// ---------- Çerezler ----------

/// Oturum çerezlerini (süresiz olanları) toplar; sonuç `Event::Cookies` ile gelir.
pub fn snapshot_session_cookies(view: &ICoreWebView2) {
    let Ok(manager) = view.cast::<ICoreWebView2_2>().and_then(|v| unsafe { v.CookieManager() }) else { return };
    let handler = GetCookiesCompletedHandler::create(Box::new(|result, list| {
        let mut out = Vec::new();
        if let (true, Some(list)) = (result.is_ok(), list) {
            let mut count = 0u32;
            unsafe {
                let _ = list.Count(&mut count);
                for i in 0..count {
                    let Ok(c) = list.GetValueAtIndex(i) else { continue };
                    if !flag(|b| c.IsSession(b)) {
                        continue; // Süreli çerezleri motor zaten diske yazıyor.
                    }
                    let mut same = COREWEBVIEW2_COOKIE_SAME_SITE_KIND_LAX;
                    let _ = c.SameSite(&mut same);
                    out.push(CookieData {
                        name: string(|p| c.Name(p)),
                        value: string(|p| c.Value(p)),
                        domain: string(|p| c.Domain(p)),
                        path: string(|p| c.Path(p)),
                        http_only: flag(|b| c.IsHttpOnly(b)),
                        secure: flag(|b| c.IsSecure(b)),
                        same_site: same.0,
                    });
                }
            }
        }
        push(Event::Cookies(out));
        Ok(())
    }));
    unsafe { let _ = manager.GetCookies(&HSTRING::new(), &handler); }
}

/// Kayıtlı oturum çerezlerini geri koyar (sayfa açılmadan önce).
pub fn restore_cookies(view: &ICoreWebView2, cookies: &[CookieData]) {
    let Ok(manager) = view.cast::<ICoreWebView2_2>().and_then(|v| unsafe { v.CookieManager() }) else { return };
    for c in cookies {
        unsafe {
            let Ok(cookie) = manager.CreateCookie(&HSTRING::from(&c.name), &HSTRING::from(&c.value), &HSTRING::from(&c.domain), &HSTRING::from(&c.path)) else {
                continue;
            };
            let _ = cookie.SetIsHttpOnly(c.http_only);
            let _ = cookie.SetIsSecure(c.secure);
            let _ = cookie.SetSameSite(COREWEBVIEW2_COOKIE_SAME_SITE_KIND(c.same_site));
            let _ = manager.AddOrUpdateCookie(&cookie);
        }
    }
}

const STORE_ORIGIN: &str = "https://chromewebstore.google.com/";

/// Chrome Web Mağazası'nda "Chrome'a ekle" düğmesini "Sqywein'e ekle" yapar ve
/// tıklanınca kurulumu Sqywein'e yaptırır. Mağazanın kendi kurulumu WebView2'de
/// çalışmadığı için ("Download interrupted") bu gerekli.
const STORE_SCRIPT: &str = r#"(() => {
  if (location.hostname !== 'chromewebstore.google.com' || !window.chrome || !window.chrome.webview) return;
  const ADD = /chrome['’]?a ekle|add to chrome|sqywein['’]?e ekle/i;
  const id = () => { const m = location.pathname.match(/\/detail\/(?:[^/]+\/)?([a-p]{32})/); return m && m[1]; };
  document.addEventListener('click', e => {
    const b = e.target.closest && e.target.closest('button');
    if (!b || !ADD.test(b.textContent || '') || !id()) return;
    e.preventDefault();
    e.stopImmediatePropagation();
    window.chrome.webview.postMessage({ type: 'cws.install', id: id() });
  }, true);
  const relabel = () => {
    for (const n of document.querySelectorAll('button, button *')) {
      if (n.childElementCount === 0 && /chrome['’]?a ekle|add to chrome/i.test(n.textContent || '')) n.textContent = "Sqywein'e ekle";
    }
  };
  new MutationObserver(relabel).observe(document, { subtree: true, childList: true });
})();"#;

/// İndirmeler kullanıcının "İndirilenler" klasörüne gider.
fn set_download_folder(view: &ICoreWebView2) {
    use windows::Win32::UI::Shell::{FOLDERID_Downloads, SHGetKnownFolderPath, KF_FLAG_DEFAULT};
    let Some(p) = profile(view) else { return };
    unsafe {
        let Ok(path) = SHGetKnownFolderPath(&FOLDERID_Downloads, KF_FLAG_DEFAULT, None) else { return };
        let folder = take_pwstr(path);
        if !folder.is_empty() {
            let _ = p.SetDefaultDownloadFolderPath(&HSTRING::from(folder));
        }
    }
}

/// Belge oluşurken çalışacak betik ekler; kimliği `Event::ScriptAdded` ile gelir.
pub fn add_document_script(view: &ICoreWebView2, tab: u32, slot: u8, script: &str) {
    let handler = AddScriptToExecuteOnDocumentCreatedCompletedHandler::create(Box::new(move |result, id| {
        if result.is_ok() {
            push(Event::ScriptAdded(tab, slot, id));
        }
        Ok(())
    }));
    unsafe { let _ = view.AddScriptToExecuteOnDocumentCreated(&HSTRING::from(script), &handler); }
}

pub fn remove_document_script(view: &ICoreWebView2, id: &str) {
    unsafe { let _ = view.RemoveScriptToExecuteOnDocumentCreated(&HSTRING::from(id)); }
}

pub fn execute(view: &ICoreWebView2, script: &str) {
    let handler = ExecuteScriptCompletedHandler::create(Box::new(|_, _| Ok(())));
    unsafe { let _ = view.ExecuteScript(&HSTRING::from(script), &handler); }
}
