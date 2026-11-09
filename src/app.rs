//! Ana pencere: sekmeler, adres çubuğu, çizim, girdi ve bellek yönetimi.
//!
//! Alt dosyalar: `pages` (iç sayfaların mesajları ve indirmeler), `account`
//! (Google hesabı ve profiller), `extensions` (uzantılar ve mağaza).

mod account;
mod extensions;
mod newtab;
mod pages;
mod session;
mod shield;
mod suggest;
mod tabs;
mod updates;
mod vault;
mod integration_ui;
mod translate;
mod extras;
mod overlay;

pub use updates::wait_for_previous;

use crate::{
    engine::{self, Event, ExtensionInfo, WM_ENGINE},
    gfx::{Align, Font, Gfx, Rect},
    history::History,
    image::Image,
    keys::{self, Cmd},
    menu::{self, Item, Menu, MenuCmd, MenuKind},
    state,
    theme::*,
    url,
};
use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
    rc::Rc,
    time::{Duration, Instant},
};
use webview2_com::Microsoft::Web::WebView2::Win32::*;
use windows::{
    core::*,
    Win32::{
        Foundation::*,
        Graphics::{Dwm::*, Gdi::*},
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            Controls::{EM_SETCUEBANNER, EM_SETMARGINS, EM_SETSEL, MARGINS, WM_MOUSELEAVE},
            HiDpi::*,
            Input::KeyboardAndMouse::*,
            WindowsAndMessaging::*,
        },
    },
};

/// Arka plandaki sekme bu süre sonunda düşük bellek hedefine alınır ve dondurulur.
const DOZE_AFTER: Duration = Duration::from_secs(30);
/// "Sekmeleri kapat" ayarı açıksa, bu süre sonunda görünüm tamamen kapatılır.
const SLEEP_AFTER: Duration = Duration::from_secs(5 * 60);

const TIMER_HOUSEKEEPING: usize = 1;
const TIMER_WAKE_ALL: usize = 2;
const TIMER_ANIM: usize = 3;
const EDIT_ID: i32 = 100;
const START_ID: i32 = 101;

/// Sekme olmayan, gizli görünümlerin kimlikleri.
const ACCOUNT_VIEW: u32 = u32::MAX;
const POPUP_VIEW: u32 = u32::MAX - 1;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mem {
    Asleep,
    Dozing,
    Suspending,
    Awake,
}

struct Tab {
    id: u32,
    url: String,
    title: String,
    loading: bool,
    /// Yüklemenin başladığı an (adres alanındaki ilerleme çizgisi için).
    load_started: Option<Instant>,
    error: Option<String>,
    ctrl: Option<ICoreWebView2Controller>,
    view: Option<ICoreWebView2>,
    creating: bool,
    /// Görünüm hazır olunca `url` yüklenmeli mi?
    needs_load: bool,
    mem: Mem,
    last_active: Instant,
    epoch: u64,
    view_epoch: u64,
    sleep_check: bool,
    sleep_discard: bool,
    next_rest_check: Instant,
    opener: Option<u32>,
    window_request: Option<Box<engine::WindowRequest>>,
    window_child: bool,
    can_back: bool,
    can_forward: bool,
    /// Sitenin gerçek simgesi.
    icon: Option<Rc<Image>>,
    /// Gizli sekme: geçmişe ve oturuma yazılmaz, çerezleri kapanınca silinir.
    private: bool,
    pinned: bool,
    muted: bool,
    /// Engelleyicinin belge betikleri (sayfa gizleme, YouTube) ve genel kural istisnaları.
    ad_scripts: [Option<String>; 2],
    ad_exceptions: Option<(bool, HashSet<String>)>,
    /// En az bir sayfa yüklendi mi? (Yalnızca indirme için açılan sekmeleri tanımak için.)
    has_loaded: bool,
    /// Sayfa Türkçe değilse dili ("?" = bilinmiyor); çeviri düğmesi bunu gösterir.
    page_lang: Option<String>,
    /// Sayfa şu an çevrilmiş mi, kaç çeviri isteği yolda.
    translated: bool,
    tr_pending: u32,
    tr_generation: u64,
    tr_error: bool,
    tr_collecting: bool,
    tr_limited: bool,
    /// Kullanıcı bu sayfada çeviriyi kapattı: aynı sayfada kendiliğinden yeniden çevrilmez.
    tr_declined: bool,
    tr_cancel: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    external_launch: Option<Instant>,
    /// Uygulanan yakınlaştırma ve hangi site için uygulandığı.
    zoom: f64,
    zoom_host: String,
    /// Açılış animasyonunun başladığı an.
    born: Instant,
    /// Ekranda çizilen konum ve genişlik; hedefe doğru yumuşakça kayar. NaN = hedefte.
    x: f32,
    w: f32,
}

impl Tab {
    fn new(id: u32, url: String, title: String) -> Self {
        Self {
            id,
            needs_load: !url.is_empty(),
            url,
            title,
            loading: false,
            load_started: None,
            error: None,
            ctrl: None,
            view: None,
            creating: false,
            mem: Mem::Asleep,
            last_active: Instant::now(),
            epoch: 0,
            page_lang: None,
            translated: false,
            tr_pending: 0,
            tr_generation: 0,
            tr_error: false,
            tr_collecting: false,
            tr_limited: false,
            tr_declined: false,
            tr_cancel: None,
            view_epoch: 0,
            sleep_check: false,
            sleep_discard: false,
            next_rest_check: Instant::now(),
            opener: None,
            window_request: None,
            window_child: false,
            can_back: false,
            can_forward: false,
            icon: None,
            private: false,
            pinned: false,
            muted: false,
            has_loaded: false,
            external_launch: None,
            ad_scripts: [None, None],
            ad_exceptions: None,
            zoom: 1.0,
            zoom_host: String::new(),
            born: Instant::now(),
            x: f32::NAN,
            w: f32::NAN,
        }
    }

    fn label(&self) -> String {
        if !self.title.is_empty() {
            self.title.clone()
        } else if url::is_internal(&self.url) {
            match url::display(&self.url).trim_start_matches("sqywein://") {
                "history" => "Geçmiş",
                "downloads" => "İndirmeler",
                "bookmarks" => "Yer işaretleri",
                "settings" => "Ayarlar",
                "extensions" => "Uzantılar",
                "passwords" => "Şifreler",
                _ => "Sqywein",
            }
            .into()
        } else if !self.url.is_empty() {
            url::host(&self.url).to_string()
        } else if self.private {
            "Gizli sekme".into()
        } else {
            "Yeni sekme".into()
        }
    }

    fn close_view(&mut self) {
        if let Some(cancel) = self.tr_cancel.take() { cancel.store(true, std::sync::atomic::Ordering::Relaxed); }
        self.window_request = None;
        self.epoch += 1;
        self.view_epoch += 1;
        self.sleep_check = false;
        self.ad_scripts = [None, None];
        self.ad_exceptions = None;
        self.can_back = false;
        self.can_forward = false;
        if let Some(ctrl) = self.ctrl.take() {
            unsafe { let _ = ctrl.Close(); }
        }
        self.view = None;
        self.creating = false;
        self.mem = Mem::Asleep;
        self.loading = false;
        self.zoom_host.clear();
        if !self.url.is_empty() {
            self.needs_load = true;
        }
    }

    fn rest_action(&self, pressure: crate::memory::Pressure, freeze: bool, sleep: bool, finished_store: bool, idle: Duration) -> Option<bool> {
        let (doze_secs, automatic_discard) = pressure.delays();
        let automatic = freeze && !self.private && !self.pinned && !self.can_back && !self.can_forward
            && automatic_discard.is_some_and(|s| idle.as_secs() >= s);
        if (sleep && idle >= SLEEP_AFTER) || (finished_store && idle >= DOZE_AFTER) || automatic { Some(true) }
        else if freeze && self.mem == Mem::Awake && idle.as_secs() >= doze_secs { Some(false) }
        else { None }
    }

    fn open_progress(&self) -> f32 {
        if !motion_enabled() { return 1.0; }
        ease_out(self.born.elapsed().as_secs_f32() * 1000.0 / TAB_OPEN_MS)
    }
}

/// Kapanan sekmenin söner gibi küçülen izi.
struct Ghost {
    rect: Rect,
    label: String,
    icon: Option<Rc<Image>>,
    active: bool,
    start: Instant,
}

impl Ghost {
    fn progress(&self) -> f32 {
        if !motion_enabled() { return 1.0; }
        self.start.elapsed().as_secs_f32() * 1000.0 / TAB_CLOSE_MS
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Hit {
    None,
    Tab(usize),
    TabClose(usize),
    NewTab,
    Min,
    Max,
    Close,
    Back,
    Forward,
    Reload,
    Address,
    Translate,
    Share,
    Star,
    Update,
    Shield,
    ZoomPill,
    AddExtension,
    Downloads,
    Pinned(usize),
    Puzzle,
    Profile,
    Menu,
    Retry,
    BarItem(usize),
    BarMore,
    Tile(usize),
    TileRemove(usize),
    TileAdd,
    Home,
    Weather,
    /// Gizli ↔ normal sekmeler arasında geçiş.
    Mode,
}

struct Layout {
    tabs: Vec<Rect>,
    new_tab: Rect,
    min: Rect,
    max: Rect,
    close: Rect,
    back: Rect,
    forward: Rect,
    home: Rect,
    reload: Rect,
    address: Rect,
    /// Adres çubuğunda yazının sığacağı alan.
    address_text: Rect,
    star: Rect,
    translate: Option<Rect>,
    shield: Option<Rect>,
    zoom_pill: Option<Rect>,
    add_ext: Option<Rect>,
    downloads: Option<Rect>,
    update: Option<Rect>,
    pinned: Vec<Rect>,
    /// Gizli/normal geçiş düğmesi (gizli sekme varken).
    mode: Option<Rect>,
    puzzle: Rect,
    share: Rect,
    profile: Rect,
    menu: Rect,
    content: Rect,
    default_banner: Option<Rect>,
    retry: Rect,
    /// Yer işaretleri çubuğu, öğeleri ve sığmayanlar için "»".
    bar: Option<Rect>,
    bar_items: Vec<Rect>,
    bar_more: Option<Rect>,
}

struct FullScreen {
    placement: WINDOWPLACEMENT,
}

/// Uzantı simgesine tıklanınca açılan küçük pencere.
struct Popup {
    hwnd: HWND,
    ctrl: Option<ICoreWebView2Controller>,
    url: String,
    gfx: Option<Gfx>,
    glass: bool,
}

type FileIconCache = RefCell<crate::cache::Cache<String, Option<(Rc<Image>, String)>>>;

pub struct App {
    default_prompt: Option<integration_ui::DefaultPrompt>,
    hwnd: HWND,
    edit: HWND,
    start_edit: HWND,
    edit_font: HFONT,
    start_font: HFONT,
    gfx: Gfx,
    dpi: u32,
    width: u32,
    height: u32,
    tabs: Vec<Tab>,
    ghosts: Vec<Ghost>,
    active: usize,
    next_id: u32,
    env: Option<ICoreWebView2Environment>,
    env_pending: bool,
    /// Motorun ana sürümü (mağazadan uzantı indirirken gerekir).
    browser_major: u32,
    closed: Vec<String>,
    hover: Hit,
    previous_hover: Hit,
    hover_progress: f32,
    selection_x: f32,
    selection_w: f32,
    animation_interval: u32,
    pressed: Hit,
    tracking_mouse: bool,
    /// Odak modu: araç çubuğu gizli; fare üste gelince (`focus_reveal`) görünür.
    focus_mode: bool,
    focus_reveal: bool,
    /// Windows paylaşım penceresine bağlı olay.
    share_token: Option<i64>,
    state: state::AppState,
    history: History,
    dirty: bool,
    /// Komut satırında `--no-rest` verilmediyse arka plan sekmeleri dondurulur.
    rest: bool,
    wake_queue: Vec<u32>,
    full_screen: Option<FullScreen>,
    menu: Option<Menu>,
    retiring_menus: Vec<Menu>,
    /// Menü düğmesine tıklanınca menü önce kapanır; hemen yeniden açılmasın.
    menu_closed: Option<(MenuKind, Instant)>,
    anim_running: bool,
    last_tick: Instant,
    t0: Instant,
    // Hesap ve profil
    avatar: Option<Rc<Image>>,
    account_view: Option<ICoreWebView2Controller>,
    account_checked: Option<Instant>,
    // Uzantılar
    extensions: Vec<ExtensionInfo>,
    ext_icons: HashMap<String, Rc<Image>>,
    installing: bool,
    adblock_loading: bool,
    pending_update_enabled: Option<bool>,
    /// Kurulmakta olan uzantı: (mağaza kimliği ya da "", klasör).
    pending_install: Option<(String, std::path::PathBuf)>,
    popup: Option<Popup>,
    surface: Option<overlay::Surface>,
    overlay_generation: u64,
    thumbnail_queue: std::collections::VecDeque<u32>,
    thumbnail_inflight: Option<u32>,
    preview_cancel: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    ocr_request: u64,
    auth_popups: HashMap<u32, Popup>,
    retired_auth: Vec<(Instant, Popup)>,
    // İndirmeler
    downloads: Vec<(u32, ICoreWebView2DownloadOperation)>,
    next_download: u32,
    show_downloads: bool,
    /// İndirme hızı: (son bayt, son an, bayt/sn).
    dl_speed: HashMap<u32, (u64, Instant, f64)>,
    /// İndirme bitince düğmenin yumuşakça parlaması.
    download_pulse: Option<Instant>,
    // Kalıcılık
    cookies_restored: bool,
    cookie_epochs: HashMap<String, u64>,
    cookie_hashes: crate::cache::Cache<String, [u8; 32]>,
    clearing_data: HashSet<String>,
    closing: bool,
    housekeeping_ticks: u32,
    memory_pressure: crate::memory::Pressure,
    icon_cache: RefCell<crate::cache::Cache<String, Option<Rc<Image>>>>,
    /// Dosya türü simgeleri (uzantı ya da .exe için dosya yolu) ve sayfalar için PNG hali.
    file_icons: FileIconCache,
    icon_fetching: RefCell<HashSet<String>>,
    // Ana sayfa ve öneriler
    logo: Option<Rc<Image>>,
    dialog: Option<newtab::Dialog>,
    suggest: Option<suggest::Suggest>,
    suggest_gen: u64,
    suggest_busy: bool,
    // Şifre yöneticisi
    vault: crate::passwords::Vault,
    pending_login: Option<vault::PendingLogin>,
    hello_until: Option<Instant>,
    hello_pending: Vec<(u32, vault::Gate)>,
    hello_request: u32,
    favicon_urls: RefCell<crate::cache::Cache<String, String>>,
    // Sekme davranışları
    context_tab: u32,
    menu_anchor: Option<POINT>,
    drag: Option<tabs::Drag>,
    /// Bırakılan sekme: yerine hızlıca oturana kadar.
    drop_snap: Option<u32>,
    /// Başlangıç sayfası kartlarının verisi ve son yenileme zamanı.
    weather: Option<crate::widgets::WeatherState>,
    rates: Option<crate::widgets::Rates>,
    /// Yolda olan istekler ve başarısızlıktan sonra yeniden deneme zamanı (Unix sn).
    /// Önizlemenin sekmeye büyüme animasyonu.
    expand: Option<extensions::Expand>,
    weather_busy: bool,
    rates_busy: bool,
    weather_retry: u64,
    rates_retry: u64,
    tooltip: HWND,
    tooltip_text: HSTRING,
    /// İndirme → başlatan sekme; indirmesi süren kapalı sekmelerin görünümleri.
    download_owner: HashMap<u32, u32>,
    lingering: Vec<(u32, ICoreWebView2Controller)>,
    ram_line: String,
    asleep_count: usize,
    // Güncelleme
    update_ready: Option<(String, std::path::PathBuf)>,
    update_status: String,
    update_busy: bool,
    restart_with: Option<std::path::PathBuf>,
    mascot: Option<Rc<Image>>,
}

// ---------- Uygulama durumuna erişim ----------

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
    static DEFERRED: RefCell<Vec<Box<dyn FnOnce()>>> = RefCell::new(Vec::new());
    static BRUSHES: RefCell<Option<(HBRUSH, HBRUSH)>> = const { RefCell::new(None) };
    static SUPPRESS_CHANGE: Cell<bool> = const { Cell::new(false) };
}

/// Uygulama başka bir çağrıda meşgulse `None` döner (iç içe pencere mesajları).
fn with_app<R>(f: impl FnOnce(&mut App) -> R) -> Option<R> {
    APP.with(|a| match a.try_borrow_mut() {
        Ok(mut guard) => guard.as_mut().map(f),
        Err(_) => None,
    })
}

/// Pencereye geri mesaj gönderen işlemler (boyut, odak, gösterme) durum bırakıldıktan sonra çalışır.
fn defer(f: impl FnOnce() + 'static) {
    DEFERRED.with(|d| d.borrow_mut().push(Box::new(f)));
}

fn run_deferred() {
    loop {
        let jobs = DEFERRED.with(|d| std::mem::take(&mut *d.borrow_mut()));
        if jobs.is_empty() {
            break;
        }
        for job in jobs {
            job();
        }
    }
}

fn wide(s: &str) -> HSTRING {
    HSTRING::from(s)
}

fn lerp_toward(cur: f32, target: f32, k: f32) -> f32 {
    if cur.is_nan() || (target - cur).abs() < 0.25 {
        target
    } else {
        cur + (target - cur) * k
    }
}

// ---------- Başlatma ve mesaj döngüsü ----------

unsafe fn create_edit(parent: HWND, instance: HINSTANCE, id: i32, cue: &str) -> Result<HWND> {
    // Katmanlı çocuk pencere: GDI'nin yazısı camın üstünde de opak görünür.
    let edit = CreateWindowExW(
        WS_EX_LAYERED,
        w!("EDIT"),
        w!(""),
        WS_CHILD | WS_VISIBLE | WINDOW_STYLE(ES_AUTOHSCROLL as u32),
        0,
        0,
        10,
        10,
        Some(parent),
        Some(HMENU(id as _)),
        Some(instance),
        None,
    )?;
    let _ = SetLayeredWindowAttributes(edit, COLORREF(0), 255, LWA_ALPHA);
    // Çift arabellekli çizim: yazarken zemin silinip yeniden yazılırken oluşan titreme yok.
    let _ = windows::Win32::UI::Shell::SetWindowSubclass(edit, Some(edit_subclass), 1, 0);
    let cue = wide(cue);
    SendMessageW(edit, EM_SETCUEBANNER, Some(WPARAM(1)), Some(LPARAM(cue.as_ptr() as isize)));
    Ok(edit)
}

/// Metin kutusunu önce bellekteki bir bitmap'e, sonra tek seferde ekrana çizer.
unsafe extern "system" fn edit_subclass(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM, _id: usize, _data: usize) -> LRESULT {
    use windows::Win32::UI::Shell::DefSubclassProc;
    match msg {
        WM_ERASEBKGND => LRESULT(1),
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            let mut rc = RECT::default();
            let _ = GetClientRect(hwnd, &mut rc);
            let (w, h) = (rc.right - rc.left, rc.bottom - rc.top);
            if w > 0 && h > 0 {
                let mem = CreateCompatibleDC(Some(hdc));
                let bmp = CreateCompatibleBitmap(hdc, w, h);
                let old = SelectObject(mem, bmp.into());
                // Zemin rengi ve yazı rengi üst pencereden (WM_CTLCOLOREDIT) gelir.
                let parent = GetParent(hwnd).unwrap_or_default();
                let brush = HBRUSH(SendMessageW(parent, WM_CTLCOLOREDIT, Some(WPARAM(mem.0 as usize)), Some(LPARAM(hwnd.0 as isize))).0 as _);
                FillRect(mem, &rc, brush);
                let _ = HideCaret(Some(hwnd));
                SendMessageW(hwnd, WM_PRINTCLIENT, Some(WPARAM(mem.0 as usize)), Some(LPARAM(PRF_CLIENT as isize)));
                let _ = BitBlt(hdc, 0, 0, w, h, Some(mem), 0, 0, SRCCOPY);
                let _ = ShowCaret(Some(hwnd));
                SelectObject(mem, old);
                let _ = DeleteObject(bmp.into());
                let _ = DeleteDC(mem);
            }
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        _ => DefSubclassProc(hwnd, msg, wparam, lparam),
    }
}

unsafe fn register_class(instance: HINSTANCE, name: PCWSTR, proc: WNDPROC, style: WNDCLASS_STYLES, icon: HICON) -> Result<()> {
    let wc = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        style,
        lpfnWndProc: proc,
        hInstance: instance,
        hIcon: icon,
        hCursor: LoadCursorW(None, IDC_ARROW)?,
        lpszClassName: name,
        ..Default::default()
    };
    RegisterClassExW(&wc);
    Ok(())
}

pub fn run() -> Result<()> {
    unsafe {
        let instance: HINSTANCE = GetModuleHandleW(None)?.into();
        let class = w!("SqyweinWindow");
        let icon = LoadIconW(Some(instance), PCWSTR(1 as _)).unwrap_or_default();
        register_class(instance, class, Some(wndproc), CS_HREDRAW | CS_VREDRAW, icon)?;
        register_class(instance, menu::CLASS, Some(menu_wndproc), CS_DROPSHADOW, HICON::default())?;
        register_class(instance, extensions::POPUP_CLASS, Some(popup_wndproc), CS_DROPSHADOW, HICON::default())?;
        register_class(instance, newtab::DIALOG_CLASS, Some(dialog_wndproc), CS_DROPSHADOW, HICON::default())?;
        register_class(instance, suggest::SUGGEST_CLASS, Some(suggest_wndproc), CS_DROPSHADOW, HICON::default())?;

        let dpi = GetDpiForSystem();
        let scale = dpi as f32 / 96.0;
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            class,
            w!("Sqywein"),
            // WS_CAPTION ve WS_SYSMENU yok: cam açıkken Windows kendi başlık düğmelerini
            // çizmez; onları araç çubuğuyla aynı hizada biz çizeriz.
            (WS_OVERLAPPEDWINDOW & !(WS_CAPTION | WS_SYSMENU)) | WS_CLIPCHILDREN,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            (1280.0 * scale) as i32,
            (820.0 * scale) as i32,
            None,
            None,
            Some(instance),
            None,
        )?;
        engine::set_target(hwnd);

        // Koyu çerçeve, yuvarlak köşe ve gölge.
        let dark: i32 = 1;
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE, &dark as *const _ as _, 4);
        let corner = DWMWCP_ROUND;
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE, &corner as *const _ as _, 4);
        let _ = DwmExtendFrameIntoClientArea(hwnd, &MARGINS { cxLeftWidth: 0, cxRightWidth: 0, cyTopHeight: 1, cyBottomHeight: 0 });

        let edit = create_edit(hwnd, instance, EDIT_ID, "Ara veya adres yaz")?;
        let start_edit = create_edit(hwnd, instance, START_ID, "Ara ya da bir adres yaz")?;

        let app = App::new(hwnd, edit, start_edit)?;
        APP.with(|a| *a.borrow_mut() = Some(app));

        // Çerçeve hesabı uygulama hazır olduktan sonra yeniden yapılsın.
        let _ = SetWindowPos(hwnd, None, 0, 0, 0, 0, SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
        // Pencere son bırakıldığı yerde ve boyutta açılır. Bu, uygulama meşgul değilken
        // yapılır; yoksa boyut mesajı kaçar ve arayüz eski boyutta gerilmiş çizilir.
        match with_app(|app| app.saved_placement()).flatten() {
            Some(placement) => {
                let _ = SetWindowPlacement(hwnd, &placement);
            }
            None => {
                let _ = ShowWindow(hwnd, SW_SHOWDEFAULT);
            }
        }
        // Her ihtimale karşı boyut, pencerenin gerçek boyutuyla eşitlenir.
        with_app(|app| app.sync_size());
        with_app(|app| {
            app.create_tooltip();
            app.start();
        });
        run_deferred();

        let mut msg = MSG::default();
        crate::integration::mark_window(hwnd)?;
        let mut edit_was_focused = false;
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            if msg.message == WM_KEYDOWN || msg.message == WM_SYSKEYDOWN {
                let in_edit = msg.hwnd == edit || msg.hwnd == start_edit;
                let vk = msg.wParam.0 as u32;
                let target = msg.hwnd;
                let consumed = with_app(|app| app.default_prompt_key(target, vk) || app.dialog_key(target, vk) || (in_edit && app.suggest_key(target, vk))).unwrap_or(false);
                if consumed {
                    run_deferred();
                    continue;
                }
                let mods = keys::modifiers();
                if in_edit && mods.ctrl && vk == 'A' as u32 {
                    SendMessageW(msg.hwnd, EM_SETSEL, Some(WPARAM(0)), Some(LPARAM(-1)));
                    continue;
                }
                if let Some(cmd) = keys::command(vk, mods, in_edit) {
                    with_app(|app| app.run(cmd));
                    run_deferred();
                    continue;
                }
            }
            if msg.message == WM_LBUTTONDOWN && msg.hwnd == edit {
                edit_was_focused = GetFocus() == edit;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
            if msg.message == WM_LBUTTONUP && msg.hwnd == edit && !edit_was_focused {
                // İlk tıklamada tüm adres seçilsin, ikinci tıklama imleci yerleştirsin.
                edit_was_focused = true;
                SendMessageW(edit, EM_SETSEL, Some(WPARAM(0)), Some(LPARAM(-1)));
            }
            run_deferred();
        }
        APP.with(|a| a.borrow_mut().take());
        Ok(())
    }
}

/// İki metin kutusunun arka plan fırçaları: (adres çubuğu, ana sayfa araması).
/// Pencerenin arkasına Windows'un bulanık camını (Acrylic) koyar ya da kaldırır.
/// Desteklenmiyorsa (Windows 10, eski Windows 11) `false` döner.
pub(crate) fn apply_backdrop(hwnd: HWND, on: bool) -> bool {
    unsafe {
        let margins = if on {
            MARGINS { cxLeftWidth: -1, cxRightWidth: -1, cyTopHeight: -1, cyBottomHeight: -1 }
        } else {
            MARGINS { cxLeftWidth: 0, cxRightWidth: 0, cyTopHeight: 1, cyBottomHeight: 0 }
        };
        let _ = DwmExtendFrameIntoClientArea(hwnd, &margins);
        let kind = if on { DWMSBT_TRANSIENTWINDOW } else { DWMSBT_AUTO };
        let ok = DwmSetWindowAttribute(hwnd, DWMWA_SYSTEMBACKDROP_TYPE, &kind as *const _ as _, std::mem::size_of_val(&kind) as u32).is_ok();
        if on && !ok {
            let _ = DwmExtendFrameIntoClientArea(hwnd, &MARGINS { cxLeftWidth: 0, cxRightWidth: 0, cyTopHeight: 1, cyBottomHeight: 0 });
        }
        ok
    }
}

/// Sekmenin kapatma düğmesi: Safari'deki gibi solda.
fn tab_close_rect(r: Rect) -> Rect {
    Rect::new(r.x + 6.0, r.cy() - 9.0, 18.0, 18.0)
}

fn edit_brushes() -> (HBRUSH, HBRUSH) {
    BRUSHES.with(|b| {
        *b.borrow_mut().get_or_insert_with(|| unsafe {
            (CreateSolidBrush(COLORREF(colorref(address_field()))), CreateSolidBrush(COLORREF(colorref(search()))))
        })
    })
}

extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            // Sistem çerçevesini kaldır: başlık çubuğunu biz çiziyoruz.
            WM_NCCALCSIZE if wparam.0 != 0 => {
                if IsZoomed(hwnd).as_bool() {
                    let params = &mut *(lparam.0 as *mut NCCALCSIZE_PARAMS);
                    let dpi = GetDpiForWindow(hwnd);
                    let frame = GetSystemMetricsForDpi(SM_CXFRAME, dpi) + GetSystemMetricsForDpi(SM_CXPADDEDBORDER, dpi);
                    let r = &mut params.rgrc[0];
                    r.left += frame;
                    r.right -= frame;
                    r.top += frame;
                    r.bottom -= frame;
                }
                return LRESULT(0);
            }
            WM_ERASEBKGND => return LRESULT(1),
            WM_CTLCOLOREDIT => {
                let hdc = HDC(wparam.0 as _);
                let is_start = GetDlgCtrlID(HWND(lparam.0 as _)) == START_ID;
                let bg = if is_start { search() } else { address_field() };
                SetTextColor(hdc, COLORREF(colorref(ink())));
                SetBkColor(hdc, COLORREF(colorref(bg)));
                let (address, search) = edit_brushes();
                return LRESULT(if is_start { search.0 } else { address.0 } as isize);
            }
            WM_COMMAND => {
                let code = ((wparam.0 >> 16) & 0xFFFF) as u32;
                // Yalnızca odak değişince çerçeve yenilenir; her harfte bütün pencere
                // yeniden çizilmez (yazarken titremeyi önler).
                if code == EN_SETFOCUS || code == EN_KILLFOCUS {
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                let control = HWND(lparam.0 as _);
                if code == 0 && with_app(|app| app.default_prompt_command(control)).unwrap_or(false) {
                    run_deferred(); return LRESULT(0);
                }
                if code == EN_CHANGE && !SUPPRESS_CHANGE.with(|c| c.get()) {
                    with_app(|app| app.on_edit_change(control));
                    run_deferred();
                } else if code == EN_KILLFOCUS {
                    with_app(|app| { app.close_suggest(); app.layout_children(); });
                    run_deferred();
                } else if code == EN_SETFOCUS {
                    with_app(|app| app.layout_children());
                    run_deferred();
                }
                return LRESULT(0);
            }
            WM_SYSCOMMAND if (wparam.0 & 0xFFF0) as u32 == SC_KEYMENU => return LRESULT(0),
            WM_GETMINMAXINFO => {
                let info = &mut *(lparam.0 as *mut MINMAXINFO);
                let scale = GetDpiForWindow(hwnd).max(96) as f32 / 96.0;
                info.ptMinTrackSize = POINT { x: (560.0 * scale) as i32, y: (360.0 * scale) as i32 };
                return LRESULT(0);
            }
            WM_DESTROY => {
                crate::log::line("PENCERE YOK EDİLDİ");
                PostQuitMessage(0);
                return LRESULT(0);
            }
            _ => {}
        }

        let handled = with_app(|app| app.handle(msg, wparam, lparam));
        if handled.is_none() && msg == WM_PAINT {
            // Uygulama meşguldü: çizimi kaçırmamak için sonra yeniden iste.
            defer(move || { let _ = InvalidateRect(Some(hwnd), None, false); });
        }
        if let Some(Some(result)) = handled {
            return result;
        }
        DefWindowProcW(hwnd, msg, wparam, lparam)
    }
}

extern "system" fn menu_wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        if msg == WM_ERASEBKGND {
            return LRESULT(1);
        }
        let handled = with_app(|app| app.menu_message(hwnd, msg, wparam, lparam));
        if handled.is_none() && msg == WM_PAINT {
            defer(move || { let _ = InvalidateRect(Some(hwnd), None, false); });
        }
        if let Some(Some(result)) = handled {
            return result;
        }
        DefWindowProcW(hwnd, msg, wparam, lparam)
    }
}

extern "system" fn dialog_wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        if msg == WM_ERASEBKGND {
            return LRESULT(1);
        }
        let handled = with_app(|app| app.dialog_message(hwnd, msg, wparam, lparam));
        run_deferred();
        if let Some(Some(result)) = handled {
            return result;
        }
        DefWindowProcW(hwnd, msg, wparam, lparam)
    }
}

extern "system" fn suggest_wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        if msg == WM_ERASEBKGND {
            return LRESULT(1);
        }
        let handled = with_app(|app| app.suggest_message(hwnd, msg, wparam, lparam));
        run_deferred();
        if let Some(Some(result)) = handled {
            return result;
        }
        DefWindowProcW(hwnd, msg, wparam, lparam)
    }
}

extern "system" fn popup_wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        if msg == WM_ERASEBKGND {
            return LRESULT(1);
        }
        if let Some(Some(result)) = with_app(|app| app.popup_message(hwnd, msg, wparam, lparam)) {
            return result;
        }
        DefWindowProcW(hwnd, msg, wparam, lparam)
    }
}

// ---------- Uygulama ----------

impl App {
    fn new(hwnd: HWND, edit: HWND, start_edit: HWND) -> Result<Self> {
        let dpi = unsafe { GetDpiForWindow(hwnd) }.max(96);
        let state = state::load();
        set_theme(&state.settings.theme);
        set_accent(&state.settings.accent);
        set_motion(state.settings.animations);
        // Arayüz her zaman CPU ile çizilir: küçük bir alan için GPU cihazı açmak
        // onlarca MB ekler. "Donanım hızlandırma" ayarı sayfaların motoruna uygulanır.
        let mut gfx = Gfx::new()?;
        gfx.set_dpi(dpi);
        let mut app = Self {
            default_prompt: None,
            hwnd,
            edit,
            start_edit,
            edit_font: HFONT::default(),
            start_font: HFONT::default(),
            gfx,
            dpi,
            width: 0,
            height: 0,
            tabs: Vec::new(),
            ghosts: Vec::new(),
            active: 0,
            next_id: 1,
            env: None,
            env_pending: false,
            browser_major: 0,
            closed: Vec::new(),
            hover: Hit::None,
            previous_hover: Hit::None,
            hover_progress: 1.0,
            selection_x: f32::NAN,
            selection_w: f32::NAN,
            animation_interval: 16,
            pressed: Hit::None,
            tracking_mouse: false,
            focus_mode: false,
            focus_reveal: false,
            share_token: None,
            history: History::load_profile(&state.current_profile),
            state,
            dirty: false,
            rest: true,
            wake_queue: Vec::new(),
            full_screen: None,
            menu: None,
            retiring_menus: Vec::new(),
            menu_closed: None,
            anim_running: false,
            last_tick: Instant::now(),
            t0: Instant::now(),
            avatar: None,
            account_view: None,
            account_checked: None,
            extensions: Vec::new(),
            ext_icons: HashMap::new(),
            installing: false,
            adblock_loading: false,
            pending_update_enabled: None,
            pending_install: None,
            popup: None,
            surface: None,
            overlay_generation: 0,
            thumbnail_queue: std::collections::VecDeque::new(),
            thumbnail_inflight: None,
            preview_cancel: None,
            ocr_request: 0,
            auth_popups: HashMap::new(),
            retired_auth: Vec::new(),
            downloads: Vec::new(),
            next_download: 1,
            show_downloads: false,
            dl_speed: HashMap::new(),
            download_pulse: None,
            cookies_restored: false,
            cookie_epochs: HashMap::new(),
            cookie_hashes: crate::cache::Cache::new(16),
            clearing_data: HashSet::new(),
            closing: false,
            housekeeping_ticks: 0,
            memory_pressure: crate::memory::Pressure::Normal,
            icon_cache: RefCell::new(crate::cache::Cache::new(128)),
            file_icons: RefCell::new(crate::cache::Cache::new(64)),
            icon_fetching: RefCell::new(HashSet::new()),
            logo: Self::load_logo(),
            dialog: None,
            suggest: None,
            suggest_gen: 0,
            suggest_busy: false,
            vault: crate::passwords::Vault::default(),
            pending_login: None,
            hello_until: None,
            hello_pending: Vec::new(),
            hello_request: 0,
            favicon_urls: RefCell::new(crate::cache::Cache::new(128)),
            context_tab: 0,
            menu_anchor: None,
            drag: None,
            drop_snap: None,
            weather: None,
            rates: None,
            expand: None,
            weather_busy: false,
            rates_busy: false,
            weather_retry: 0,
            rates_retry: 0,
            tooltip: HWND::default(),
            tooltip_text: HSTRING::new(),
            download_owner: HashMap::new(),
            lingering: Vec::new(),
            ram_line: String::new(),
            asleep_count: 0,
            update_ready: None,
            update_status: String::new(),
            update_busy: false,
            restart_with: None,
            mascot: crate::image::from_bytes(include_bytes!("../Assets/mascot-question.png"), 260).map(Rc::new),
        };
        app.vault = crate::passwords::Vault::load(&app.state.current_profile);
        app.next_download = app.state.download_list.iter().map(|d| d.id).max().unwrap_or(0) + 1;
        app.apply_fonts();
        Ok(app)
    }

    /// Oturumu geri yükler: hiçbir sekme motor başlatmaz, yalnızca etkin olan uyanır.
    /// Cam efektini ayara ve Windows'un saydamlık tercihine göre açar/kapatır.
    fn update_glass(&mut self) {
        let want = self.state.settings.glass && crate::theme::system_transparency();
        let on = apply_backdrop(self.hwnd, want) && want;
        crate::theme::set_glass(on);
        self.gfx.set_glass(on);
        self.invalidate();
    }

    fn start(&mut self) {
        self.update_glass();
        // Başlangıç sayfası kartları açılışta hazırlanır: kayıtlı veri hemen, yenisi arkada.
        self.refresh_widgets();
        let saved = std::mem::take(&mut self.state.tabs);
        if self.state.settings.startup != "newtab" {
            for t in &saved {
                self.push_tab(t.url.clone(), t.title.clone());
                if let Some(tab) = self.tabs.last_mut() { tab.pinned = t.pinned; }
            }
        }

        // Komut satırından gelen adresler yeni sekmede açılır: sqywein.exe https://ornek.com
        let mut opened = 0;
        let mut wake_all = false;
        let mut literal = false;
        for arg in std::env::args().skip(1) {
            if arg == "--" && !literal { literal = true; continue; }
            if literal {
                if let Some(u) = url::to_url_with(&arg, self.state.settings.search_url()) { self.push_tab(u,String::new()); opened += 1; }
                continue;
            }
            match arg.as_str() {
                "--wake-all" => wake_all = true, // RAM karşılaştırması: tüm sekmeler yüklensin.
                arg if arg.starts_with("--wait-pid=") => {},
                "--no-rest" => self.rest = false, // Karşılaştırma: otomatik uyutma kapalı.
                arg if arg.starts_with('-') => {},
                _ => {
                    if let Some(u) = url::to_url(&arg) {
                        self.push_tab(u, String::new());
                        opened += 1;
                    }
                }
            }
        }
        if self.tabs.is_empty() {
            self.push_tab(String::new(), String::new());
        }
        // Açılışta sekmeler animasyonsuz yerinde dursun.
        let past = Instant::now().checked_sub(Duration::from_secs(5)).unwrap_or_else(Instant::now);
        for t in &mut self.tabs {
            t.born = past;
        }
        let first = if opened > 0 {
            self.tabs.len() - 1
        } else {
            (self.state.active_index.max(0) as usize).min(self.tabs.len() - 1)
        };
        self.layout_children();
        self.activate(first);
        // Boş başlangıçta motor ve gizli hesap görünümü açılmaz; ihtiyaç anında hazırlanır.
        if wake_all {
            self.wake_queue = self.tabs.iter().map(|t| t.id).collect();
            unsafe { SetTimer(Some(self.hwnd), TIMER_WAKE_ALL, 1500, None) };
        }
        unsafe { SetTimer(Some(self.hwnd), TIMER_HOUSEKEEPING, 5000, None) };
        self.load_avatar();
        self.start_adblock();
        self.maybe_check_updates();
        self.dirty = false; // Açılışta değişen bir şey yok; gereksiz kayıt yapılmasın.
        self.init_browser_integration();
        self.refresh_theme();
    }

    fn push_tab(&mut self, url: String, title: String) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        self.tabs.push(Tab::new(id, url, title));
        self.tabs.len() - 1
    }

    fn index_of(&self, id: u32) -> Option<usize> {
        self.tabs.iter().position(|t| t.id == id)
    }

    fn scale(&self) -> f32 {
        self.dpi as f32 / 96.0
    }

    fn invalidate(&self) {
        unsafe { let _ = InvalidateRect(Some(self.hwnd), None, false); }
    }

    fn on_start_page(&self) -> bool {
        !self.chrome_hidden() && self.tabs.get(self.active).is_some_and(|t| t.url.is_empty())
    }

    fn active_view(&self) -> Option<ICoreWebView2> {
        self.tabs.get(self.active).and_then(|t| t.view.clone())
    }

    /// Profil verisine ulaşmak için herhangi bir görünüm (gizli olmayan).
    fn any_view(&self) -> Option<ICoreWebView2> {
        let active = self.tabs.get(self.active).filter(|t| !t.private).and_then(|t| t.view.clone());
        active
            .or_else(|| self.tabs.iter().filter(|t| !t.private).find_map(|t| t.view.clone()))
            .or_else(|| self.account_view.as_ref().and_then(|c| unsafe { c.CoreWebView2() }.ok()))
    }

    // ---------- Mesajlar ----------

    fn handle(&mut self, msg: u32, wparam: WPARAM, lparam: LPARAM) -> Option<LRESULT> {
        match msg {
            crate::integration::LINK_MESSAGE => {
                if lparam.0 == 0 { return Some(LRESULT(0)); }
                let data = unsafe { &*(lparam.0 as *const crate::integration::CopyData) };
                if data.tag != 0x535159 || data.size > 64 * 1024 || data.bytes.is_null() { return Some(LRESULT(0)); }
                let bytes = unsafe { std::slice::from_raw_parts(data.bytes, data.size as usize) };
                let Ok(urls) = serde_json::from_slice::<Vec<String>>(bytes) else { return Some(LRESULT(0)) };
                for target in urls.into_iter().take(100).filter(|u| u.len() <= 8192) {
                    if let Some(u) = url::to_url_with(&target, self.state.settings.search_url()) { self.new_tab(u, None); }
                }
                Some(LRESULT(1))
            }
            WM_PAINT => {
                let mut ps = PAINTSTRUCT::default();
                unsafe { BeginPaint(self.hwnd, &mut ps) };
                self.paint();
                unsafe { let _ = EndPaint(self.hwnd, &ps); }
                Some(LRESULT(0))
            }
            WM_SIZE => {
                self.width = (lparam.0 & 0xFFFF) as u32;
                self.height = ((lparam.0 >> 16) & 0xFFFF) as u32;
                self.on_size(wparam.0 as u32 == SIZE_MINIMIZED);
                Some(LRESULT(0))
            }
            WM_EXITSIZEMOVE => {
                self.sync_size();
                None
            }
            WM_MOVE => {
                self.close_menu();
                self.close_popup();
                self.close_suggest();
                None
            }
            WM_SETTINGCHANGE | WM_THEMECHANGED => {
                set_motion(self.state.settings.animations);
                if self.state.settings.theme == "system" && set_theme("system") { self.refresh_theme(); }
                if crate::theme::system_transparency() != crate::theme::glass() { self.update_glass(); }
                self.animate(); None
            }
            WM_ACTIVATEAPP if wparam.0 != 0 => {
                if self.state.settings.theme=="system" && set_theme("system"){self.refresh_theme();}
                if self.default_prompt.is_some() && crate::integration::is_default_browser(){self.dismiss_default_prompt();}
                None
            }
            WM_DPICHANGED => {
                self.dpi = ((wparam.0 >> 16) & 0xFFFF) as u32;
                self.gfx.set_dpi(self.dpi);
                self.apply_fonts();
                let r = unsafe { *(lparam.0 as *const RECT) };
                let hwnd = self.hwnd;
                defer(move || unsafe {
                    let _ = SetWindowPos(hwnd, None, r.left, r.top, r.right - r.left, r.bottom - r.top, SWP_NOZORDER | SWP_NOACTIVATE);
                });
                Some(LRESULT(0))
            }
            WM_NCHITTEST => Some(LRESULT(self.hit_test_nc(lparam) as isize)),
            WM_MOUSEMOVE => {
                if wparam.0 & 0x0001 != 0 && self.drag.is_some() {
                    let (x, _) = self.point(lparam);
                    self.drag_move(x);
                    return Some(LRESULT(0));
                }
                if self.focus_mode && !self.focus_reveal && self.point(lparam).1 <= FOCUS_STRIP + 1.0 {
                    self.set_focus_reveal(true);
                }
                let hit = self.hit(lparam);
                if !self.tracking_mouse {
                    let mut tme = TRACKMOUSEEVENT {
                        cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                        dwFlags: TME_LEAVE,
                        hwndTrack: self.hwnd,
                        dwHoverTime: 0,
                    };
                    unsafe { let _ = TrackMouseEvent(&mut tme); }
                    self.tracking_mouse = true;
                }
                if hit != self.hover {
                    self.previous_hover = self.hover;
                    self.hover_progress = 0.0;
                    self.hover = hit;
                    self.animate();
                    self.update_tooltip();
                    self.invalidate();
                }
                Some(LRESULT(0))
            }
            WM_RBUTTONUP => {
                if let Hit::Tab(i) | Hit::TabClose(i) = self.hit(lparam) {
                    self.open_tab_menu(i);
                }
                Some(LRESULT(0))
            }
            WM_CAPTURECHANGED => {
                if self.drag.take().is_some() {
                    self.animate();
                }
                None
            }
            WM_MOUSELEAVE => {
                self.tracking_mouse = false;
                if self.focus_reveal && self.menu.is_none() && self.suggest.is_none() && self.popup.is_none() && unsafe { GetFocus() } != self.edit {
                    self.set_focus_reveal(false);
                }
                self.previous_hover = self.hover;
                self.hover = Hit::None;
                self.hover_progress = 0.0;
                self.animate();
                self.pressed = Hit::None;
                self.invalidate();
                Some(LRESULT(0))
            }
            WM_LBUTTONDOWN => {
                let hit = self.hit(lparam);
                self.pressed = hit;
                self.close_suggest();
                match hit {
                    Hit::Tab(i) => {
                        self.activate(i); // Basar basmaz geçsin: daha çevik hissettirir.
                        let (x, _) = self.point(lparam);
                        self.begin_drag(i, x);
                    }
                    Hit::None if self.on_start_page() => self.focus_start(),
                    _ => {}
                }
                Some(LRESULT(0))
            }
            WM_LBUTTONUP => {
                if self.end_drag() {
                    self.pressed = Hit::None;
                    self.invalidate();
                    return Some(LRESULT(0));
                }
                let hit = self.hit(lparam);
                if hit == self.pressed {
                    self.click(hit);
                }
                self.pressed = Hit::None;
                Some(LRESULT(0))
            }
            WM_MBUTTONUP => {
                match self.hit(lparam) {
                    Hit::Tab(i) | Hit::TabClose(i) => self.close_tab(i),
                    Hit::BarItem(i) => self.open_bookmark(i, true),
                    Hit::Tile(i) => self.open_shortcut(i, true),
                    _ => {}
                }
                Some(LRESULT(0))
            }
            WM_SETFOCUS => {
                self.focus_content();
                Some(LRESULT(0))
            }
            WM_TIMER => {
                match wparam.0 {
                    suggest::TIMER_SUGGEST => self.fetch_suggestions(),
                    overlay::TIMER_THUMBNAIL => self.thumbnail_timeout(),
                    translate::TIMER_TRANSLATE => self.translate_tick(),
                    extensions::TIMER_EXPAND => self.expand_tick(),
                    TIMER_HOUSEKEEPING => {
                        self.fold_privacy_stats();
                        if self.on_start_page() { self.refresh_widgets(); }
                        // Oturum çerezleri 30 saniyede bir şifreli olarak saklanır.
                        self.housekeeping_ticks = self.housekeeping_ticks.wrapping_add(1);
                        let previous = self.memory_pressure;
                        self.memory_pressure = previous.sample();
                        if self.memory_pressure != previous {
                            crate::log::line(&format!("BELLEK {:?}", self.memory_pressure));
                        }
                        if self.memory_pressure != previous && self.memory_pressure != crate::memory::Pressure::Normal {
                            self.icon_cache.borrow_mut().trim_to(32);
                            self.file_icons.borrow_mut().trim_to(16);
                            self.favicon_urls.borrow_mut().trim_to(32);
                            self.gfx.trim_images();
                        }
                        if self.housekeeping_ticks.is_multiple_of(6) {
                            self.snapshot_cookies();
                        }
                        self.cleanup_auth_popups(false);
                        self.rest_unused_tabs();
                        self.update_ram_line();
                        if self.housekeeping_ticks.is_multiple_of(720) {
                            self.maybe_check_updates(); // Saatte bir bakar; 12 saat dolduysa denetler.
                        }
                        if self.dirty {
                            self.save();
                        }
                        if self.history.is_dirty() {
                            let _ = self.history.save_profile(&self.state.current_profile);
                        }
                    }
                    TIMER_WAKE_ALL => match self.wake_queue.first().copied() {
                        Some(id) => {
                            self.wake_queue.remove(0);
                            if let Some(i) = self.index_of(id) {
                                self.activate(i);
                            }
                        }
                        None => unsafe { let _ = KillTimer(Some(self.hwnd), TIMER_WAKE_ALL); },
                    },
                    TIMER_ANIM => self.tick(),
                    session::TIMER_CLOSE => self.finish_close(),
                    _ => {}
                }
                Some(LRESULT(0))
            }
            WM_ENGINE => {
                while let Some(e) = engine::pop() {
                    self.on_engine(e);
                }
                for b in engine::take_background() {
                    self.on_background(b);
                }
                Some(LRESULT(0))
            }
            WM_CLOSE => {
                crate::log::line("PENCERE KAPAT İSTEĞİ");
                self.remember_placement();
                self.begin_close();
                Some(LRESULT(0))
            }
            _ => None,
        }
    }

    /// Pencerenin gerçek iç boyutunu okuyup çizim yüzeyini ve yerleşimi ona uydurur.
    fn sync_size(&mut self) {
        let mut rc = RECT::default();
        if unsafe { GetClientRect(self.hwnd, &mut rc) }.is_err() {
            return;
        }
        let (w, h) = ((rc.right - rc.left).max(0) as u32, (rc.bottom - rc.top).max(0) as u32);
        let minimized = unsafe { IsIconic(self.hwnd) }.as_bool();
        if (w, h) != (self.width, self.height) && !minimized {
            self.width = w;
            self.height = h;
            self.on_size(false);
        }
    }

    fn on_size(&mut self, minimized: bool) {
        if let Some(view) = self.active_view() {
            engine::set_low_memory(&view, minimized);
        }
        if !minimized {
            self.gfx.resize(self.width, self.height);
            self.layout_children();
            self.animate();
        } else {
            self.anim_running = false;
            unsafe { let _ = KillTimer(Some(self.hwnd), TIMER_ANIM); }
        }
        self.close_menu();
        self.close_popup();
        self.close_suggest();
        self.invalidate();
    }

    // ---------- Animasyon ----------

    /// Animasyon zamanlayıcısını gerekiyorsa başlatır (~60 kare/sn).
    fn animate(&mut self) {
        if unsafe { IsIconic(self.hwnd) }.as_bool() {
            self.stop_animation(); return;
        }
        if !motion_enabled() {
            let targets = self.layout().tabs;
            for (t, r) in self.tabs.iter_mut().zip(targets.iter()) { t.x = r.x; t.w = r.w; }
            if let Some(r) = self.layout().tabs.get(self.active) { self.selection_x = r.x; self.selection_w = r.w; }
            self.ghosts.clear(); self.hover_progress = 1.0;
            self.stop_animation();
            self.invalidate(); return;
        }
        if !self.anim_running {
            self.anim_running = true;
            self.animation_interval = 10;
            self.last_tick = Instant::now();
            // Windows zamanlayıcısı varsayılan olarak 15,6 ms adımlıdır; animasyon
            // sürerken 1 ms'ye inilir ki kareler eşit aralıkla gelsin (takılma olmasın).
            unsafe { let _ = windows::Win32::Media::timeBeginPeriod(1); }
            unsafe { SetTimer(Some(self.hwnd), TIMER_ANIM, 10, None) };
        }
    }

    /// Ana sayfa düğmesi: bu sekmede başlangıç sayfasını gösterir.
    fn go_home(&mut self) {
        let i = self.active;
        let tab = &mut self.tabs[i];
        tab.url.clear();
        tab.title.clear();
        tab.error = None;
        tab.loading = false;
        self.show_active();
        self.animate();
    }

    /// Başlangıç sayfası kartları: kayıtlı veri hemen görünür; hava durumu yarım
    /// saatte, kurlar üç saatte bir arkada yenilenir. İkisi aynı anda istenir.
    fn refresh_widgets(&mut self) {
        if !self.state.settings.show_widgets || self.tabs.get(self.active).is_some_and(|t| t.private) {
            return;
        }
        use crate::widgets::{now, WeatherState};
        let t = now();
        if self.weather.is_none() {
            if let Some((_, w)) = &self.state.widgets.weather { self.weather = Some(WeatherState::Ready(w.clone())); }
        }
        if self.rates.is_none() {
            if let Some((_, r)) = &self.state.widgets.rates { self.rates = Some(r.clone()); }
        }
        let stale = |at: Option<u64>, limit: u64| at.is_none_or(|at| t.saturating_sub(at) >= limit);
        if !self.weather_busy && t >= self.weather_retry && stale(self.state.widgets.weather.as_ref().map(|w| w.0), 30 * 60) {
            self.weather_busy = true;
            crate::widgets::refresh_weather(self.state.settings.weather_city.clone(), self.state.widgets.location.clone());
        }
        if !self.rates_busy && t >= self.rates_retry && stale(self.state.widgets.rates.as_ref().map(|r| r.0), 3 * 3600) {
            self.rates_busy = true;
            crate::widgets::refresh_rates();
        }
    }

    fn on_weather(&mut self, weather: crate::widgets::WeatherState, location: Option<crate::widgets::Location>) {
        use crate::widgets::{now, WeatherState};
        self.weather_busy = false;
        match &weather {
            WeatherState::Ready(w) => {
                self.state.widgets.weather = Some((now(), w.clone()));
                self.state.widgets.location = location;
                self.dirty = true;
                self.weather = Some(weather);
            }
            // Kayıtlı veri varken geçici bir hata onu silmez; beş dakika sonra yeniden denenir.
            WeatherState::Failed => {
                self.weather_retry = now() + 300;
                if self.weather.is_none() { self.weather = Some(weather); }
            }
            WeatherState::NoLocation => {
                self.weather_retry = now() + 300;
                if self.state.widgets.weather.is_none() { self.weather = Some(weather); }
            }
        }
        if self.on_start_page() { self.invalidate(); }
    }

    fn on_rates(&mut self, rates: Option<crate::widgets::Rates>) {
        use crate::widgets::now;
        self.rates_busy = false;
        match rates {
            Some(r) => { self.state.widgets.rates = Some((now(), r.clone())); self.rates = Some(r); self.dirty = true; }
            None => self.rates_retry = now() + 300,
        }
        if self.on_start_page() { self.invalidate(); }
    }

    /// Gizli ↔ normal: öbür moddaki en son kullanılan sekmeye geçer; yoksa o modda yeni sekme açar.
    fn switch_mode(&mut self) {
        let private = !self.tabs[self.active].private;
        let target = self.tabs.iter().enumerate().filter(|(_, t)| t.private == private).max_by_key(|(_, t)| t.last_active).map(|(i, _)| i);
        match target {
            Some(i) => self.activate(i),
            None if private => self.new_private_tab(),
            None => self.new_tab(String::new(), None),
        }
        self.layout_children();
        self.animate();
        self.invalidate();
    }

    /// Ctrl+Tab: aynı moddaki (gizli ya da normal) bir sonraki sekme.
    fn cycle_tab(&self, step: usize) -> usize {
        let n = self.tabs.len();
        let private = self.tabs[self.active].private;
        let mut i = self.active;
        for _ in 0..n {
            i = (i + step) % n;
            if self.tabs[i].private == private {
                return i;
            }
        }
        self.active
    }

    fn stop_animation(&mut self) {
        if self.anim_running {
            unsafe { let _ = windows::Win32::Media::timeEndPeriod(1); }
        }
        self.anim_running = false;
        unsafe { let _ = KillTimer(Some(self.hwnd), TIMER_ANIM); }
    }

    fn interaction_moving(&self) -> bool {
        let targets = self.layout().tabs;
        self.hover_progress < 1.0 || !self.ghosts.is_empty()
            || self.tabs.iter().any(|t| t.open_progress() < 1.0)
            || self.tabs.iter().zip(&targets).any(|(t,r)| t.x != r.x || t.w != r.w)
            || targets.get(self.active).is_some_and(|r| self.selection_x != r.x || self.selection_w != r.w)
    }

    fn needs_animation(&self) -> bool {
        let targets = self.layout().tabs;
        self.interaction_moving()
            || self.installing
            || self.download_pulse.is_some_and(|t| t.elapsed() < Duration::from_millis(2400))
            || self.downloads.iter().any(|(id, _)| self.download_record(*id).is_some_and(|d| d.status == "progress"))
            || self.dragging_id().is_some()
            || self.tabs.iter().any(|t| t.loading || t.open_progress() < 1.0)
            || self.tabs.iter().zip(&targets).any(|(t, r)| t.x != r.x || t.w != r.w)
    }

    fn tick(&mut self) {
        let now = Instant::now();
        let dt = now.duration_since(self.last_tick).as_secs_f32().min(0.05);
        self.last_tick = now;
        let k = 1.0 - (-dt * 18.0).exp();
        self.hover_progress += (1.0 - self.hover_progress) * (1.0 - (-dt * 32.0).exp());
        if self.hover_progress > 0.995 { self.hover_progress = 1.0; }
        let targets = self.layout().tabs;
        let dragged = self.dragging_id();
        // Sürüklerken yer açan sekmeler ve bırakılan sekme hızlı ve net oturur.
        let snap = 1.0 - (-dt * 34.0).exp();
        let drop = self.drop_snap;
        for (t, r) in self.tabs.iter_mut().zip(&targets) {
            if Some(t.id) != dragged {
                let kk = if dragged.is_some() || Some(t.id) == drop { snap } else { k };
                t.x = lerp_toward(t.x, r.x, kk);
            }
            t.w = lerp_toward(t.w, r.w, k);
        }
        if let (Some(r), Some(tab)) = (targets.get(self.active), self.tabs.get(self.active)) {
            if Some(tab.id) == dragged || Some(tab.id) == drop {
                // Etkin sekme taşınıyor: kart yazıyla birlikte, aynı karede hareket eder.
                self.selection_x = tab.x;
                self.selection_w = tab.w;
            } else {
                self.selection_x = lerp_toward(self.selection_x, r.x, k);
                self.selection_w = lerp_toward(self.selection_w, r.w, k);
            }
        }
        if let Some(id) = drop {
            if self.index_of(id).is_none_or(|i| self.tabs[i].x == targets[i].x) { self.drop_snap = None; }
        }
        let interval = if self.interaction_moving() { 10 } else { 16 };
        if interval != self.animation_interval { self.animation_interval = interval; unsafe { SetTimer(Some(self.hwnd), TIMER_ANIM, interval, None); } }
        self.ghosts.retain(|g| g.progress() < 1.0);
        self.invalidate();
        if !self.needs_animation() {
            self.stop_animation();
        }
    }

    /// Ekranda çizilecek sekme dikdörtgenleri (animasyonlu konumlar).
    fn shown_tabs(&self, l: &Layout) -> Vec<Rect> {
        self.tabs
            .iter()
            .zip(&l.tabs)
            .map(|(t, r)| {
                let x = if t.x.is_nan() { r.x } else { t.x };
                let w = if t.w.is_nan() { r.w } else { t.w };
                Rect::new(x, r.y, w * t.open_progress(), r.h)
            })
            .collect()
    }

    // ---------- Yerleşim ----------

    fn layout(&self) -> Layout {
        let s = self.scale();
        let w = self.width as f32 / s;
        let h = self.height as f32 / s;

        // Pencere düğmeleri sağ üst köşede, araç çubuğunun içinde.
        let close = Rect::new(w - CAPTION_BTN, 0.0, CAPTION_BTN, CAPTION_H);
        let max = Rect::new(close.x - CAPTION_BTN, 0.0, CAPTION_BTN, close.h);
        let min = Rect::new(max.x - CAPTION_BTN, 0.0, CAPTION_BTN, close.h);

        // Araç çubuğu (Safari düzeni): solda geri/ileri, ortada adres, sağda araçlar.
        let cy = TOOL_ROW / 2.0;
        let btn = |x: f32| Rect::new(x, cy - 16.0, 32.0, 32.0);
        let back = btn(12.0);
        let forward = btn(back.right() + 2.0);
        let home = btn(forward.right() + 2.0);

        // Sağdan sola: menü, yeni sekme, yapboz, sabitlenen uzantılar, indirmeler.
        // Profil düğmesi yok: Google oturumu arka planda kendiliğinden çalışır.
        let menu = btn(min.x - 40.0);
        let profile = Rect::new(0.0, 0.0, 0.0, 0.0);
        let new_tab = btn(menu.x - 36.0);
        let share = btn(new_tab.x - 36.0);
        let puzzle = btn(share.x - 36.0);
        let mut x = puzzle.x;
        let pinned: Vec<Rect> = self
            .pinned_extensions()
            .iter()
            .map(|_| {
                x -= 36.0;
                btn(x)
            })
            .collect();
        let downloads = self.show_downloads.then(|| {
            x -= 36.0;
            btn(x)
        });
        let update = self.update_pill_width().map(|w| {
            x -= w + 6.0;
            Rect::new(x, cy - 12.0, w, 24.0)
        });

        // Adres alanı pencerenin tam ortasında; yer yoksa iki grup arasına sığar.
        let (lo, hi) = (home.right() + 16.0, x - 16.0);
        let room = (hi - lo).max(120.0);
        let aw = (w * 0.46).clamp(300.0, 720.0).min(room);
        let ax = (w / 2.0 - aw / 2.0).clamp(lo, (hi - aw).max(lo));
        let address = Rect::new(ax, cy - 17.0, aw, 34.0);

        // Adresin sağ ucu: yenile, yıldız, kalkan, yakınlaştırma, "Sqywein'e ekle".
        let reload = Rect::new(address.right() - 30.0, cy - 13.0, 26.0, 26.0);
        let star = Rect::new(reload.x - 28.0, cy - 13.0, 26.0, 26.0);
        let mut inner_right = star.x;
        let translate = self.translate_visible().then(|| {
            let r = Rect::new(inner_right - 28.0, cy - 13.0, 26.0, 26.0);
            inner_right = r.x;
            r
        });
        let shield = self.shield_visible().then(|| {
            let w = self.shield_width();
            let r = Rect::new(inner_right - w - 2.0, cy - 13.0, w, 26.0);
            inner_right = r.x;
            r
        });
        let zoom_pill = self.zoom_label().map(|_| {
            let r = Rect::new(inner_right - 56.0, cy - 11.0, 52.0, 22.0);
            inner_right = r.x;
            r
        });
        let add_ext = self.store_page_id().map(|_| {
            let r = Rect::new(inner_right - 124.0, cy - 12.0, 118.0, 24.0);
            inner_right = r.x;
            r
        });
        let text_x = address.x + if self.tabs.get(self.active).is_some_and(|t| t.private) { 64.0 } else { 30.0 };
        let address_text = Rect::new(text_x, address.y, (inner_right - text_x - 6.0).max(20.0), address.h);

        // Sekmeler: alttaki şeridi eşit genişlikte paylaşır.
        // Gizli sekmedeyken şeritte yalnızca gizli sekmeler görünür (ayrı pencere gibi);
        // öbürleri sol kenarda sıfır genişliğe kapanır.
        let left = 8.0;
        let avail = (w - 2.0 * left).max(0.0);
        let private_mode = self.tabs.get(self.active).is_some_and(|t| t.private);
        let n = self.tabs.iter().filter(|t| t.private == private_mode).count().max(1) as f32;
        let mode = self.tabs.iter().any(|t| t.private).then(|| Rect::new(8.0, TOOL_ROW + 3.0, 34.0, TAB_ROW - 7.0));
        let left = if mode.is_some() { left + 38.0 } else { left };
        let avail = if mode.is_some() { (avail - 38.0).max(0.0) } else { avail };
        let tw = (avail / n).max(TAB_MIN.min(avail / n));
        let mut slot = 0.0;
        let tabs = self.tabs.iter().map(|t| {
            if t.private != private_mode {
                return Rect::new(left, TOOL_ROW + 3.0, 0.0, TAB_ROW - 7.0);
            }
            let r = Rect::new(left + slot * tw + 1.0, TOOL_ROW + 3.0, tw - 2.0, TAB_ROW - 7.0);
            slot += 1.0;
            r
        }).collect();

        let (bar, bar_items, bar_more) = if self.bar_visible() {
            let (items, more) = self.bar_layout(CHROME, w);
            (Some(Rect::new(0.0, CHROME, w, newtab::BAR_H)), items, more)
        } else {
            (None, Vec::new(), None)
        };
        let top = CHROME + bar.map_or(0.0, |b| b.h);
        let default_banner = (self.default_prompt.is_some() && !self.chrome_hidden()).then(|| Rect::new(GUTTER, top, (w - 2.0*GUTTER).max(0.0), if w < 760.0 { 86.0 } else { 48.0 }));
        let top = top + default_banner.map_or(0.0, |r| r.h);
        let content = if self.full_screen.is_some() {
            Rect::new(0.0, 0.0, w, h)
        } else if self.chrome_hidden() {
            // Odak modu: üstte fareyi yakalayan ince bir şerit kalır.
            Rect::new(0.0, FOCUS_STRIP, w, (h - FOCUS_STRIP).max(0.0))
        } else {
            Rect::new(GUTTER, top, (w - 2.0 * GUTTER).max(0.0), (h - top - GUTTER).max(0.0))
        };
        let retry = Rect::new(content.cx() - 70.0, content.cy() + 34.0, 140.0, 36.0);

        Layout {
            tabs,
            new_tab,
            min,
            max,
            close,
            back,
            forward,
            home,
            reload,
            address,
            address_text,
            star,
            translate,
            shield,
            zoom_pill,
            add_ext,
            downloads,
            update,
            pinned,
            mode,
            puzzle,
            share,
            profile,
            menu,
            content,
            default_banner,
            retry,
            bar,
            bar_items,
            bar_more,
        }
    }

    fn px(&self, r: Rect) -> RECT {
        let s = self.scale();
        RECT {
            left: (r.x * s).round() as i32,
            top: (r.y * s).round() as i32,
            right: (r.right() * s).round() as i32,
            bottom: (r.bottom() * s).round() as i32,
        }
    }

    /// Metin kutusunu verilen alanın içinde dikey ortalar.
    fn place_edit(&self, edit: HWND, area: Rect, font_dip: f32, show: bool) {
        let s = self.scale();
        unsafe {
            if !show {
                let _ = ShowWindow(edit, SW_HIDE);
                return;
            }
            let line = (font_dip * 1.45 * s).round() as i32;
            let x = (area.x * s).round() as i32;
            let w = (area.w * s).round() as i32;
            let y = (area.cy() * s).round() as i32 - line / 2;
            let _ = SetWindowPos(edit, None, x, y, w.max(10), line, SWP_NOZORDER | SWP_NOACTIVATE | SWP_SHOWWINDOW);
        }
    }

    /// Metin kutularını ve etkin sayfayı pencere boyutuna göre yerleştirir.
    fn layout_children(&self) {
        let l = self.layout();
        self.place_default_prompt(&l);
        let chrome = !self.chrome_hidden();
        // Safari gibi: yazarken metin kutusu, yazmıyorken ortada alan adı.
        let editing = unsafe { GetFocus() } == self.edit || self.tabs.get(self.active).is_none_or(|t| t.url.is_empty());
        self.place_edit(self.edit, l.address_text, 13.5, chrome && editing);
        let search = self.start_layout(l.content).search;
        self.place_edit(self.start_edit, Rect::new(search.x + 52.0, search.y, search.w - 72.0, search.h), 15.0, self.on_start_page());
        if let Some(ctrl) = self.tabs.get(self.active).and_then(|t| t.ctrl.as_ref()) {
            unsafe { let _ = ctrl.SetBounds(self.px(l.content)); }
        }
    }

    fn make_font(&self, size_dip: f32) -> HFONT {
        unsafe {
            CreateFontW(
                -(size_dip * self.scale()).round() as i32,
                0,
                0,
                0,
                FW_NORMAL.0 as i32,
                0,
                0,
                0,
                DEFAULT_CHARSET,
                OUT_DEFAULT_PRECIS,
                CLIP_DEFAULT_PRECIS,
                CLEARTYPE_QUALITY,
                (DEFAULT_PITCH.0 | FF_DONTCARE.0) as u32,
                w!("Segoe UI Variable Text"),
            )
        }
    }

    fn apply_fonts(&mut self) {
        let margin = (4.0 * self.scale()) as isize;
        for (edit, size, slot) in [(self.edit, 13.5, 0), (self.start_edit, 15.0, 1)] {
            let font = self.make_font(size);
            unsafe {
                SendMessageW(edit, WM_SETFONT, Some(WPARAM(font.0 as usize)), Some(LPARAM(1)));
                SendMessageW(edit, EM_SETMARGINS, Some(WPARAM((EC_LEFTMARGIN | EC_RIGHTMARGIN) as usize)), Some(LPARAM(margin | (margin << 16))));
            }
            let old = if slot == 0 { std::mem::replace(&mut self.edit_font, font) } else { std::mem::replace(&mut self.start_font, font) };
            if !old.is_invalid() {
                unsafe { let _ = DeleteObject(old.into()); }
            }
        }
        self.layout_children();
    }

    // ---------- İsabet testi ----------

    fn point(&self, lparam: LPARAM) -> (f32, f32) {
        let s = self.scale();
        let x = (lparam.0 & 0xFFFF) as i16 as f32 / s;
        let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as f32 / s;
        (x, y)
    }

    fn hit(&self, lparam: LPARAM) -> Hit {
        let (x, y) = self.point(lparam);
        self.hit_at(x, y)
    }

    fn hit_at(&self, x: f32, y: f32) -> Hit {
        if self.chrome_hidden() {
            return Hit::None;
        }
        let l = self.layout();
        for (r, h) in [(l.close, Hit::Close), (l.max, Hit::Max), (l.min, Hit::Min)] {
            if r.contains(x, y) {
                return h;
            }
        }
        for (r, h) in [
            (l.new_tab, Hit::NewTab),
            (l.share, Hit::Share),
            (l.back, Hit::Back),
            (l.forward, Hit::Forward),
            (l.home, Hit::Home),
            (l.reload, Hit::Reload),
            (l.star, Hit::Star),
            (l.puzzle, Hit::Puzzle),
            (l.profile, Hit::Profile),
            (l.menu, Hit::Menu),
        ] {
            if r.contains(x, y) {
                return h;
            }
        }
        for (r, h) in [(l.mode, Hit::Mode), (l.translate, Hit::Translate), (l.update, Hit::Update), (l.shield, Hit::Shield), (l.zoom_pill, Hit::ZoomPill), (l.add_ext, Hit::AddExtension), (l.downloads, Hit::Downloads)] {
            if r.is_some_and(|r| r.contains(x, y)) {
                return h;
            }
        }
        if let Some(i) = l.pinned.iter().position(|r| r.contains(x, y)) {
            return Hit::Pinned(i);
        }
        if let Some(i) = l.bar_items.iter().position(|r| r.contains(x, y)) {
            return Hit::BarItem(i);
        }
        if l.bar_more.is_some_and(|r| r.contains(x, y)) {
            return Hit::BarMore;
        }
        if self.on_start_page() && l.content.contains(x, y) {
            return self.start_hit(l.content, x, y);
        }
        for (i, r) in l.tabs.iter().enumerate() {
            if r.contains(x, y) {
                return if r.w > 70.0 && tab_close_rect(*r).contains(x, y) { Hit::TabClose(i) } else { Hit::Tab(i) };
            }
        }
        // Adres alanının boş yeri: tıklayınca yazmaya başlanır.
        if l.address.contains(x, y) {
            return Hit::Address;
        }
        if self.tabs.get(self.active).is_some_and(|t| t.error.is_some()) && l.retry.contains(x, y) {
            return Hit::Retry;
        }
        Hit::None
    }

    fn hit_test_nc(&self, lparam: LPARAM) -> u32 {
        let mut pt = POINT { x: (lparam.0 & 0xFFFF) as i16 as i32, y: ((lparam.0 >> 16) & 0xFFFF) as i16 as i32 };
        unsafe { let _ = ScreenToClient(self.hwnd, &mut pt); }
        let s = self.scale();
        let (x, y) = (pt.x as f32 / s, pt.y as f32 / s);
        let (w, h) = (self.width as f32 / s, self.height as f32 / s);

        let maximized = unsafe { IsZoomed(self.hwnd) }.as_bool();
        if !maximized && self.full_screen.is_none() {
            let e = RESIZE_EDGE;
            let (l, r, t, b) = (x < e, x >= w - e, y < e, y >= h - e);
            match (l, r, t, b) {
                (true, _, true, _) => return HTTOPLEFT,
                (_, true, true, _) => return HTTOPRIGHT,
                (true, _, _, true) => return HTBOTTOMLEFT,
                (_, true, _, true) => return HTBOTTOMRIGHT,
                (true, ..) => return HTLEFT,
                (_, true, ..) => return HTRIGHT,
                (_, _, true, _) => return HTTOP,
                (.., true) => return HTBOTTOM,
                _ => {}
            }
        }
        if !self.chrome_hidden() && y < CHROME && self.hit_at(x, y) == Hit::None {
            return HTCAPTION;
        }
        HTCLIENT
    }

    fn click(&mut self, hit: Hit) {
        let hwnd = self.hwnd;
        match hit {
            Hit::TabClose(i) => self.close_tab(i),
            Hit::NewTab => self.run(Cmd::NewTab),
            Hit::Min => defer(move || unsafe { let _ = ShowWindow(hwnd, SW_MINIMIZE); }),
            Hit::Max => defer(move || unsafe {
                let _ = ShowWindow(hwnd, if IsZoomed(hwnd).as_bool() { SW_RESTORE } else { SW_MAXIMIZE });
            }),
            Hit::Close => defer(move || unsafe { let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0)); }),
            Hit::Back => self.run(Cmd::Back),
            Hit::Forward => self.run(Cmd::Forward),
            Hit::Reload => {
                if self.tabs[self.active].loading {
                    if let Some(v) = &self.tabs[self.active].view {
                        unsafe { let _ = v.Stop(); }
                    }
                } else {
                    self.run(Cmd::Reload);
                }
            }
            Hit::Star => self.run(Cmd::Bookmark),
            Hit::Address => self.focus_address(),
            Hit::Translate => self.open_translate(),
            Hit::Share => self.open_menu(MenuKind::Share),
            Hit::Shield => self.open_menu(MenuKind::Shield),
            Hit::Update => self.apply_update(),
            Hit::ZoomPill => {
                // Yakınlaştırmayı varsayılana döndürür.
                let z = self.default_zoom();
                if let Some(c) = &self.tabs[self.active].ctrl {
                    unsafe { let _ = c.SetZoomFactor(z); }
                }
            }
            Hit::AddExtension => {
                if let Some(id) = self.store_page_id() {
                    self.install_store(id);
                }
            }
            Hit::Downloads => self.open_menu(MenuKind::Downloads),
            Hit::Pinned(i) => {
                if let Some(id) = self.pinned_extensions().get(i).cloned() {
                    self.open_extension_popup(&id, Some(i));
                }
            }
            Hit::Puzzle => self.open_menu(MenuKind::Extensions),
            Hit::Profile => self.open_menu(MenuKind::Profile),
            Hit::Menu => self.open_menu(MenuKind::Main),
            Hit::Retry => self.run(Cmd::Reload),
            Hit::BarItem(i) => self.open_bookmark(i, true),
            Hit::BarMore => self.open_menu(MenuKind::BarOverflow),
            Hit::Tile(i) => self.open_shortcut(i, keys::modifiers().ctrl),
            Hit::TileRemove(i) => self.remove_shortcut(i),
            Hit::TileAdd => self.open_dialog(),
            Hit::Home => self.go_home(),
            Hit::Weather => self.new_tab(format!("{}#startup", url::internal("settings")), None),
            Hit::Mode => self.switch_mode(),
            Hit::Tab(_) | Hit::None => {}
        }
    }

    // ---------- Komutlar ----------

    pub fn run(&mut self, cmd: Cmd) {
        self.close_menu();
        let view = self.active_view();
        match cmd {
            // Gizli moddayken yeni sekme de gizli açılır.
            Cmd::NewTab if self.tabs.get(self.active).is_some_and(|t| t.private) => self.new_private_tab(),
            Cmd::NewTab => self.new_tab(String::new(), None),
            Cmd::PrivateTab => self.new_private_tab(),
            Cmd::CloseTab => self.close_tab(self.active),
            Cmd::ReopenTab => {
                if let Some(u) = self.closed.pop() {
                    self.new_tab(u, None);
                }
            }
            Cmd::FocusAddress => self.focus_address(),
            Cmd::Translate => self.toggle_translate(),
            Cmd::Spotlight => self.open_surface(overlay::Surface::Spotlight),
            Cmd::Overview => self.open_surface(overlay::Surface::Tabs),
            Cmd::Focus => self.toggle_focus_mode(),
            Cmd::SwitchMode => self.switch_mode(),
            Cmd::NextTab => self.activate(self.cycle_tab(1)),
            Cmd::PrevTab => self.activate(self.cycle_tab(self.tabs.len() - 1)),
            Cmd::GoToTab(i) => {
                if i < self.tabs.len() {
                    self.activate(i);
                }
            }
            Cmd::LastTab => self.activate(self.tabs.len() - 1),
            Cmd::Back => {
                if let Some(v) = view {
                    unsafe { let _ = v.GoBack(); }
                }
            }
            Cmd::Forward => {
                if let Some(v) = view {
                    unsafe { let _ = v.GoForward(); }
                }
            }
            Cmd::Reload => {
                let tab = &mut self.tabs[self.active];
                if tab.error.take().is_some() || view.is_none() {
                    tab.needs_load = !tab.url.is_empty();
                    if let Some(v) = &view {
                        tab.needs_load = false;
                        unsafe { let _ = v.Navigate(&wide(&tab.url)); }
                    }
                    self.show_active();
                } else if let Some(v) = view {
                    unsafe { let _ = v.Reload(); }
                }
            }
            Cmd::DevTools => {
                if let Some(v) = view {
                    unsafe { let _ = v.OpenDevToolsWindow(); }
                }
            }
            Cmd::Bookmark => self.toggle_bookmark(),
            Cmd::History => self.open_page("history"),
            Cmd::Downloads => self.open_page("downloads"),
            Cmd::Bookmarks => self.open_page("bookmarks"),
            Cmd::Go => {
                self.close_suggest();
                let from_start = unsafe { GetFocus() } == self.start_edit;
                let text = Self::text_of(if from_start { self.start_edit } else { self.edit });
                self.navigate(&text);
            }
            Cmd::Cancel => {
                if unsafe { GetFocus() } == self.start_edit {
                    Self::set_text(self.start_edit, "");
                } else {
                    let current = url::display(&self.tabs[self.active].url);
                    Self::set_text(self.edit, &current);
                    self.focus_content();
                }
            }
        }
        self.invalidate();
    }

    // ---------- Yer işaretleri ve yakınlaştırma ----------

    fn is_bookmarked(&self, url: &str) -> bool {
        !url.is_empty() && self.state.favorites.iter().any(|b| b.url == url)
    }

    fn toggle_bookmark(&mut self) {
        let tab = &self.tabs[self.active];
        if tab.url.is_empty() || url::is_internal(&tab.url) {
            return;
        }
        let url = tab.url.clone();
        if let Some(i) = self.state.favorites.iter().position(|b| b.url == url) {
            self.state.favorites.remove(i);
        } else {
            let title = tab.label();
            self.state.favorites.push(state::Bookmark { title, url });
        }
        self.dirty = true;
        self.notify_pages("bookmarks");
        self.invalidate();
    }

    /// Varsayılan yakınlaştırma: ayar 0 ise ekranın ölçeği (ör. %125).
    fn default_zoom(&self) -> f64 {
        match self.state.settings.default_zoom {
            0 => self.dpi as f64 / 96.0,
            pct => pct as f64 / 100.0,
        }
    }

    /// Adres çubuğunda gösterilecek yakınlaştırma (%100 değilse).
    fn zoom_label(&self) -> Option<String> {
        let tab = self.tabs.get(self.active)?;
        if tab.ctrl.is_none() || tab.url.is_empty() || (tab.zoom - 1.0).abs() < 0.005 {
            return None;
        }
        Some(format!("%{}", (tab.zoom * 100.0).round() as i32))
    }

    /// Sitenin kayıtlı yakınlaştırmasını (yoksa varsayılanı) uygular.
    fn apply_zoom(&mut self, i: usize) {
        let default = self.default_zoom();
        let tab = &mut self.tabs[i];
        let host = url::host(&tab.url).to_string();
        let Some(ctrl) = &tab.ctrl else { return };
        if host == tab.zoom_host && !host.is_empty() {
            return;
        }
        tab.zoom_host = host.clone();
        let z = self.state.zoom.get(&host).copied().unwrap_or(default);
        tab.zoom = z;
        unsafe { let _ = ctrl.SetZoomFactor(z); }
    }

    /// Kullanıcı yakınlaştırınca (Ctrl+tekerlek, menü) site için hatırlanır.
    fn on_zoom_changed(&mut self, i: usize) {
        let default = self.default_zoom();
        let tab = &mut self.tabs[i];
        let Some(ctrl) = &tab.ctrl else { return };
        let mut z = 1.0f64;
        unsafe { let _ = ctrl.ZoomFactor(&mut z); }
        tab.zoom = z;
        let host = url::host(&tab.url).to_string();
        if !host.is_empty() && !tab.private && !url::is_internal(&tab.url) {
            if (z - default).abs() < 0.005 {
                self.state.zoom.remove(&host);
            } else {
                self.state.zoom.insert(host, z);
            }
            self.dirty = true;
        }
        if let Some(m) = &mut self.menu {
            if m.kind == MenuKind::Main && i == self.active {
                m.zoom = Some(z);
                unsafe { let _ = InvalidateRect(Some(m.hwnd), None, false); }
            }
        }
        self.layout_children();
        self.invalidate();
    }

    // ---------- Menüler ----------

    fn main_menu_items(&self) -> Vec<Item> {
        let has_page = self.tabs[self.active].view.is_some();
        let page = |label: &str, hint: &str, cmd| {
            let mut item = Item::act(label, hint, cmd);
            if let Item::Action { enabled, .. } = &mut item {
                *enabled = has_page;
            }
            item
        };
        vec![
            Item::act("Yeni sekme", "Ctrl+T", MenuCmd::NewTab),
            Item::act("Yeni gizli sekme", "Ctrl+Shift+N", MenuCmd::PrivateTab),
            Item::Sep,
            Item::act("Geçmiş", "Ctrl+H", MenuCmd::History),
            Item::act("İndirmeler", "Ctrl+J", MenuCmd::Downloads),
            Item::act("Yer işaretleri", "Ctrl+Shift+O", MenuCmd::Bookmarks),
            Item::act("Uzantılar", "", MenuCmd::Extensions),
            Item::act("Şifreler", "", MenuCmd::Passwords),
            Item::Sep,
            Item::Zoom,
            page("Yazdır…", "Ctrl+P", MenuCmd::Print),
            page("Sayfada bul…", "Ctrl+F", MenuCmd::Find),
            page("Paylaş…", "", MenuCmd::OpenShare),
            page("Görüntü içinde görüntü", "", MenuCmd::PictureInPicture),
            Item::act(if self.tabs[self.active].private { "Normal sekmelere dön" } else { "Gizli sekmelere geç" }, "Ctrl+Shift+P", MenuCmd::SwitchMode),
            Item::act(if self.focus_mode { "Odak modundan çık" } else { "Odak modu" }, "Ctrl+Shift+F", MenuCmd::FocusMode),
            Item::act("Hızlı erişim", "Ctrl+Space", MenuCmd::Spotlight),
            Item::act("Sekmelere genel bakış", "Ctrl+Shift+A", MenuCmd::Overview),
            page(if self.tabs[self.active].translated { "Orijinal dili göster" } else { "Sayfayı çevir" }, "Ctrl+Shift+Y", MenuCmd::Translate),
            page("Geliştirici araçları", "F12", MenuCmd::DevTools),
            page("Bu sitenin izinleri…", "", MenuCmd::SitePermissions),
            page("Başka tarayıcıda aç", "", MenuCmd::OpenCompatible(self.tabs[self.active].id)),
            Item::act("Ağ giriş sayfasını aç", "", MenuCmd::NetworkLogin),
            Item::Sep,
            Item::act("Ayarlar", "", MenuCmd::Settings),
            Item::act("Çıkış", "", MenuCmd::Exit),
        ]
    }

    fn open_menu(&mut self, kind: MenuKind) {
        // Aynı düğmeye ikinci tıklama menüyü kapatır (menü odak kaybedince zaten kapandı).
        if let Some((k, at)) = self.menu_closed {
            if k == kind && at.elapsed() < Duration::from_millis(250) {
                self.menu_closed = None;
                return;
            }
        }
        self.close_menu();
        self.close_popup();
        let l = self.layout();
        let (anchor_dip, items) = match kind {
            MenuKind::Main => (l.menu, self.main_menu_items()),
            MenuKind::Profile => {
                self.refresh_account(false);
                (l.profile, self.profile_menu_items())
            }
            MenuKind::Extensions => {
                self.refresh_extensions();
                (l.puzzle, self.extension_menu_items())
            }
            MenuKind::Downloads => (l.downloads.unwrap_or(l.menu), self.download_items()),
            MenuKind::BarOverflow => (l.bar_more.unwrap_or(l.menu), self.bar_overflow_items()),
            MenuKind::Shield => (l.shield.unwrap_or(l.star), self.shield_menu_items()),
            MenuKind::SavePassword => (l.star, self.save_password_items()),
            MenuKind::TabContext => (l.menu, self.tab_menu_items()),
            MenuKind::Share => (l.share, self.share_menu_items()),
            MenuKind::Translate => (l.translate.unwrap_or(l.star), self.translation_items(false)),
        };
        let pt = match self.menu_anchor.take() {
            Some(pt) => pt,
            None => {
                let r = self.px(anchor_dip);
                let mut pt = POINT { x: r.right, y: r.bottom + (4.0 * self.scale()) as i32 };
                unsafe { let _ = ClientToScreen(self.hwnd, &mut pt); }
                pt
            }
        };
        let zoom = self.tabs[self.active].ctrl.as_ref().map(|_| self.tabs[self.active].zoom);
        if let Ok(m) = Menu::open(self.hwnd, kind, pt, self.dpi, items, zoom, self.gfx.hardware) {
            let hwnd = m.hwnd;
            self.menu = Some(m);
            defer(move || unsafe { let _ = ShowWindow(hwnd, SW_SHOW); });
        }
        self.invalidate();
    }

    fn close_menu(&mut self) {
        if let Some(mut m) = self.menu.take() {
            self.menu_closed = Some((m.kind, Instant::now()));
            if m.start_exit() { self.retiring_menus.push(m); }
            else { let hwnd = m.hwnd; defer(move || unsafe { let _ = DestroyWindow(hwnd); }); }
            self.invalidate();
        }
    }

    fn menu_message(&mut self, hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> Option<LRESULT> {
        if let Some(i) = self.retiring_menus.iter().position(|m| m.hwnd == hwnd) {
            match msg {
                WM_TIMER => {
                    self.retiring_menus[i].fade_step();
                    if self.retiring_menus[i].exit_finished() { self.retiring_menus.remove(i); defer(move || unsafe { let _ = DestroyWindow(hwnd); }); }
                    return Some(LRESULT(0));
                }
                WM_PAINT => {
                    let mut ps = PAINTSTRUCT::default();
                    unsafe { BeginPaint(hwnd, &mut ps); }
                    self.retiring_menus[i].paint();
                    unsafe { let _ = EndPaint(hwnd, &ps); }
                    return Some(LRESULT(0));
                }
                WM_NCHITTEST => return Some(LRESULT(HTTRANSPARENT as isize)),
                _ => return None,
            }
        }
        let menu = self.menu.as_mut().filter(|m| m.hwnd == hwnd)?;
        match msg {
            WM_PAINT => {
                let mut ps = PAINTSTRUCT::default();
                unsafe { BeginPaint(hwnd, &mut ps) };
                menu.paint();
                unsafe { let _ = EndPaint(hwnd, &ps); }
                Some(LRESULT(0))
            }
            WM_LBUTTONDOWN if menu.begin_drag(lparam) => {
                unsafe { SetCapture(hwnd); let _ = InvalidateRect(Some(hwnd), None, false); }
                Some(LRESULT(0))
            }
            WM_LBUTTONDOWN => { menu.begin_hold(lparam); Some(LRESULT(0)) }
            WM_MOUSEMOVE if menu.dragging() => {
                menu.drag_to(lparam);
                unsafe { let _ = InvalidateRect(Some(hwnd), None, false); }
                Some(LRESULT(0))
            }
            WM_MOUSEMOVE => {
                let part = menu.hit(lparam);
                menu.cancel_hold_if_moved(part);
                if menu.set_hover(part) {
                    unsafe { let _ = InvalidateRect(Some(hwnd), None, false); }
                }
                let mut tme = TRACKMOUSEEVENT { cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: hwnd, dwHoverTime: 0 };
                unsafe { let _ = TrackMouseEvent(&mut tme); }
                Some(LRESULT(0))
            }
            WM_MOUSELEAVE => {
                menu.cancel_hold();
                if menu.set_hover(None) {
                    unsafe { let _ = InvalidateRect(Some(hwnd), None, false); }
                }
                Some(LRESULT(0))
            }
            WM_LBUTTONUP if menu.dragging() => {
                unsafe { let _ = ReleaseCapture(); }
                let level = menu.end_drag();
                unsafe { let _ = InvalidateRect(Some(hwnd), None, false); }
                if let Some(level) = level {
                    self.menu_command(MenuCmd::SetAdblockLevel(level));
                }
                Some(LRESULT(0))
            }
            WM_LBUTTONUP => {
                menu.cancel_hold();
                if let Some(cmd) = menu.hit(lparam).and_then(|p| menu.command(p)) {
                    self.menu_command(cmd);
                }
                Some(LRESULT(0))
            }
            WM_KEYDOWN if wparam.0 as u16 == VK_ESCAPE.0 => {
                self.close_menu();
                self.focus_content();
                Some(LRESULT(0))
            }
            WM_MOUSEWHEEL => { menu.scroll_by(-((wparam.0 >> 16) as u16 as i16 as f32) / 120.0 * 102.0); Some(LRESULT(0)) }
            WM_TIMER if wparam.0 == 2 => {
                if let Some(cmd) = menu.hold_command() { self.menu_command(cmd); }
                Some(LRESULT(0))
            }
            WM_KEYDOWN => {
                if let Some(cmd) = menu.keyboard(wparam.0 as u32) { self.menu_command(cmd); }
                Some(LRESULT(0))
            }
            WM_TIMER => {
                menu.fade_step();
                Some(LRESULT(0))
            }
            WM_ACTIVATE if (wparam.0 & 0xFFFF) as u32 == WA_INACTIVE => {
                self.close_menu();
                Some(LRESULT(0))
            }
            _ => None,
        }
    }

    fn menu_command(&mut self, cmd: MenuCmd) {
        let ctrl = self.tabs[self.active].ctrl.clone();
        let view = self.active_view();
        if self.translation_command(cmd) { return; }
        match cmd {
            MenuCmd::ZoomIn | MenuCmd::ZoomOut => {
                // Yakınlaştırırken menü açık kalır; oran olay gelince güncellenir.
                if let Some(c) = ctrl {
                    let next = menu::zoom_step(self.tabs[self.active].zoom, cmd == MenuCmd::ZoomIn);
                    unsafe { let _ = c.SetZoomFactor(next); }
                }
                return;
            }
            MenuCmd::SetAdblockLevel(level) => {
                // Mod değişince menü açık kalır, sayfa yenilenir.
                self.set_site_level(level);
                let items = self.shield_menu_items();
                if let Some(m) = &mut self.menu {
                    m.set_items(items);
                }
                return;
            }
            MenuCmd::PinExtension(i) => {
                // Sabitleme menüyü kapatmaz.
                self.toggle_pin(i);
                let items = self.extension_menu_items();
                if let Some(m) = &mut self.menu {
                    m.set_items(items);
                }
                return;
            }
            _ => {}
        }
        self.close_menu();
        match cmd {
            MenuCmd::NewTab => self.new_tab(String::new(), None),
            MenuCmd::PrivateTab => self.new_private_tab(),
            MenuCmd::History => self.open_page("history"),
            MenuCmd::Downloads => self.open_page("downloads"),
            MenuCmd::Bookmarks => self.open_page("bookmarks"),
            MenuCmd::Extensions => self.open_page("extensions"),
            MenuCmd::Settings | MenuCmd::ManageProfiles => self.open_page("settings"),
            MenuCmd::NetworkLogin => self.new_tab("http://www.msftconnecttest.com/redirect".into(),None),
            MenuCmd::OpenCompatible(id) => {
                if let Some(target) = self.index_of(id).map(|i| self.tabs[i].url.clone()) {
                    defer(move || { if let Err(error)=crate::integration::open_compatible_browser(&target) { unsafe {
                        MessageBoxW(None,&HSTRING::from(error.to_string()),w!("Sqywein"),MB_OK | MB_ICONERROR);
                    } } });
                }
            }
            MenuCmd::Passwords => self.open_page("passwords"),
            MenuCmd::Print => {
                if let Some(v) = view.and_then(|v| v.cast::<ICoreWebView2_16>().ok()) {
                    unsafe { let _ = v.ShowPrintUI(COREWEBVIEW2_PRINT_DIALOG_KIND_BROWSER); }
                }
            }
            MenuCmd::Find => self.find_in_page(),
            MenuCmd::Translate => self.toggle_translate(),
            MenuCmd::TranslationOptions | MenuCmd::TranslationLanguagePage(_, _) | MenuCmd::TranslationLanguages(_) | MenuCmd::TranslationLanguage(_, _) | MenuCmd::TranslationAlways | MenuCmd::TranslationNever | MenuCmd::TranslationSite | MenuCmd::TranslationSettings | MenuCmd::TranslationOriginal | MenuCmd::TranslationClose => {},
            MenuCmd::OpenShare => { let hwnd = self.hwnd; defer(move || { with_app(|app| app.open_menu(MenuKind::Share)); let _ = hwnd; }); }
            MenuCmd::CopyLink => self.copy_link(),
            MenuCmd::ShareWindows => { defer(|| { with_app(|app| app.share_windows()); }); }
            MenuCmd::EmailLink => self.email_link(),
            MenuCmd::FocusMode => self.toggle_focus_mode(),
            MenuCmd::SwitchMode => self.switch_mode(),
            MenuCmd::Spotlight => self.open_surface(overlay::Surface::Spotlight),
            MenuCmd::Overview => self.open_surface(overlay::Surface::Tabs),
            MenuCmd::PreviewDownload(id) => self.open_surface(overlay::Surface::Download(id)),
            MenuCmd::PictureInPicture => self.picture_in_picture(),
            MenuCmd::SitePermissions => {
                let origin = self.tabs[self.active].url.clone();
                self.new_tab(format!("{}#permissions={}", url::internal("settings"), url::encode(&origin)), None);
            }
            MenuCmd::DevTools => self.run(Cmd::DevTools),
            MenuCmd::SignIn => self.new_tab("https://accounts.google.com/".into(), None),
            MenuCmd::ManageAccount => self.new_tab("https://myaccount.google.com/".into(), None),
            MenuCmd::SwitchProfile(i) => self.switch_profile(i),
            MenuCmd::AddProfile => self.add_profile(None),
            MenuCmd::OpenExtension(i) => {
                if let Some(id) = self.extensions.get(i).map(|e| e.id.clone()) {
                    self.open_extension_popup(&id, None);
                }
            }
            MenuCmd::Store => self.new_tab("https://chromewebstore.google.com/".into(), None),
            MenuCmd::OpenBookmark(i) => self.open_bookmark(i, true),
            MenuCmd::TabPin | MenuCmd::TabMute | MenuCmd::TabNewRight
            | MenuCmd::TabReload
            | MenuCmd::TabDuplicate
            | MenuCmd::TabSleep
            | MenuCmd::TabSleepOthers
            | MenuCmd::TabCloseOthers
            | MenuCmd::TabCloseRight
            | MenuCmd::TabClose => self.tab_command(cmd),
            MenuCmd::SavePassword => self.save_pending_login(false),
            MenuCmd::NeverSavePassword => self.save_pending_login(true),
            MenuCmd::DismissPassword => self.pending_login = None,

            MenuCmd::OpenDownload(id) => self.download_action("downloads.open", id),
            MenuCmd::ShowDownload(id) => self.download_action("downloads.show", id),
            MenuCmd::CancelDownload(id) => self.download_action("downloads.cancel", id),
            MenuCmd::Exit => {
                let hwnd = self.hwnd;
                defer(move || unsafe { let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0)); });
            }
            MenuCmd::ZoomIn | MenuCmd::ZoomOut | MenuCmd::PinExtension(_) | MenuCmd::SetAdblockLevel(_) => {}
        }
    }

    /// Motorun yerleşik "sayfada bul" çubuğu Ctrl+F ile açılır.
    fn find_in_page(&self) {
        let Some(ctrl) = self.tabs[self.active].ctrl.clone() else { return };
        defer(move || unsafe {
            let _ = ctrl.MoveFocus(COREWEBVIEW2_MOVE_FOCUS_REASON_PROGRAMMATIC);
            let key = |vk: VIRTUAL_KEY, up: bool| INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT { wVk: vk, wScan: 0, dwFlags: if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) }, time: 0, dwExtraInfo: 0 },
                },
            };
            let f = VIRTUAL_KEY('F' as u16);
            let inputs = [key(VK_CONTROL, false), key(f, false), key(f, true), key(VK_CONTROL, true)];
            SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
        });
    }

    // ---------- Sekmeler ----------

    fn new_tab(&mut self, url: String, opener: Option<u32>) {
        let private = opener.and_then(|id| self.index_of(id)).is_some_and(|i| self.tabs[i].private);
        self.insert_tab(url, opener, private);
    }

    fn new_private_tab(&mut self) {
        self.insert_tab(String::new(), None, true);
    }

    fn insert_tab(&mut self, url: String, opener: Option<u32>, private: bool) {
        let id = self.next_id;
        self.next_id += 1;
        let mut tab = Tab::new(id, url, String::new());
        tab.opener = opener;
        tab.private = private;
        // Sayfadan açılan sekme açanın hemen yanına, diğerleri sona eklenir.
        let at = opener.and_then(|id| self.index_of(id)).map_or(self.tabs.len(), |i| i + 1);
        let at = at.max(self.tabs.iter().filter(|t| t.pinned).count());
        self.tabs.insert(at, tab);
        if at <= self.active {
            self.active += 1;
        }
        self.dirty = true;
        self.activate(at);
        self.animate();
    }

    /// İç sayfayı açar; zaten açıksa ona geçer.
    fn open_page(&mut self, page: &str) {
        let target = url::internal(page);
        if let Some(i) = self.tabs.iter().position(|t| t.url == target) {
            self.activate(i);
            return;
        }
        if self.tabs[self.active].url.is_empty() && !self.tabs[self.active].private {
            self.navigate(&format!("sqywein://{page}"));
        } else {
            self.new_tab(target, None);
        }
    }

    fn close_tab(&mut self, i: usize) {
        if i >= self.tabs.len() {
            return;
        }
        // Kapanan sekme yerinde sönerek küçülür.
        let l = self.layout();
        if let Some(r) = self.shown_tabs(&l).get(i) {
            let t = &self.tabs[i];
            self.ghosts.push(Ghost { rect: *r, label: t.label(), icon: t.icon.clone(), active: i == self.active, start: Instant::now() });
        }
        let mut tab = self.tabs.remove(i);
        self.hello_pending.retain(|(id, _)| *id != tab.id);
        if let Some(popup) = self.auth_popups.remove(&tab.id) { self.retired_auth.push((Instant::now(), popup)); }
        crate::adblock::forget(tab.id);
        if !tab.url.is_empty() && !tab.private && tab.has_loaded {
            self.closed.push(tab.url.clone());
            if self.closed.len() > 50 { self.closed.remove(0); }
        }
        if !self.keep_for_download(&mut tab) {
            tab.close_view();
        }
        self.dirty = true;

        if self.tabs.is_empty() {
            self.push_tab(String::new(), String::new());
            self.active = 0;
            self.show_active();
        } else if i == self.active {
            // Bu sekmeyi açan sekme hâlâ açıksa oraya dönülür.
            let next = tab.opener.and_then(|id| self.index_of(id)).unwrap_or(i.min(self.tabs.len() - 1));
            self.active = usize::MAX; // Önceki etkin sekme artık yok.
            self.activate(next);
        } else if i < self.active {
            self.active -= 1;
        }
        self.animate();
        self.invalidate();
    }

    fn activate(&mut self, i: usize) {
        if i >= self.tabs.len() {
            return;
        }
        if self.active != i {
            if let Some(prev) = self.tabs.get_mut(self.active) {
                prev.last_active = Instant::now();
                if let Some(ctrl) = &prev.ctrl {
                    unsafe { let _ = ctrl.SetIsVisible(false); }
                }
                if let Some(v) = &prev.view { engine::set_low_memory(v, true); }
            }
        }
        self.close_suggest();
        self.active = i;
        self.animate();
        self.tabs[i].last_active = Instant::now();
        self.dirty = true;
        self.show_active();
        if !self.tabs[i].loading && !self.tabs[i].translated && self.tabs[i].page_lang.is_some() { self.detect_language(i); }
    }

    /// Başlangıç, hata ve web sayfasından hangisinin görüneceğine tek yerden karar verir.
    fn show_active(&mut self) {
        let i = self.active;
        let url = self.tabs[i].url.clone();
        Self::set_text(self.edit, &url::display(&url));
        self.update_title();
        self.layout_children();
        self.invalidate();

        if url.is_empty() {
            self.refresh_widgets();
            self.auto_favorites();
            if let Some(ctrl) = &self.tabs[i].ctrl {
                unsafe { let _ = ctrl.SetIsVisible(false); }
            }
            Self::set_text(self.start_edit, "");
            self.focus_start();
            return;
        }
        if self.tabs[i].error.is_some() {
            if let Some(ctrl) = &self.tabs[i].ctrl {
                unsafe { let _ = ctrl.SetIsVisible(false); }
            }
            return;
        }
        let content = self.px(self.layout().content);
        let was_resting = matches!(self.tabs[i].mem, Mem::Dozing | Mem::Suspending);
        let tab = &mut self.tabs[i];
        match (&tab.ctrl, &tab.view) {
            (Some(ctrl), Some(view)) => {
                engine::set_low_memory(view, false);
                if matches!(tab.mem, Mem::Dozing | Mem::Suspending) {
                    engine::wake(view);
                    tab.mem = Mem::Awake;
                }
                unsafe {
                    let _ = ctrl.SetBounds(content);
                    let _ = ctrl.SetIsVisible(true);
                }
                if tab.needs_load {
                    tab.needs_load = false;
                    unsafe { let _ = view.Navigate(&wide(&tab.url)); }
                }
                self.focus_content();
            }
            _ => self.ensure_view(i),
        }
        if was_resting { self.apply_page_cosmetics(i); }
    }

    /// Motor ortamını (henüz yoksa) başlatır.
    fn ensure_env(&mut self) {
        if self.env.is_some() || self.env_pending {
            return;
        }
        self.env_pending = true;
        let dir = state::root().join("profile");
        let st = &self.state.settings;
        if engine::create_environment(&dir.to_string_lossy(), st.hardware_acceleration, st.privacy_shield).is_err() {
            self.env_pending = false;
            self.tabs[self.active].error = Some(ENGINE_MISSING.into());
            self.invalidate();
        }
    }

    fn engine_profile(&self) -> String {
        state::engine_profile_name(&self.state.current_profile)
    }

    fn ensure_view(&mut self, i: usize) {
        let profile = self.engine_profile();
        let tab = &mut self.tabs[i];
        if tab.ctrl.is_some() || tab.creating {
            return;
        }
        match &self.env {
            Some(env) => {
                tab.creating = true;
                if engine::create_view(env, self.hwnd, tab.id, &profile, tab.private).is_err() {
                    tab.creating = false;
                    tab.error = Some("Sekme açılamadı. Yeniden dene.".into());
                }
            }
            None => self.ensure_env(),
        }
    }

    fn navigate(&mut self, text: &str) {
        self.close_suggest();
        let search = self.state.settings.search_url();
        let Some(target) = url::to_url_with(text, search) else { return };
        let tab = &mut self.tabs[self.active];
        tab.url = target.clone();
        tab.error = None;
        tab.title.clear();
        tab.icon = None;
        self.dirty = true;
        if let Some(view) = &tab.view {
            tab.needs_load = false;
            unsafe { let _ = view.Navigate(&wide(&target)); }
        } else {
            tab.needs_load = true;
        }
        self.show_active();
        self.focus_content();
    }

    /// Kullanılmayan sekmeleri dondurur; "sekmeleri kapat" ayarı açıksa sonra kapatır.
    fn rest_unused_tabs(&mut self) {
        if !self.rest {
            return;
        }
        let (freeze, sleep) = (self.state.settings.freeze_tabs, self.state.settings.sleep_tabs);
        let now = Instant::now();
        let openers: HashSet<u32> = self.tabs.iter().filter(|t| t.window_child).filter_map(|t| t.opener).collect();
        let busy: HashSet<u32> = self.downloads.iter().filter_map(|(id, _)| self.download_owner.get(id).copied()).collect();
        let mut order: Vec<usize> = (0..self.tabs.len()).collect();
        order.sort_by_key(|i| self.tabs[*i].last_active);
        let mut budget = if self.memory_pressure == crate::memory::Pressure::Critical { 4 } else { 2 };
        for i in order {
            let tab = &mut self.tabs[i];
            if i == self.active || tab.window_child || openers.contains(&tab.id) {
                continue;
            }
            if tab.sleep_check || tab.mem == Mem::Suspending || now < tab.next_rest_check { continue; }
            let Some(view) = tab.view.clone() else { continue };
            let idle = now.duration_since(tab.last_active);
            if self.state.settings.never_sleep.iter().any(|h| h == url::host(&tab.url)) { continue; }
            let finished_store = extensions::store_id(&tab.url).is_some_and(|id| self.state.store_installs.contains_key(&id));
            let Some(discard) = tab.rest_action(self.memory_pressure, freeze, sleep, finished_store, idle) else { continue };
            if tab.loading || busy.contains(&tab.id) || engine::plays_audio(&view) { continue; }
            tab.sleep_check = true; tab.sleep_discard = discard;
            tab.next_rest_check = now + Duration::from_secs(30);
            engine::check_rest(&view, tab.id, tab.epoch, discard);
            budget -= 1;
            if budget == 0 { break; }
        }
    }

    // ---------- Motor olayları ----------

    fn on_engine(&mut self, e: Event) {
        match e {
            Event::EnvReady(env) => {
                self.env_pending = false;
                match env {
                    Some(env) => {
                        let version = engine::string(|p| unsafe { env.BrowserVersionString(p) });
                        self.browser_major = version.split('.').next().and_then(|m| m.parse().ok()).unwrap_or(0);
                        self.env = Some(env);
                        if self.tabs.get(self.active).is_some_and(|t| !t.url.is_empty() && t.error.is_none()) {
                            self.ensure_view(self.active);
                        }
                        self.refresh_account(false);
                        if let Some(popup) = &self.popup { if popup.ctrl.is_none() { let _ = engine::create_view(self.env.as_ref().unwrap(), popup.hwnd, POPUP_VIEW, &self.engine_profile(), false); } }
                    }
                    None => {
                        self.tabs[self.active].error = Some(ENGINE_MISSING.into());
                        self.show_active();
                    }
                }
            }
            Event::Suspended(id, epoch, ok) => {
                if let Some(i) = self.index_of(id) {
                    let t = &mut self.tabs[i];
                    if t.epoch == epoch {
                        if i == self.active || t.mem != Mem::Suspending {
                            if let Some(v) = &t.view { engine::wake(v); }
                        } else {
                            t.mem = if ok { Mem::Dozing } else { Mem::Awake };
                            crate::log::line(&format!("DONDUR #{id} {ok}"));
                        }
                    }
                }
            }
            Event::SleepChecked(id, epoch, discard, safe) => {
                let busy = self.download_owner.iter().any(|(dl, owner)| *owner == id && self.downloads.iter().any(|(d, _)| d == dl));
                if let Some(i) = self.index_of(id) {
                    let t = &mut self.tabs[i];
                    if t.epoch == epoch && t.sleep_check && t.sleep_discard == discard {
                        t.sleep_check = false;
                        t.mem = Mem::Awake;
                        if safe && !busy && i != self.active && !t.loading && !self.state.settings.never_sleep.iter().any(|h| h == url::host(&t.url)) {
                            if discard {
                                t.close_view();
                                crate::log::line(&format!("UYUT #{id}"));
                            } else if let Some(v) = &t.view {
                                t.mem = Mem::Suspending; engine::doze(v, id, epoch);
                            }
                        }
                    }
                }
            }
            Event::ViewReady(id, ctrl) => self.on_view_ready(id, ctrl),
            Event::NavStarting(id, uri) => {
                self.hello_pending.retain(|(tab, _)| *tab != id);
                if let Some(i) = self.index_of(id) {
                    let t = &mut self.tabs[i];
                    t.epoch += 1; t.sleep_check = false; t.external_launch = None;
                    if let Some(cancel) = t.tr_cancel.take() { cancel.store(true, std::sync::atomic::Ordering::Relaxed); }
                    t.page_lang = None; t.translated = false; t.tr_pending = 0; t.tr_generation = t.tr_generation.wrapping_add(1); t.tr_error = false; t.tr_collecting = false; t.tr_limited = false; t.tr_declined = false;
                }
                if let Some(i) = self.index_of(id) {
                    self.on_nav_starting_adblock(i, &uri);
                    if !self.tabs[i].loading { self.tabs[i].load_started = Some(Instant::now()); }
                    self.tabs[i].loading = true;
                    self.animate(); // Yükleme halkası dönsün.
                    self.invalidate();
                }
            }
            Event::ExternalLaunch(id) => {
                if let Some(i) = self.index_of(id) { self.tabs[i].external_launch = Some(Instant::now()); }
            }
            Event::NavCompleted(id, ok, status) => {
                if id == ACCOUNT_VIEW {
                    self.on_account_page_loaded(ok);
                    return;
                }
                let Some(i) = self.index_of(id) else { return };
                let tab = &mut self.tabs[i];
                tab.loading = false;
                if !ok && status == COREWEBVIEW2_WEB_ERROR_STATUS_CONNECTION_ABORTED
                    && tab.external_launch.take().is_some_and(|t| t.elapsed() < Duration::from_secs(60)) {
                    tab.error = None; self.invalidate(); return;
                }
                if ok {
                    tab.has_loaded = true;
                }
                if ok { self.detect_language(i); }
                let tab = &mut self.tabs[i];
                crate::log::line(&format!("YÜKLENDİ #{id} {} {}", if ok { "tamam" } else { "hata" }, status.0));
                let (u, t, private) = (tab.url.clone(), tab.title.clone(), tab.private);
                if ok && !u.is_empty() && !private && !url::is_internal(&u) {
                    self.history.record(&u, &t);
                }
                // Google'da oturum açılıp kapanınca profildeki hesap güncellensin.
                if ok && !private && url::host(&u).ends_with("google.com") && u.contains("accounts.google.com") {
                    self.refresh_account(true);
                }
                // Gizleme kuralları belge oluşurken kaydedilir; motor o belgeye yetişemediyse
                // yüklenen sayfaya doğrudan da uygulanır (betik kendini tekrar etmez).
                if ok {
                    self.apply_page_cosmetics(i);
                }
                let tab = &mut self.tabs[i];
                if !ok {
                    if let Some(message) = engine::describe_error(status) {
                        tab.error = Some(message.into());
                        if i == self.active {
                            self.show_active();
                        }
                    }
                }
                self.invalidate();
            }
            Event::SourceChanged(id) => {
                if id == POPUP_VIEW && matches!(self.surface, Some(overlay::Surface::Link(_))) {
                    if let Some(popup) = &mut self.popup {
                        if let Some(view) = popup.ctrl.as_ref().and_then(|c| unsafe { c.CoreWebView2().ok() }) {
                            let source = engine::string(|p| unsafe { view.Source(p) });
                            if source.starts_with("http://") || source.starts_with("https://") {
                                popup.url = source.clone(); self.surface = Some(overlay::Surface::Link(source));
                                unsafe { let _ = InvalidateRect(Some(popup.hwnd), None, false); }
                            }
                        }
                    }
                    return;
                }
                let Some(i) = self.index_of(id) else { return };
                let Some(view) = self.tabs[i].view.clone() else { return };
                let source = engine::string(|p| unsafe { view.Source(p) });
                if source.is_empty() || source == "about:blank" && self.tabs[i].url.is_empty() {
                    return;
                }
                if url::host(&source) != url::host(&self.tabs[i].url) {
                    self.tabs[i].icon = None;
                }
                self.tabs[i].url = source.clone();
                self.dirty = true;
                self.apply_zoom(i);
                if i == self.active && unsafe { GetFocus() } != self.edit {
                    Self::set_text(self.edit, &url::display(&source));
                }
                self.layout_children();
                self.invalidate();
            }
            Event::TitleChanged(id) => {
                let Some(i) = self.index_of(id) else { return };
                let Some(view) = self.tabs[i].view.clone() else { return };
                self.tabs[i].title = engine::string(|p| unsafe { view.DocumentTitle(p) });
                let (u, t) = (self.tabs[i].url.clone(), self.tabs[i].title.clone());
                if !self.tabs[i].private {
                    self.history.update_title(&u, &t);
                }
                self.dirty = true;
                if i == self.active {
                    self.update_title();
                }
                self.invalidate();
            }
            Event::HistoryChanged(id) => {
                let Some(i) = self.index_of(id) else { return };
                let Some(view) = self.tabs[i].view.clone() else { return };
                self.tabs[i].can_back = engine::flag(|b| unsafe { view.CanGoBack(b) });
                self.tabs[i].can_forward = engine::flag(|b| unsafe { view.CanGoForward(b) });
                self.invalidate();
            }
            Event::NewWindow(id, uri, request) => {
                crate::log::line(&format!("POPUP REQUEST #{id} {uri}"));
                // Keep the extension document alive while its authentication child
                // exchanges postMessage/WindowProxy results with the opener.
                let popup = if id == POPUP_VIEW { self.popup.take() } else { None };
                if let Some(p) = &popup { let hwnd = p.hwnd; defer(move || unsafe { let _ = ShowWindow(hwnd, SW_HIDE); }); }
                self.new_tab(if uri.is_empty() { "about:blank".into() } else { uri }, Some(id));
                let tab = &mut self.tabs[self.active];
                tab.needs_load = false;
                tab.window_child = true;
                tab.window_request = Some(request);
                if tab.error.is_some() { tab.window_request = None; }
                if let Some(p) = popup { self.auth_popups.insert(tab.id, p); }
            }
            Event::WindowReady(id, ok) => {
                crate::log::line(&format!("POPUP READY #{id} {ok}"));
                if let Some(i) = self.index_of(id) {
                    let request = self.tabs[i].window_request.take();
                    if let (true, Some(request), Some(view)) = (ok, request, self.tabs[i].view.clone()) {
                        if let Err(error) = request.attach(&view) { crate::log::line(&format!("POPUP ATTACH ERROR #{id} {error}")); self.tabs[i].error = Some("Açılır pencere bağlanamadı. Yeniden dene.".into()); }
                        else { crate::log::line(&format!("POPUP ATTACHED #{id}")); }
                    } else { self.tabs[i].error = Some("Açılır pencere hazırlanamadı.".into()); }
                    if i == self.active { self.show_active(); }
                }
            }
            Event::CloseRequested(id) => {
                crate::log::line(&format!("POPUP CLOSE #{id}"));
                if let Some(i) = self.index_of(id) { self.close_tab(i); }
                else if id == POPUP_VIEW { self.close_popup(); }
            }
            Event::ProcessFailed(id, kind) => {
                crate::log::line(&format!("PROCESS FAILURE #{id} {}",kind.0));
                if kind == COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED {
                    // Motor çöktü: sekmeler kaybolmaz, görünümler atılır, etkin sekme yeniden açılır.
                    self.env = None;
                    self.env_pending = false;
                    self.account_view = None;
                    for t in &mut self.tabs {
                        t.close_view();
                    }
                    self.show_active();
                } else if let Some(i) = self.index_of(id) {
                    if let Some(view) = &self.tabs[i].view {
                        unsafe { let _ = view.Reload(); }
                    }
                }
            }
            Event::LinkPreview(id, target) => {
                if self.index_of(id) == Some(self.active) && (target.starts_with("https://") || target.starts_with("http://")) {
                    self.open_surface(overlay::Surface::Link(target));
                }
            }
            Event::Shortcut(id, vk) => {
                if id == POPUP_VIEW && vk == VK_ESCAPE.0 as u32 { self.close_popup(); self.focus_content(); return; }
                if let Some(cmd) = keys::command(vk, keys::modifiers(), false) {
                    self.run(cmd);
                }
            }
            Event::FullScreen(id) => {
                if self.index_of(id) == Some(self.active) {
                    if let Some(view) = self.active_view() {
                        let on = engine::flag(|b| unsafe { view.ContainsFullScreenElement(b) });
                        self.set_full_screen(on);
                    }
                }
            }
            Event::FaviconChanged(id) => {
                if let Some(view) = self.index_of(id).and_then(|i| self.tabs[i].view.clone()) {
                    engine::request_favicon(&view, id);
                }
            }
            Event::Favicon(id, img, bytes) => {
                if let Some(i) = self.index_of(id) {
                    if !self.tabs[i].private {
                        let page = self.tabs[i].url.clone();
                        self.store_favicon(&page, img.as_ref(), &bytes);
                    }
                    self.tabs[i].icon = img.map(Rc::new);
                    self.invalidate();
                }
            }
            Event::Cookies(profile, generation, cookies) => self.on_cookies(&profile, generation, cookies),
            Event::DataCleared(profile, tab, ok) => {
                self.clearing_data.remove(&profile);
                if let Some(id) = tab {
                    engine::push(Event::PageReply(id, serde_json::json!({"type":"data.clear","ok":ok}).to_string()));
                }
            }
            Event::Cosmetic(id, classes, ids) => self.on_cosmetic(id, classes, ids),
            Event::ScriptAdded(id, slot, generation, script) => self.on_script_added(id, slot, generation, script),
            Event::StoreInstall(id) => self.install_store(id),
            Event::Password(id, frame, source, json) => self.on_password_message(id, frame, &source, &json),
            Event::Zoom(id) => {
                if let Some(i) = self.index_of(id) {
                    self.on_zoom_changed(i);
                }
            }
            Event::PageReply(id, json) => {
                if let Some(v) = self.index_of(id).and_then(|i| self.tabs[i].view.as_ref()) {
                    if url::is_internal(&engine::string(|p| unsafe { v.Source(p) })) { engine::post_json(v, &json); }
                }
            }
            Event::OcrRequested(id) => {
                if let Some(i) = self.index_of(id).filter(|i| *i == self.active) {
                    self.ocr_request = self.ocr_request.wrapping_add(1);
                    if let Some(view) = &self.tabs[i].view {
                        let script = format!(r#"(()=>{{const p=window.__sqImagePoint,n=p&&document.elementFromPoint(...p);if(!n||n.tagName!=='IMG')return null;const r=n.getBoundingClientRect();return {{k:'ocr',e:{},request:{},rect:[r.x/innerWidth,r.y/innerHeight,r.width/innerWidth,r.height/innerHeight]}};}})()"#, self.tabs[i].epoch, self.ocr_request);
                        engine::run_script(view, id, &script);
                    }
                }
            }
            Event::OcrCaptured(id, epoch, request, image) => {
                if self.index_of(id) == Some(self.active) && self.tabs[self.active].epoch == epoch && self.ocr_request == request {
                    if let Some(image) = image { crate::ocr::recognize(image, id, epoch, request); }
                    else { self.open_surface(overlay::Surface::Notice("Görüntü yakalanamadı. Resmi görünür hale getirip tekrar dene.".into())); }
                }
            }
            Event::Thumbnail(generation, id, image) => self.on_thumbnail(generation, id, image),
            Event::WebMessage(id, json) => self.on_web_message(id, &json),
            Event::DownloadStarted(id, op) => self.on_download_started(id, op),
            Event::DownloadChanged(id) => self.on_download_changed(id),
            Event::Script(id, result) => {
                if id == ACCOUNT_VIEW {
                    self.on_account_json(&result);
                } else {
                    self.on_tab_script(id, &result);
                }
            }
            Event::Extensions(list) => self.on_extensions(list),
            Event::ExtensionAdded(id) => self.on_extension_added(id),
        }
    }

    fn on_view_ready(&mut self, id: u32, ctrl: Option<ICoreWebView2Controller>) {
        if id == ACCOUNT_VIEW {
            self.on_account_view_ready(ctrl);
            return;
        }
        if id == POPUP_VIEW {
            self.on_popup_view_ready(ctrl);
            return;
        }
        let Some(i) = self.index_of(id) else {
            // Sekme bu arada kapatıldı.
            if let Some(c) = ctrl {
                unsafe { let _ = c.Close(); }
            }
            return;
        };
        self.tabs[i].creating = false;
        let Some(ctrl) = ctrl else {
            self.tabs[i].error = Some(ENGINE_MISSING.into());
            if i == self.active {
                self.show_active();
            }
            return;
        };
        let view = match engine::configure(&ctrl, id, 0xFFFFFF, &self.state.settings) {
            Ok(v) => v,
            Err(_) => {
                unsafe { let _ = ctrl.Close(); }
                self.tabs[i].error = Some("Sekme açılamadı. Yeniden dene.".into());
                return;
            }
        };
        unsafe { let _ = ctrl.SetIsVisible(false); }
        if !self.tabs[i].private {
            self.restore_session_cookies(&view);
        }
        let tab = &mut self.tabs[i];
        tab.ctrl = Some(ctrl);
        engine::set_muted(&view, tab.muted);
        tab.view = Some(view);
        tab.mem = Mem::Awake;
        self.attach_adblock(i);
        self.apply_zoom(i);
        if self.extensions.is_empty() {
            self.refresh_extensions();
        }
        if self.tabs[i].window_request.is_some() {
            if let Some(v) = &self.tabs[i].view { engine::prepare_window(v, id); }
            return;
        }
        let tab = &mut self.tabs[i];
        if i == self.active {
            self.show_active();
        } else if tab.needs_load {
            tab.needs_load = false;
            if let Some(v) = &tab.view {
                unsafe { let _ = v.Navigate(&wide(&tab.url)); }
            }
        }
    }

    // ---------- Tam ekran ----------

    fn set_full_screen(&mut self, on: bool) {
        if on == self.full_screen.is_some() {
            return;
        }
        let hwnd = self.hwnd;
        if on {
            let mut placement = WINDOWPLACEMENT { length: std::mem::size_of::<WINDOWPLACEMENT>() as u32, ..Default::default() };
            unsafe { let _ = GetWindowPlacement(hwnd, &mut placement); }
            self.full_screen = Some(FullScreen { placement });
            defer(move || unsafe {
                let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
                let mut info = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
                let _ = GetMonitorInfoW(monitor, &mut info);
                let r = info.rcMonitor;
                SetWindowLongPtrW(hwnd, GWL_STYLE, (WS_POPUP | WS_VISIBLE | WS_CLIPCHILDREN).0 as isize);
                let _ = SetWindowPos(hwnd, Some(HWND_TOP), r.left, r.top, r.right - r.left, r.bottom - r.top, SWP_FRAMECHANGED | SWP_NOOWNERZORDER);
            });
        } else if let Some(fs) = self.full_screen.take() {
            defer(move || unsafe {
                SetWindowLongPtrW(hwnd, GWL_STYLE, (WS_OVERLAPPEDWINDOW | WS_VISIBLE | WS_CLIPCHILDREN).0 as isize);
                let _ = SetWindowPlacement(hwnd, &fs.placement);
                let _ = SetWindowPos(hwnd, None, 0, 0, 0, 0, SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOOWNERZORDER);
            });
        }
        self.layout_children();
        self.invalidate();
    }

    // ---------- Metin kutuları ve odak ----------

    fn text_of(edit: HWND) -> String {
        unsafe {
            let len = GetWindowTextLengthW(edit);
            let mut buf = vec![0u16; len as usize + 1];
            let n = GetWindowTextW(edit, &mut buf);
            String::from_utf16_lossy(&buf[..n as usize])
        }
    }

    fn set_text(edit: HWND, text: &str) {
        SUPPRESS_CHANGE.with(|c| c.set(true));
        unsafe { let _ = SetWindowTextW(edit, &wide(text)); }
        SUPPRESS_CHANGE.with(|c| c.set(false));
    }

    // Odak yalnızca pencere öndeyken taşınır: arkadaki bir pencerede SetFocus ya da
    // MoveFocus pencereyi öne çıkarır ve kullanıcının başka yerde yazdıklarını
    // çalar. Pencere etkinleşince WM_SETFOCUS odağı zaten yerine koyar.

    fn focus_address(&self) {
        let (hwnd, edit) = (self.hwnd, self.edit);
        defer(move || unsafe {
            if GetForegroundWindow() == hwnd {
                let _ = SetFocus(Some(edit));
                SendMessageW(edit, EM_SETSEL, Some(WPARAM(0)), Some(LPARAM(-1)));
            }
        });
    }

    fn focus_start(&self) {
        let (hwnd, edit) = (self.hwnd, self.start_edit);
        defer(move || unsafe {
            if GetForegroundWindow() == hwnd {
                let _ = SetFocus(Some(edit));
            }
        });
    }

    fn focus_content(&self) {
        let Some(tab) = self.tabs.get(self.active) else { return };
        if tab.url.is_empty() {
            self.focus_start();
        } else if let Some(ctrl) = tab.ctrl.clone() {
            let hwnd = self.hwnd;
            defer(move || unsafe {
                if GetForegroundWindow() == hwnd {
                    let _ = ctrl.MoveFocus(COREWEBVIEW2_MOVE_FOCUS_REASON_PROGRAMMATIC);
                }
            });
        }
    }

    fn update_title(&self) {
        let title = format!("{} — Sqywein", self.tabs[self.active].label());
        let hwnd = self.hwnd;
        defer(move || unsafe { let _ = SetWindowTextW(hwnd, &wide(&title)); });
    }

    // ---------- Kayıt ----------

    /// Gizli sekmeler oturuma yazılmaz.
    fn session_tabs(&self) -> (Vec<state::SavedTab>, i32) {
        let mut active = 0;
        let mut out = Vec::new();
        for (i, t) in self.tabs.iter().enumerate() {
            if t.private {
                continue;
            }
            if i == self.active {
                active = out.len() as i32;
            }
            out.push(state::SavedTab { pinned: t.pinned, title: t.title.clone(), url: t.url.clone() });
        }
        (out, active)
    }

    fn save(&mut self) {
        let (tabs, active) = self.session_tabs();
        self.state.tabs = tabs;
        self.state.active_index = active;
        if state::save(&self.state).is_ok() {
            self.dirty = false;
        }
        self.state.tabs.clear();
    }

    // ---------- Çizim ----------

    fn paint(&mut self) {
        if self.width == 0 || self.height == 0 || !self.gfx.begin(self.hwnd, self.width, self.height) {
            return;
        }
        let l = self.layout();
        let g = &self.gfx;
        if crate::theme::glass() { g.clear_transparent(); } else { g.clear(base()); }
        if self.focus_mode && self.chrome_hidden() && self.full_screen.is_none() {
            let (c, a) = toolbar_tint();
            g.fill_alpha(Rect::new(0.0, 0.0, self.width as f32 / self.scale(), FOCUS_STRIP), c, a);
        }
        if !self.chrome_hidden() {
            let w = self.width as f32 / self.scale();
            // Cam katmanları: araç çubuğu, sekme şeridi ve aralarında ince çizgiler.
            let (c, a) = toolbar_tint();
            g.fill_alpha(Rect::new(0.0, 0.0, w, TOOL_ROW), c, a);
            let (c, a) = tabbar_tint();
            g.fill_alpha(Rect::new(0.0, TOOL_ROW, w, TAB_ROW), c, a);
            g.fill_alpha(Rect::new(0.0, TOOL_ROW, w, 1.0), line(), if is_dark() { 0.55 } else { 0.8 });
            if let Some(bar) = l.bar {
                let (c, a) = toolbar_tint();
                g.fill_alpha(bar, c, a);
            }
            g.fill_alpha(Rect::new(0.0, l.content.y - 1.0, w, 1.0), line(), if is_dark() { 0.7 } else { 1.0 });
            self.paint_tabs(&l);
            self.paint_caption(&l);
            self.paint_toolbar(&l);
            self.paint_bar(&l);
        }
        self.paint_content(&l);
        self.paint_default_prompt(&l);
        self.gfx.end();
    }

    fn paint_content(&self, l: &Layout) {
        let g = &self.gfx;
        let tab = &self.tabs[self.active];
        if tab.url.is_empty() {
            let (c, a) = page_tint();
            g.fill_alpha(l.content, c, a);
            self.paint_start(l);
        } else if let Some(err) = &tab.error {
            g.round(l.content, 10.0, raised());
            let (cx, cy) = (l.content.x, l.content.cy());
            if let Some(m) = &self.mascot {
                let h = 120.0;
                let w = h * m.width as f32 / m.height as f32;
                g.image(m, Rect::new(l.content.cx() - w / 2.0, cy - 60.0 - h, w, h), false);
            }
            g.text("Sayfa açılamadı", Rect::new(cx, cy - 40.0, l.content.w, 28.0), Font::UiBold, ink(), Align::Center);
            g.text(err, Rect::new(cx, cy - 10.0, l.content.w, 24.0), Font::Ui, muted(), Align::Center);
            g.round(l.retry, 10.0, if self.hover == Hit::Retry { ink() } else { accent() });
            g.text("Yeniden dene", l.retry, Font::Ui, base(), Align::Center);
        } else {
            // Sayfanın altı opak: görünüm hazırlanırken cam görünmesin.
            g.fill(l.content, if tab.ctrl.is_none() { raised() } else { base() });
        }
    }

    /// Sekme simgesi: gerçek site simgesi, yüklenirken halka, yoksa baş harf.
    #[allow(clippy::too_many_arguments)] // Mirrors the existing paint_tab drawing interface.
    fn paint_badge(&self, badge: Rect, label: &str, icon: Option<&Image>, loading: bool, active: bool, dim: bool, private: bool) {
        let g = &self.gfx;
        if loading {
            let turn = self.t0.elapsed().as_secs_f32() / 0.9;
            g.spinner(badge.cx(), badge.cy(), 6.5, turn.fract(), accent(), 2.0);
        } else if let Some(img) = icon {
            g.image(img, badge, false);
        } else {
            g.round(badge, 5.0, if private { accent() } else if active { line() } else { hover() });
            let letter: String = label.chars().next().map(|c| c.to_uppercase().collect()).unwrap_or_default();
            g.text(&letter, badge, Font::Small, if private { base() } else if dim { faint() } else { ink() }, Align::Center);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn paint_tab(&self, r: Rect, tab: Option<&Tab>, label: &str, icon: Option<&Image>, active: bool, hovered: bool, close_hover: bool) {
        let g = &self.gfx;
        let private = tab.is_some_and(|t| t.private);
        if active {
            if tab.is_none() { let (c, a) = tab_active(); g.round_alpha(r, 7.0, c, a); }
            if private {
                g.round_outline_alpha(r, 7.0, accent(), 1.0, 0.7);
            }
        } else if hovered {
            g.round_alpha(r, 7.0, hover(), if is_dark() { 0.55 } else { 0.7 });
        }
        if r.w < 24.0 {
            return;
        }
        let loading = tab.is_some_and(|t| t.loading);
        let asleep = tab.is_some_and(|t| t.ctrl.is_none() && !t.url.is_empty());
        let show_close = (active || hovered) && r.w > 70.0;
        // Safari: simge ve başlık ortada; sığmazsa soldan başlar. Kapat düğmesi solda.
        let left = r.x + if show_close { 28.0 } else { 10.0 };
        let right = r.right() - 10.0;
        let room = (right - left - 22.0).max(0.0);
        let text_w = g.measure(label, Font::Small).min(room);
        let group = 22.0 + text_w;
        let gx = (r.cx() - group / 2.0).max(left).min((right - group).max(left));
        let badge = Rect::new(gx, r.cy() - 8.0, 16.0, 16.0);
        self.paint_badge(badge, label, icon, loading, active, asleep, private);
        if text_w > 8.0 {
            let color = if active { ink() } else if asleep { faint() } else { muted() };
            g.text(label, Rect::new(badge.right() + 6.0, r.y, text_w + 2.0, r.h), Font::Small, color, Align::Left);
        }
        if show_close {
            let close = tab_close_rect(r);
            if close_hover {
                g.round(close, 5.0, line());
            }
            g.text(ICON_CLOSE, close, Font::SmallIcon, muted(), Align::Center);
        }
    }

    fn paint_tabs(&self, l: &Layout) {
        let g = &self.gfx;
        // Kapanan sekmelerin izleri: sönerek daralır.
        for ghost in &self.ghosts {
            let p = ease_out(ghost.progress());
            let r = Rect::new(ghost.rect.x, ghost.rect.y, ghost.rect.w * (1.0 - p), ghost.rect.h);
            g.set_opacity(1.0 - p);
            self.paint_tab(r, None, &ghost.label, ghost.icon.as_deref(), ghost.active, false, false);
        }
        g.set_opacity(1.0);
        if let Some(r) = l.mode {
            // Gizli moddayken vurgu renginde dolu; normal modda gizli sekme sayısıyla.
            let private = self.tabs[self.active].private;
            if private { g.round(r, 7.0, accent()); } else { self.glass_chip(r, self.hover_amount(Hit::Mode).max(0.0)); }
            g.text(ICON_PRIVATE, r, Font::MidIcon, if private { base() } else { ink() }, Align::Center);
            if !private {
                let count = self.tabs.iter().filter(|t| t.private).count();
                // Rozet: normal yazı tipiyle (simge yazı tipinde rakam yoktur), 9'dan fazlası "9+".
                let label = if count > 9 { "9+".to_string() } else { count.to_string() };
                let bw = if count > 9 { 22.0 } else { 16.0 };
                let b = Rect::new(r.right() - bw + 4.0, r.y - 3.0, bw, 16.0);
                g.round(b.inset(-1.5, -1.5), 9.5, tabbar_tint().0);
                g.round(b, 8.0, system_blue());
                g.text(&label, b, Font::Small, 0xFFFFFF, Align::Center);
            }
        }
        let shown = self.shown_tabs(l);
        // Sürüklenen sekme en üstte, kendi kartıyla çizilir; komşuların yazısı onun üstüne binmez.
        let dragged = self.dragging_id().and_then(|id| self.index_of(id));
        let card_style = |card: Rect| {
            let (c, a) = tab_active();
            g.round_alpha(card, 7.0, c, a);
            g.round_outline_alpha(card, 7.0, if is_dark() { 0xFFFFFF } else { 0x000000 }, 1.0, if is_dark() { 0.08 } else { 0.07 });
        };
        // Etkin sekme: kayarak yer değiştiren yumuşak cam kart.
        if let Some(target) = shown.get(self.active).filter(|_| dragged != Some(self.active)) {
            let x = if self.selection_x.is_finite() && motion_enabled() { self.selection_x } else { target.x };
            let w = if self.selection_w.is_finite() && motion_enabled() { self.selection_w } else { target.w };
            card_style(Rect::new(x, target.y, w, target.h));
        }
        // Sürüklerken yalnızca tutulan sekme "üzerinde" sayılır (sıra değişse de).
        let hovered_at = |i: usize| match dragged {
            Some(d) => i == d,
            None => matches!(self.hover, Hit::Tab(h) | Hit::TabClose(h) if h == i),
        };
        // Etkin ya da üzerine gelinen sekmenin yanında olmayan ince ayraçlar.
        let busy = |i: usize| i == self.active || hovered_at(i);
        for (i, r) in shown.iter().enumerate().skip(1) {
            if !busy(i) && !busy(i - 1) && r.w > 24.0 {
                g.fill_alpha(Rect::new(r.x - 1.0, r.y + 8.0, 1.0, r.h - 16.0), line(), if is_dark() { 0.9 } else { 1.0 });
            }
        }
        let label_of = |tab: &Tab| {
            // Henüz yüklenmemiş (uyuyan) sekmeler de diskteki simgesiyle görünür.
            if tab.private { tab.icon.clone() } else { tab.icon.clone().or_else(|| self.icon_for(&tab.url)) }
        };
        for (i, (tab, r)) in self.tabs.iter().zip(&shown).enumerate() {
            if Some(i) == dragged { continue; }
            let asleep = tab.ctrl.is_none() && !tab.url.is_empty() && i != self.active;
            let hovered = hovered_at(i);
            g.set_opacity(tab.open_progress() * if asleep && !hovered { 0.55 } else { 1.0 });
            let close_hover = dragged.is_none() && self.hover == Hit::TabClose(i);
            self.paint_tab(*r, Some(tab), &tab.label(), label_of(tab).as_deref(), i == self.active, hovered, close_hover);
        }
        g.set_opacity(1.0);
        if let Some(d) = dragged {
            let tab = &self.tabs[d];
            let r = shown[d];
            // Altındakini tamamen örten zemin, hafif gölge ve kart.
            g.glow(r.cx(), r.cy() + 3.0, r.w * 0.5 + 6.0, r.h * 0.5 + 6.0, 0x000000, if is_dark() { 0.45 } else { 0.18 });
            let (c, _) = tabbar_tint();
            g.round(r, 7.0, c);
            card_style(r);
            self.paint_tab(r, Some(tab), &tab.label(), label_of(tab).as_deref(), d == self.active, true, false);
        }
    }

    fn paint_caption(&self, l: &Layout) {
        let g = &self.gfx;
        let maximized = unsafe { IsZoomed(self.hwnd) }.as_bool();
        for (r, hit, icon) in [
            (l.min, Hit::Min, ICON_MIN),
            (l.max, Hit::Max, if maximized { ICON_RESTORE } else { ICON_MAX }),
            (l.close, Hit::Close, ICON_CLOSE),
        ] {
            // Araç çubuğundaki düğmelerle aynı boy ve aynı orta çizgi.
            let mut b = Rect::new(r.cx() - 18.0, TOOL_ROW / 2.0 - 16.0, 36.0, 32.0);
            if self.pressed == hit { b = b.inset(1.0, 1.0); }
            let amount = self.hover_amount(hit);
            if amount > 0.0 {
                if hit == Hit::Close {
                    g.round_alpha(b, 8.0, if self.pressed == hit { 0xC42B1C } else { 0xE5484D }, amount);
                } else {
                    self.glass_chip(b, amount);
                }
            }
            let color = if hit == Hit::Close && amount > 0.5 { 0xFFFFFF } else if amount > 0.0 { ink() } else { muted() };
            g.text(icon, b, Font::SmallIcon, color, Align::Center);
        }
    }

    /// Yumuşak kenarlı cam zemin: üzerine gelinen düğmeler ve camdaki alanlar için.
    fn glass_chip(&self, r: Rect, amount: f32) {
        let g = &self.gfx;
        if is_dark() {
            g.round_alpha(r, 8.0, 0xFFFFFF, 0.12 * amount);
            g.round_outline_alpha(r, 8.0, 0xFFFFFF, 1.0, 0.08 * amount);
        } else {
            g.round_alpha(r, 8.0, 0x000000, 0.07 * amount);
            g.round_outline_alpha(r, 8.0, 0x000000, 1.0, 0.05 * amount);
        }
    }

    /// Adres alanında yazmıyorken gösterilen kısa ad: alan adı ya da sayfa adı.
    fn address_label(&self, tab: &Tab) -> String {
        if tab.url.is_empty() { return "Ara veya adres yaz".into(); }
        if url::is_internal(&tab.url) {
            let page = tab.url.trim_start_matches("https://sqywein.ui/");
            return format!("sqywein://{}", page.split(['?', '#']).next().unwrap_or(page));
        }
        let host = url::host(&tab.url);
        if host.is_empty() { tab.url.clone() } else { host.trim_start_matches("www.").to_string() }
    }

    fn hover_amount(&self, hit: Hit) -> f32 {
        if !motion_enabled() { return if self.hover == hit { 1.0 } else { 0.0 }; }
        if self.hover == hit { self.hover_progress } else if self.previous_hover == hit { 1.0 - self.hover_progress } else { 0.0 }
    }
    /// Adres alanının içindeki düğmeler: alan rengine göre belirgin, yumuşak geçişli.
    fn field_button(&self, r: Rect, hit: Hit) {
        let amount = self.hover_amount(hit);
        if amount > 0.0 {
            self.gfx.round_alpha(r, 7.0, if is_dark() { 0x5A5A5E } else { 0xD1D1D6 }, amount);
        }
    }

    fn tool_button(&self, mut r: Rect, hit: Hit, active: bool) {
        let amount = if active { 1.0 } else { self.hover_amount(hit) };
        if self.pressed == hit { r = Rect::new(r.x + 1.0, r.y + 1.0, (r.w - 2.0).max(0.0), (r.h - 2.0).max(0.0)); }
        if amount > 0.0 { self.glass_chip(r, amount); }
    }

    fn paint_toolbar(&self, l: &Layout) {
        let g = &self.gfx;
        let tab = &self.tabs[self.active];
        for (r, hit, icon, enabled) in [
            (l.back, Hit::Back, ICON_BACK, tab.can_back),
            (l.forward, Hit::Forward, ICON_FORWARD, tab.can_forward),
            (l.home, Hit::Home, ICON_HOME, true),
            (l.new_tab, Hit::NewTab, ICON_ADD, true),
        ] {
            if enabled {
                self.tool_button(r, hit, false);
            }
            g.text(icon, r, Font::Icon, if enabled { ink() } else { faint() }, Align::Center);
        }

        // Adres alanı (Safari): opak dolgu, odakta vurgu renginde halka.
        let focused = unsafe { GetFocus() } == self.edit;
        let editing = focused || tab.url.is_empty();
        if focused {
            g.round_alpha(Rect::new(l.address.x - 3.0, l.address.y - 3.0, l.address.w + 6.0, l.address.h + 6.0), 12.0, accent(), 0.35);
        }
        if editing {
            g.round(l.address, 9.0, address_field());
        } else {
            let lift = self.hover_amount(Hit::Address);
            if is_dark() {
                g.round_alpha(l.address, 9.0, 0xFFFFFF, 0.09 + 0.05 * lift);
                g.round_outline_alpha(l.address, 9.0, 0xFFFFFF, 1.0, 0.07 + 0.05 * lift);
            } else {
                g.round_alpha(l.address, 9.0, 0x000000, 0.055 + 0.03 * lift);
                g.round_outline_alpha(l.address, 9.0, 0x000000, 1.0, 0.04 + 0.03 * lift);
            }
        }
        if focused {
            g.round_outline(l.address, 9.0, accent(), 1.5);
        }
        let x = l.address.x + 8.0;
        let secure = tab.url.starts_with("https://") && !url::is_internal(&tab.url);
        if tab.private {
            let pill = Rect::new(x, l.address.cy() - 10.0, 48.0, 20.0);
            g.round(pill, 10.0, accent());
            g.text("Gizli", pill, Font::Small, base(), Align::Center);
        }
        if editing {
            if !tab.private {
                g.text(if secure { ICON_LOCK } else { ICON_SEARCH }, Rect::new(x, l.address.y, 20.0, l.address.h), Font::MidIcon, faint(), Align::Center);
            }
        } else {
            // Yazmıyorken: kilit + alan adı, alanın ortasında.
            let shown = self.address_label(tab);
            let lo = l.address.x + if tab.private { 62.0 } else { 12.0 };
            let hi = l.address_text.right();
            let tw = g.measure(&shown, Font::Ui).min((hi - lo - 20.0).max(0.0));
            let gw = if secure { 18.0 } else { 0.0 } + tw;
            let gx = (l.address.cx() - gw / 2.0).clamp(lo, (hi - gw).max(lo));
            if secure {
                g.text(ICON_LOCK, Rect::new(gx, l.address.y, 14.0, l.address.h), Font::SmallIcon, muted(), Align::Center);
            }
            g.text(&shown, Rect::new(gx + if secure { 18.0 } else { 0.0 }, l.address.y, tw + 2.0, l.address.h), Font::Ui, ink(), Align::Left);
        }
        // Yükleniyor (Safari gibi): alanın altında soldan dolan mavi çizgi.
        // Süre ilerledikçe yavaşlayarak %90'a yaklaşır; sayfa bitince kaybolur.
        if tab.loading {
            let t = tab.load_started.map_or(0.0, |s| s.elapsed().as_secs_f32());
            let p = 0.08 + 0.84 * (1.0 - (-t / 1.8).exp());
            let track = Rect::new(l.address.x + 8.0, l.address.bottom() - 2.5, l.address.w - 16.0, 2.0);
            g.round(Rect::new(track.x, track.y, track.w * p, track.h), 1.0, system_blue());
        }
        // Yenile / durdur: adres alanının sağ ucunda.
        if !tab.url.is_empty() {
            self.field_button(l.reload, Hit::Reload);
            g.text(if tab.loading { ICON_STOP } else { ICON_RELOAD }, l.reload, Font::MidIcon, if self.hover == Hit::Reload { ink() } else { muted() }, Align::Center);
        }

        if let Some(r) = l.translate {
            self.paint_translate(r);
        }
        if let Some(r) = l.shield {
            self.paint_shield(r);
        }
        // Yıldız: sayfa yer işaretlerindeyse dolu ve vurgu renginde.
        let can_star = !tab.url.is_empty() && !url::is_internal(&tab.url);
        if can_star {
            self.field_button(l.star, Hit::Star);
            let starred = self.is_bookmarked(&tab.url);
            g.text(if starred { ICON_STAR_FILLED } else { ICON_STAR }, l.star, Font::MidIcon, if starred { accent() } else { muted() }, Align::Center);
        }
        if let (Some(r), Some(label)) = (l.zoom_pill, self.zoom_label()) {
            g.round(r, 11.0, if self.hover == Hit::ZoomPill { line() } else { hover() });
            g.text(&label, r, Font::Small, ink(), Align::Center);
        }
        if let (Some(r), Some(store_id)) = (l.add_ext, self.store_page_id()) {
            let installed = self.state.store_installs.contains_key(&store_id);
            let (bg, fg, text) = if self.installing {
                (hover(), muted(), "Ekleniyor…")
            } else if installed {
                (hover(), muted(), "Eklendi ✓")
            } else {
                (if self.hover == Hit::AddExtension { ink() } else { accent() }, base(), "Sqywein'e ekle")
            };
            g.round(r, 12.0, bg);
            g.text(text, r, Font::Small, fg, Align::Center);
        }

        if let Some(r) = l.update {
            self.paint_update_pill(r);
        }
        // İndirmeler: etkin indirme varken ilerleme halkası.
        if let Some(r) = l.downloads {
            let open = self.menu.as_ref().is_some_and(|m| m.kind == MenuKind::Downloads);
            if let Some(t) = self.download_pulse.map(|t| t.elapsed().as_secs_f32()).filter(|t| *t < 2.4) {
                // Bitti: iki kez yavaşça parlayıp söner.
                let wave = 0.5 - 0.5 * (t * std::f32::consts::TAU / 1.2).cos();
                let fade = 1.0 - t / 2.4;
                g.glow(r.cx(), r.cy(), 26.0, 26.0, accent(), 0.55 * wave * fade);
                g.round_alpha(r, 8.0, accent(), 0.25 * wave * fade);
            }
            self.tool_button(r, Hit::Downloads, open);
            if let Some(p) = self.download_progress() {
                // İlerledikçe dolan halka (saat 12'den başlar).
                g.arc(r.cx(), r.cy(), 13.0, 0.75, 0.999, line(), 2.5);
                g.arc(r.cx(), r.cy(), 13.0, 0.75, p.max(0.03), accent(), 2.5);
            }
            g.text(ICON_DOWNLOAD, r, Font::Icon, ink(), Align::Center);
        }

        // Sabitlenen uzantılar
        for (r, id) in l.pinned.iter().zip(self.pinned_extensions()) {
            let i = l.pinned.iter().position(|p| p == r).unwrap_or(0);
            self.tool_button(*r, Hit::Pinned(i), false);
            match self.ext_icons.get(&id) {
                Some(img) => g.image(img, r.centered(18.0, 18.0), false),
                None => {
                    let name = self.extensions.iter().find(|e| e.id == id).map(|e| e.name.clone()).unwrap_or_default();
                    menu::avatar(g, r.centered(20.0, 20.0), None, line(), &name);
                }
            }
        }

        let open = self.menu.as_ref().map(|m| m.kind);
        self.tool_button(l.share, Hit::Share, open == Some(MenuKind::Share));
        g.text(ICON_SHARE, l.share, Font::Icon, ink(), Align::Center);
        self.tool_button(l.puzzle, Hit::Puzzle, open == Some(MenuKind::Extensions));
        g.text(ICON_PUZZLE, l.puzzle, Font::Icon, ink(), Align::Center);

        self.tool_button(l.menu, Hit::Menu, open == Some(MenuKind::Main));
        g.text(ICON_MENU, l.menu, Font::Icon, ink(), Align::Center);
    }
}

const ENGINE_MISSING: &str = "Tarayıcı motoru (WebView2) başlatılamadı. Yeniden dene, olmazsa Sqywein'i kapatıp aç.";

#[cfg(test)]
mod resource_tests {
    use super::*;
    #[test]
    fn automatic_rest_preserves_history_pins_and_private_sessions() {
        use crate::memory::Pressure::*;
        let mut tab = Tab::new(1, "https://example.com".into(), String::new());
        tab.mem = Mem::Awake;
        let idle = Duration::from_secs(200);
        assert_eq!(tab.rest_action(Normal, true, false, false, idle), Some(false));
        assert_eq!(tab.rest_action(Elevated, true, false, false, idle), Some(true));
        assert_eq!(tab.rest_action(Critical, false, false, false, idle), None);
        tab.can_back = true;
        assert_eq!(tab.rest_action(Critical, true, false, false, idle), Some(false));
        tab.can_back = false; tab.pinned = true;
        assert_eq!(tab.rest_action(Critical, true, false, false, idle), Some(false));
        tab.pinned = false; tab.private = true;
        assert_eq!(tab.rest_action(Critical, true, false, false, idle), Some(false));
        tab.private = false; tab.mem = Mem::Dozing;
        assert_eq!(tab.rest_action(Normal, true, false, false, idle), None);
        assert_eq!(tab.rest_action(Critical, true, false, false, idle), Some(true));
    }
    #[test]
    fn discarding_view_clears_document_metadata_and_invalidates_pending_checks() {
        let mut tab = Tab::new(1, "https://example.com".into(), String::new());
        tab.ad_scripts = [Some("old".into()), None];
        tab.ad_exceptions = Some((true, HashSet::from([".ad".into()])));
        tab.sleep_check = true; tab.can_back = true;
        tab.close_view();
        assert_eq!(tab.epoch, 1); assert!(!tab.sleep_check);
        assert!(tab.ad_scripts.iter().all(Option::is_none));
        assert!(tab.ad_exceptions.is_none()); assert!(!tab.can_back);
        assert!(tab.needs_load);
    }
}
