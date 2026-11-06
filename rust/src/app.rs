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
mod vault;

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
    Awake,
}

struct Tab {
    id: u32,
    url: String,
    title: String,
    loading: bool,
    error: Option<String>,
    ctrl: Option<ICoreWebView2Controller>,
    view: Option<ICoreWebView2>,
    creating: bool,
    /// Görünüm hazır olunca `url` yüklenmeli mi?
    needs_load: bool,
    mem: Mem,
    last_active: Instant,
    opener: Option<u32>,
    can_back: bool,
    can_forward: bool,
    /// Sitenin gerçek simgesi.
    icon: Option<Rc<Image>>,
    /// Gizli sekme: geçmişe ve oturuma yazılmaz, çerezleri kapanınca silinir.
    private: bool,
    /// Engelleyicinin belge betikleri (sayfa gizleme, YouTube) ve genel kural istisnaları.
    ad_scripts: [Option<String>; 2],
    ad_exceptions: Option<(bool, HashSet<String>)>,
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
            error: None,
            ctrl: None,
            view: None,
            creating: false,
            mem: Mem::Asleep,
            last_active: Instant::now(),
            opener: None,
            can_back: false,
            can_forward: false,
            icon: None,
            private: false,
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

    fn open_progress(&self) -> f32 {
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
    Star,
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
}

struct Layout {
    tabs: Vec<Rect>,
    new_tab: Rect,
    min: Rect,
    max: Rect,
    close: Rect,
    back: Rect,
    forward: Rect,
    reload: Rect,
    address: Rect,
    /// Adres çubuğunda yazının sığacağı alan.
    address_text: Rect,
    star: Rect,
    shield: Option<Rect>,
    zoom_pill: Option<Rect>,
    add_ext: Option<Rect>,
    downloads: Option<Rect>,
    pinned: Vec<Rect>,
    puzzle: Rect,
    profile: Rect,
    menu: Rect,
    content: Rect,
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
}

pub struct App {
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
    pressed: Hit,
    tracking_mouse: bool,
    state: state::AppState,
    history: History,
    dirty: bool,
    /// Komut satırında `--no-rest` verilmediyse arka plan sekmeleri dondurulur.
    rest: bool,
    wake_queue: Vec<u32>,
    full_screen: Option<FullScreen>,
    menu: Option<Menu>,
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
    /// Kurulmakta olan uzantı: (mağaza kimliği ya da "", klasör).
    pending_install: Option<(String, std::path::PathBuf)>,
    popup: Option<Popup>,
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
    closing: bool,
    housekeeping_ticks: u32,
    icon_cache: RefCell<HashMap<String, Option<Rc<Image>>>>,
    /// Dosya türü simgeleri (uzantı ya da .exe için dosya yolu) ve sayfalar için PNG hali.
    file_icons: RefCell<HashMap<String, Option<(Rc<Image>, String)>>>,
    icon_fetching: RefCell<HashSet<String>>,
    // Ana sayfa ve öneriler
    logo: Option<Rc<Image>>,
    dialog: Option<newtab::Dialog>,
    suggest: Option<suggest::Suggest>,
    suggest_gen: u64,
    // Şifre yöneticisi
    vault: crate::passwords::Vault,
    pending_login: Option<vault::PendingLogin>,
    hello_until: Option<Instant>,
    hello_pending: Vec<(u32, vault::Gate)>,
    hello_request: u32,
    favicon_urls: RefCell<HashMap<String, String>>,
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
    let edit = CreateWindowExW(
        WINDOW_EX_STYLE(0),
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
    let cue = wide(cue);
    SendMessageW(edit, EM_SETCUEBANNER, Some(WPARAM(1)), Some(LPARAM(cue.as_ptr() as isize)));
    Ok(edit)
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
            WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN,
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
        let _ = SetWindowPos(hwnd, None, 0, 0, 0, 0, SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER);
        let _ = ShowWindow(hwnd, SW_SHOWDEFAULT);
        with_app(|app| app.start());
        run_deferred();

        let mut msg = MSG::default();
        let mut edit_was_focused = false;
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            if msg.message == WM_KEYDOWN || msg.message == WM_SYSKEYDOWN {
                let in_edit = msg.hwnd == edit || msg.hwnd == start_edit;
                let vk = msg.wParam.0 as u32;
                let target = msg.hwnd;
                let consumed = with_app(|app| app.dialog_key(target, vk) || (in_edit && app.suggest_key(target, vk))).unwrap_or(false);
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
fn edit_brushes() -> (HBRUSH, HBRUSH) {
    BRUSHES.with(|b| {
        *b.borrow_mut().get_or_insert_with(|| unsafe {
            (CreateSolidBrush(COLORREF(colorref(RAISED))), CreateSolidBrush(COLORREF(colorref(SEARCH))))
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
                let bg = if is_start { SEARCH } else { RAISED };
                SetTextColor(hdc, COLORREF(colorref(INK)));
                SetBkColor(hdc, COLORREF(colorref(bg)));
                let (address, search) = edit_brushes();
                return LRESULT(if is_start { search.0 } else { address.0 } as isize);
            }
            WM_COMMAND => {
                // Metin kutuları odak değiştirince çerçeveleri yenilensin.
                let _ = InvalidateRect(Some(hwnd), None, false);
                let code = ((wparam.0 >> 16) & 0xFFFF) as u32;
                let control = HWND(lparam.0 as _);
                if code == EN_CHANGE && !SUPPRESS_CHANGE.with(|c| c.get()) {
                    with_app(|app| app.on_edit_change(control));
                    run_deferred();
                } else if code == EN_KILLFOCUS {
                    with_app(|app| app.close_suggest());
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
        // Arayüz her zaman CPU ile çizilir: küçük bir alan için GPU cihazı açmak
        // onlarca MB ekler. "Donanım hızlandırma" ayarı sayfaların motoruna uygulanır.
        let mut gfx = Gfx::new()?;
        gfx.set_dpi(dpi);
        let mut app = Self {
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
            pressed: Hit::None,
            tracking_mouse: false,
            state,
            history: History::load(),
            dirty: false,
            rest: true,
            wake_queue: Vec::new(),
            full_screen: None,
            menu: None,
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
            pending_install: None,
            popup: None,
            downloads: Vec::new(),
            next_download: 1,
            show_downloads: false,
            dl_speed: HashMap::new(),
            download_pulse: None,
            cookies_restored: false,
            closing: false,
            housekeeping_ticks: 0,
            icon_cache: RefCell::new(HashMap::new()),
            file_icons: RefCell::new(HashMap::new()),
            icon_fetching: RefCell::new(HashSet::new()),
            logo: Self::load_logo(),
            dialog: None,
            suggest: None,
            suggest_gen: 0,
            vault: crate::passwords::Vault::default(),
            pending_login: None,
            hello_until: None,
            hello_pending: Vec::new(),
            hello_request: 0,
            favicon_urls: RefCell::new(HashMap::new()),
        };
        app.vault = crate::passwords::Vault::load(&app.state.current_profile);
        app.next_download = app.state.download_list.iter().map(|d| d.id).max().unwrap_or(0) + 1;
        app.apply_fonts();
        Ok(app)
    }

    /// Oturumu geri yükler: hiçbir sekme motor başlatmaz, yalnızca etkin olan uyanır.
    fn start(&mut self) {
        let saved = std::mem::take(&mut self.state.tabs);
        if self.state.settings.startup != "newtab" {
            for t in &saved {
                self.push_tab(t.url.clone(), t.title.clone());
            }
        }

        // Komut satırından gelen adresler yeni sekmede açılır: sqywein.exe https://ornek.com
        let mut opened = 0;
        let mut wake_all = false;
        for arg in std::env::args().skip(1) {
            match arg.as_str() {
                "--wake-all" => wake_all = true, // RAM karşılaştırması: tüm sekmeler yüklensin.
                "--no-rest" => self.rest = false, // Karşılaştırma: otomatik uyutma kapalı.
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
        // Hesap bilgisi ve uzantılar için motor, sayfa açılmasa da hazırlanır.
        self.ensure_env();
        if wake_all {
            self.wake_queue = self.tabs.iter().map(|t| t.id).collect();
            unsafe { SetTimer(Some(self.hwnd), TIMER_WAKE_ALL, 1500, None) };
        }
        unsafe { SetTimer(Some(self.hwnd), TIMER_HOUSEKEEPING, 5000, None) };
        self.load_avatar();
        self.start_adblock();
        self.dirty = false; // Açılışta değişen bir şey yok; gereksiz kayıt yapılmasın.
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
        self.full_screen.is_none() && self.tabs.get(self.active).is_some_and(|t| t.url.is_empty())
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
                let minimized = wparam.0 as u32 == SIZE_MINIMIZED;
                if let Some(view) = self.active_view() {
                    engine::set_low_memory(&view, minimized);
                }
                if !minimized {
                    self.gfx.resize(self.width, self.height);
                    self.layout_children();
                }
                self.close_menu();
                self.close_popup();
                self.close_suggest();
                self.invalidate();
                Some(LRESULT(0))
            }
            WM_MOVE => {
                self.close_menu();
                self.close_popup();
                self.close_suggest();
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
                    self.hover = hit;
                    self.invalidate();
                }
                Some(LRESULT(0))
            }
            WM_MOUSELEAVE => {
                self.tracking_mouse = false;
                self.hover = Hit::None;
                self.pressed = Hit::None;
                self.invalidate();
                Some(LRESULT(0))
            }
            WM_LBUTTONDOWN => {
                let hit = self.hit(lparam);
                self.pressed = hit;
                self.close_suggest();
                match hit {
                    Hit::Tab(i) => self.activate(i), // Basar basmaz geçsin: daha çevik hissettirir.
                    Hit::None if self.on_start_page() => self.focus_start(),
                    _ => {}
                }
                Some(LRESULT(0))
            }
            WM_LBUTTONUP => {
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
                    TIMER_HOUSEKEEPING => {
                        // Oturum çerezleri 30 saniyede bir şifreli olarak saklanır.
                        self.housekeeping_ticks += 1;
                        if self.housekeeping_ticks % 6 == 0 {
                            self.snapshot_cookies();
                        }
                        self.rest_unused_tabs();
                        if self.dirty {
                            self.save();
                        }
                        if self.history.is_dirty() {
                            let _ = self.history.save();
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
                self.begin_close();
                Some(LRESULT(0))
            }
            _ => None,
        }
    }

    // ---------- Animasyon ----------

    /// Animasyon zamanlayıcısını gerekiyorsa başlatır (~60 kare/sn).
    fn animate(&mut self) {
        if !self.anim_running {
            self.anim_running = true;
            self.last_tick = Instant::now();
            unsafe { SetTimer(Some(self.hwnd), TIMER_ANIM, 16, None) };
        }
    }

    fn needs_animation(&self) -> bool {
        let targets = self.layout().tabs;
        !self.ghosts.is_empty()
            || self.installing
            || self.download_pulse.is_some_and(|t| t.elapsed() < Duration::from_millis(2400))
            || self.downloads.iter().any(|(id, _)| self.download_record(*id).is_some_and(|d| d.status == "progress"))
            || self.tabs.iter().any(|t| t.loading || t.open_progress() < 1.0)
            || self.tabs.iter().zip(&targets).any(|(t, r)| t.x != r.x || t.w != r.w)
    }

    fn tick(&mut self) {
        let now = Instant::now();
        let dt = now.duration_since(self.last_tick).as_secs_f32().min(0.05);
        self.last_tick = now;
        let k = 1.0 - (-dt * 20.0).exp();
        let targets = self.layout().tabs;
        for (t, r) in self.tabs.iter_mut().zip(&targets) {
            t.x = lerp_toward(t.x, r.x, k);
            t.w = lerp_toward(t.w, r.w, k);
        }
        self.ghosts.retain(|g| g.progress() < 1.0);
        self.invalidate();
        if !self.needs_animation() {
            self.anim_running = false;
            unsafe { let _ = KillTimer(Some(self.hwnd), TIMER_ANIM); }
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

        let close = Rect::new(w - CAPTION_BTN, 0.0, CAPTION_BTN, TAB_ROW - 6.0);
        let max = Rect::new(close.x - CAPTION_BTN, 0.0, CAPTION_BTN, close.h);
        let min = Rect::new(max.x - CAPTION_BTN, 0.0, CAPTION_BTN, close.h);

        // Sağda pencereyi sürüklemek için boşluk bırakılır.
        let left = 8.0;
        let avail = (min.x - left - 36.0 - 48.0).max(0.0);
        let n = self.tabs.len().max(1) as f32;
        let tw = (avail / n).clamp(TAB_MIN.min(avail / n), TAB_MAX);
        let tabs = (0..self.tabs.len()).map(|i| Rect::new(left + i as f32 * tw, 6.0, tw - 4.0, TAB_ROW - 8.0)).collect();
        let new_tab = Rect::new(left + self.tabs.len() as f32 * tw, 8.0, 28.0, 28.0);

        let ty = TAB_ROW + 6.0;
        let btn = |x: f32| Rect::new(x, ty, 32.0, 32.0);
        let back = btn(8.0);
        let forward = btn(back.right() + 4.0);
        let reload = btn(forward.right() + 4.0);

        // Sağdan sola: menü, profil, yapboz, sabitlenen uzantılar, indirmeler.
        let menu = btn(w - 40.0);
        let profile = btn(menu.x - 36.0);
        let puzzle = btn(profile.x - 36.0);
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
        let address = Rect::new(reload.right() + 8.0, ty, (x - reload.right() - 16.0).max(60.0), 32.0);

        // Adres çubuğunun sağ ucu: yıldız, yakınlaştırma, "Sqywein'e ekle".
        let star = Rect::new(address.right() - 32.0, ty + 2.0, 28.0, 28.0);
        let mut inner_right = star.x;
        let shield = self.shield_visible().then(|| {
            let w = self.shield_width();
            let r = Rect::new(inner_right - w - 2.0, ty + 2.0, w, 28.0);
            inner_right = r.x;
            r
        });
        let zoom_pill = self.zoom_label().map(|_| {
            let r = Rect::new(inner_right - 56.0, ty + 5.0, 52.0, 22.0);
            inner_right = r.x;
            r
        });
        let add_ext = self.store_page_id().map(|_| {
            let r = Rect::new(inner_right - 124.0, ty + 4.0, 118.0, 24.0);
            inner_right = r.x;
            r
        });
        let text_x = address.x + if self.tabs.get(self.active).is_some_and(|t| t.private) { 70.0 } else { 34.0 };
        let address_text = Rect::new(text_x, ty, (inner_right - text_x - 6.0).max(20.0), 32.0);

        let (bar, bar_items, bar_more) = if self.bar_visible() {
            let (items, more) = self.bar_layout(CHROME, w);
            (Some(Rect::new(0.0, CHROME, w, newtab::BAR_H)), items, more)
        } else {
            (None, Vec::new(), None)
        };
        let top = CHROME + bar.map_or(0.0, |b| b.h);
        let content = if self.full_screen.is_some() {
            Rect::new(0.0, 0.0, w, h)
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
            reload,
            address,
            address_text,
            star,
            shield,
            zoom_pill,
            add_ext,
            downloads,
            pinned,
            puzzle,
            profile,
            menu,
            content,
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
        let chrome = self.full_screen.is_none();
        self.place_edit(self.edit, l.address_text, 13.5, chrome);
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
        if self.full_screen.is_some() {
            return Hit::None;
        }
        let l = self.layout();
        for (r, h) in [
            (l.close, Hit::Close),
            (l.max, Hit::Max),
            (l.min, Hit::Min),
            (l.new_tab, Hit::NewTab),
            (l.back, Hit::Back),
            (l.forward, Hit::Forward),
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
        for (r, h) in [(l.shield, Hit::Shield), (l.zoom_pill, Hit::ZoomPill), (l.add_ext, Hit::AddExtension), (l.downloads, Hit::Downloads)] {
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
                let close = Rect::new(r.right() - 26.0, r.y + (r.h - 20.0) / 2.0, 20.0, 20.0);
                return if r.w > 70.0 && close.contains(x, y) { Hit::TabClose(i) } else { Hit::Tab(i) };
            }
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
        if self.full_screen.is_none() && y < TAB_ROW && self.hit_at(x, y) == Hit::None {
            return HTCAPTION;
        }
        HTCLIENT
    }

    fn click(&mut self, hit: Hit) {
        let hwnd = self.hwnd;
        match hit {
            Hit::TabClose(i) => self.close_tab(i),
            Hit::NewTab => self.new_tab(String::new(), None),
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
            Hit::Shield => self.open_menu(MenuKind::Shield),
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
            Hit::BarItem(i) => self.open_bookmark(i, keys::modifiers().ctrl),
            Hit::BarMore => self.open_menu(MenuKind::BarOverflow),
            Hit::Tile(i) => self.open_shortcut(i, keys::modifiers().ctrl),
            Hit::TileRemove(i) => self.remove_shortcut(i),
            Hit::TileAdd => self.open_dialog(),
            Hit::Tab(_) | Hit::None => {}
        }
    }

    // ---------- Komutlar ----------

    pub fn run(&mut self, cmd: Cmd) {
        self.close_menu();
        let view = self.active_view();
        match cmd {
            Cmd::NewTab => self.new_tab(String::new(), None),
            Cmd::PrivateTab => self.new_private_tab(),
            Cmd::CloseTab => self.close_tab(self.active),
            Cmd::ReopenTab => {
                if let Some(u) = self.closed.pop() {
                    self.new_tab(u, None);
                }
            }
            Cmd::FocusAddress => self.focus_address(),
            Cmd::NextTab => self.activate((self.active + 1) % self.tabs.len()),
            Cmd::PrevTab => self.activate((self.active + self.tabs.len() - 1) % self.tabs.len()),
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
            page("Geliştirici araçları", "F12", MenuCmd::DevTools),
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
        };
        let r = self.px(anchor_dip);
        let mut pt = POINT { x: r.right, y: r.bottom + (4.0 * self.scale()) as i32 };
        unsafe { let _ = ClientToScreen(self.hwnd, &mut pt); }
        let zoom = self.tabs[self.active].ctrl.as_ref().map(|_| self.tabs[self.active].zoom);
        if let Ok(m) = Menu::open(self.hwnd, kind, pt, self.dpi, items, zoom, self.gfx.hardware) {
            let hwnd = m.hwnd;
            self.menu = Some(m);
            defer(move || unsafe { let _ = ShowWindow(hwnd, SW_SHOW); });
        }
        self.invalidate();
    }

    fn close_menu(&mut self) {
        if let Some(m) = self.menu.take() {
            self.menu_closed = Some((m.kind, Instant::now()));
            let hwnd = m.hwnd;
            defer(move || unsafe { let _ = DestroyWindow(hwnd); });
            self.invalidate();
        }
    }

    fn menu_message(&mut self, hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> Option<LRESULT> {
        let menu = self.menu.as_mut().filter(|m| m.hwnd == hwnd)?;
        match msg {
            WM_PAINT => {
                let mut ps = PAINTSTRUCT::default();
                unsafe { BeginPaint(hwnd, &mut ps) };
                menu.paint();
                unsafe { let _ = EndPaint(hwnd, &ps); }
                Some(LRESULT(0))
            }
            WM_MOUSEMOVE => {
                let part = menu.hit(lparam);
                if menu.set_hover(part) {
                    unsafe { let _ = InvalidateRect(Some(hwnd), None, false); }
                }
                let mut tme = TRACKMOUSEEVENT { cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: hwnd, dwHoverTime: 0 };
                unsafe { let _ = TrackMouseEvent(&mut tme); }
                Some(LRESULT(0))
            }
            WM_MOUSELEAVE => {
                if menu.set_hover(None) {
                    unsafe { let _ = InvalidateRect(Some(hwnd), None, false); }
                }
                Some(LRESULT(0))
            }
            WM_LBUTTONUP => {
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
            MenuCmd::Passwords => self.open_page("passwords"),
            MenuCmd::Print => {
                if let Some(v) = view.and_then(|v| v.cast::<ICoreWebView2_16>().ok()) {
                    unsafe { let _ = v.ShowPrintUI(COREWEBVIEW2_PRINT_DIALOG_KIND_BROWSER); }
                }
            }
            MenuCmd::Find => self.find_in_page(),
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
            MenuCmd::OpenBookmark(i) => self.open_bookmark(i, false),
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
        crate::adblock::forget(tab.id);
        if !tab.url.is_empty() && !tab.private {
            self.closed.push(tab.url.clone());
        }
        tab.close_view();
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
            }
        }
        self.active = i;
        self.tabs[i].last_active = Instant::now();
        self.dirty = true;
        self.show_active();
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
        let tab = &mut self.tabs[i];
        match (&tab.ctrl, &tab.view) {
            (Some(ctrl), Some(view)) => {
                if tab.mem == Mem::Dozing {
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
        for (i, tab) in self.tabs.iter_mut().enumerate() {
            if i == self.active {
                continue;
            }
            let Some(view) = tab.view.clone() else { continue };
            let idle = now.duration_since(tab.last_active);
            if engine::plays_audio(&view) {
                continue; // Arka planda müzik çalan sekme uyutulmaz.
            }
            if sleep && idle >= SLEEP_AFTER {
                tab.close_view();
            } else if freeze && idle >= DOZE_AFTER && tab.mem == Mem::Awake {
                engine::doze(&view);
                tab.mem = Mem::Dozing;
            }
        }
        self.invalidate();
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
                        self.refresh_account(true);
                    }
                    None => {
                        self.tabs[self.active].error = Some(ENGINE_MISSING.into());
                        self.show_active();
                    }
                }
            }
            Event::ViewReady(id, ctrl) => self.on_view_ready(id, ctrl),
            Event::NavStarting(id, uri) => {
                if let Some(i) = self.index_of(id) {
                    self.on_nav_starting_adblock(i, &uri);
                    self.tabs[i].loading = true;
                    self.animate(); // Yükleme halkası dönsün.
                    self.invalidate();
                }
            }
            Event::NavCompleted(id, ok, status) => {
                if id == ACCOUNT_VIEW {
                    self.on_account_page_loaded(ok);
                    return;
                }
                let Some(i) = self.index_of(id) else { return };
                let tab = &mut self.tabs[i];
                tab.loading = false;
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
            Event::NewWindow(id, uri) => {
                if id == POPUP_VIEW {
                    // Uzantı penceresinden açılan bağlantı yeni sekmeye gider.
                    self.close_popup();
                    self.new_tab(uri, None);
                } else {
                    self.new_tab(uri, Some(id));
                }
            }
            Event::ProcessFailed(id, kind) => {
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
            Event::Shortcut(vk) => {
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
            Event::Cookies(cookies) => self.on_cookies(cookies),
            Event::Cosmetic(id, classes, ids) => self.on_cosmetic(id, classes, ids),
            Event::ScriptAdded(id, slot, script) => self.on_script_added(id, slot, script),
            Event::StoreInstall(id) => self.install_store(id),
            Event::Password(id, source, json) => self.on_password_message(id, &source, &json),
            Event::Zoom(id) => {
                if let Some(i) = self.index_of(id) {
                    self.on_zoom_changed(i);
                }
            }
            Event::WebMessage(id, json) => self.on_web_message(id, &json),
            Event::DownloadStarted(op) => self.on_download_started(op),
            Event::DownloadChanged(id) => self.on_download_changed(id),
            Event::Script(id, result) => {
                if id == ACCOUNT_VIEW {
                    self.on_account_json(&result);
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
        let view = match engine::configure(&ctrl, id, BASE, &self.state.settings) {
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
        tab.view = Some(view);
        tab.mem = Mem::Awake;
        self.attach_adblock(i);
        self.apply_zoom(i);
        if self.extensions.is_empty() {
            self.refresh_extensions();
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
            out.push(state::SavedTab { title: t.title.clone(), url: t.url.clone() });
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
        self.gfx.clear(BASE);
        if self.full_screen.is_none() {
            self.paint_tabs(&l);
            self.paint_caption(&l);
            self.paint_toolbar(&l);
            self.paint_bar(&l);
        }
        self.paint_content(&l);
        self.gfx.end();
    }

    fn paint_content(&self, l: &Layout) {
        let g = &self.gfx;
        let tab = &self.tabs[self.active];
        if tab.url.is_empty() {
            self.paint_start(l);
        } else if let Some(err) = &tab.error {
            g.round(l.content, 10.0, RAISED);
            let (cx, cy) = (l.content.x, l.content.cy());
            g.text("Sayfa açılamadı", Rect::new(cx, cy - 40.0, l.content.w, 28.0), Font::UiBold, INK, Align::Center);
            g.text(err, Rect::new(cx, cy - 10.0, l.content.w, 24.0), Font::Ui, MUTED, Align::Center);
            g.round(l.retry, 10.0, if self.hover == Hit::Retry { INK } else { ACCENT });
            g.text("Yeniden dene", l.retry, Font::Ui, BASE, Align::Center);
        } else if tab.ctrl.is_none() {
            // Görünüm hazırlanırken boş kart.
            g.round(l.content, 10.0, RAISED);
        }
    }

    /// Sekme simgesi: gerçek site simgesi, yüklenirken halka, yoksa baş harf.
    fn paint_badge(&self, badge: Rect, label: &str, icon: Option<&Image>, loading: bool, active: bool, dim: bool, private: bool) {
        let g = &self.gfx;
        if loading {
            let turn = self.t0.elapsed().as_secs_f32() / 0.9;
            g.spinner(badge.cx(), badge.cy(), 6.5, turn.fract(), ACCENT, 2.0);
        } else if let Some(img) = icon {
            g.image(img, badge, false);
        } else {
            g.round(badge, 5.0, if private { ACCENT } else if active { LINE } else { HOVER });
            let letter: String = label.chars().next().map(|c| c.to_uppercase().collect()).unwrap_or_default();
            g.text(&letter, badge, Font::Small, if private { BASE } else if dim { FAINT } else { INK }, Align::Center);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn paint_tab(&self, r: Rect, tab: Option<&Tab>, label: &str, icon: Option<&Image>, active: bool, hovered: bool, close_hover: bool) {
        let g = &self.gfx;
        let private = tab.is_some_and(|t| t.private);
        if active {
            g.round(r, 8.0, RAISED);
            if private {
                g.round_outline_alpha(r, 8.0, ACCENT, 1.0, 0.6);
            }
        } else if hovered {
            g.round(r, 8.0, HOVER);
        }
        if r.w < 24.0 {
            return;
        }
        let loading = tab.is_some_and(|t| t.loading);
        let asleep = tab.is_some_and(|t| t.ctrl.is_none() && !t.url.is_empty());
        let badge = Rect::new(r.x + 10.0, r.cy() - 8.0, 16.0, 16.0);
        self.paint_badge(badge, label, icon, loading, active, asleep, private);

        let show_close = (active || hovered) && r.w > 70.0;
        let text_w = r.w - 34.0 - if show_close { 28.0 } else { 8.0 };
        if text_w > 8.0 {
            let color = if active { INK } else if asleep { FAINT } else { MUTED };
            g.text(label, Rect::new(r.x + 32.0, r.y, text_w, r.h), Font::Ui, color, Align::Left);
        }
        if show_close {
            let close = Rect::new(r.right() - 26.0, r.cy() - 10.0, 20.0, 20.0);
            if close_hover {
                g.round(close, 6.0, LINE);
            }
            g.text(ICON_CLOSE, close, Font::SmallIcon, MUTED, Align::Center);
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
        let shown = self.shown_tabs(l);
        for (i, (tab, r)) in self.tabs.iter().zip(&shown).enumerate() {
            g.set_opacity(tab.open_progress());
            let hovered = matches!(self.hover, Hit::Tab(h) | Hit::TabClose(h) if h == i);
            // Henüz yüklenmemiş (uyuyan) sekmeler de diskteki simgesiyle görünür.
            let icon = if tab.private { tab.icon.clone() } else { tab.icon.clone().or_else(|| self.icon_for(&tab.url)) };
            self.paint_tab(*r, Some(tab), &tab.label(), icon.as_deref(), i == self.active, hovered, self.hover == Hit::TabClose(i));
        }
        g.set_opacity(1.0);

        // "+" düğmesi son sekmenin ardından kayarak gelir.
        let nx = shown.last().map_or(l.new_tab.x, |r| r.right() + 4.0);
        let new_tab = Rect::new(nx, l.new_tab.y, l.new_tab.w, l.new_tab.h);
        if self.hover == Hit::NewTab {
            g.round(new_tab, 8.0, HOVER);
        }
        g.text(ICON_ADD, new_tab, Font::MidIcon, MUTED, Align::Center);
    }

    fn paint_caption(&self, l: &Layout) {
        let g = &self.gfx;
        let maximized = unsafe { IsZoomed(self.hwnd) }.as_bool();
        for (r, hit, icon) in [
            (l.min, Hit::Min, ICON_MIN),
            (l.max, Hit::Max, if maximized { ICON_RESTORE } else { ICON_MAX }),
            (l.close, Hit::Close, ICON_CLOSE),
        ] {
            let hovered = self.hover == hit;
            if hovered {
                g.fill(r, if hit == Hit::Close { DANGER } else { HOVER });
            }
            g.text(icon, r, Font::SmallIcon, if hovered { INK } else { MUTED }, Align::Center);
        }
    }

    fn tool_button(&self, r: Rect, hit: Hit, active: bool) {
        if self.hover == hit || active {
            self.gfx.round(r, 8.0, HOVER);
        }
    }

    fn paint_toolbar(&self, l: &Layout) {
        let g = &self.gfx;
        let tab = &self.tabs[self.active];
        let reload_icon = if tab.loading { ICON_STOP } else { ICON_RELOAD };
        for (r, hit, icon, enabled) in [
            (l.back, Hit::Back, ICON_BACK, tab.can_back),
            (l.forward, Hit::Forward, ICON_FORWARD, tab.can_forward),
            (l.reload, Hit::Reload, reload_icon, !tab.url.is_empty()),
        ] {
            if enabled {
                self.tool_button(r, hit, false);
            }
            g.text(icon, r, Font::Icon, if enabled { INK } else { LINE }, Align::Center);
        }

        // Adres çubuğu
        let focused = unsafe { GetFocus() } == self.edit;
        g.round(l.address, 10.0, RAISED);
        g.round_outline(l.address, 10.0, if focused { ACCENT } else { LINE }, if focused { 1.5 } else { 1.0 });
        let mut x = l.address.x + 8.0;
        if tab.private {
            let pill = Rect::new(x, l.address.cy() - 10.0, 52.0, 20.0);
            g.round(pill, 10.0, ACCENT);
            g.text("Gizli", pill, Font::Small, BASE, Align::Center);
            x = pill.right() + 4.0;
        } else {
            let secure = tab.url.starts_with("https://") && !url::is_internal(&tab.url);
            g.text(if secure { ICON_LOCK } else { ICON_SEARCH }, Rect::new(x, l.address.y, 22.0, l.address.h), Font::MidIcon, FAINT, Align::Center);
        }
        let _ = x;

        if let Some(r) = l.shield {
            self.paint_shield(r);
        }
        // Yıldız: sayfa yer işaretlerindeyse dolu ve vurgu renginde.
        let can_star = !tab.url.is_empty() && !url::is_internal(&tab.url);
        if can_star {
            if self.hover == Hit::Star {
                g.round(l.star, 6.0, HOVER);
            }
            let starred = self.is_bookmarked(&tab.url);
            g.text(if starred { ICON_STAR_FILLED } else { ICON_STAR }, l.star, Font::MidIcon, if starred { ACCENT } else { MUTED }, Align::Center);
        }
        if let (Some(r), Some(label)) = (l.zoom_pill, self.zoom_label()) {
            g.round(r, 11.0, if self.hover == Hit::ZoomPill { LINE } else { HOVER });
            g.text(&label, r, Font::Small, INK, Align::Center);
        }
        if let (Some(r), Some(store_id)) = (l.add_ext, self.store_page_id()) {
            let installed = self.state.store_installs.contains_key(&store_id);
            let (bg, fg, text) = if self.installing {
                (HOVER, MUTED, "Ekleniyor…")
            } else if installed {
                (HOVER, MUTED, "Eklendi ✓")
            } else {
                (if self.hover == Hit::AddExtension { INK } else { ACCENT }, BASE, "Sqywein'e ekle")
            };
            g.round(r, 12.0, bg);
            g.text(text, r, Font::Small, fg, Align::Center);
        }

        // İndirmeler: etkin indirme varken ilerleme halkası.
        if let Some(r) = l.downloads {
            let open = self.menu.as_ref().is_some_and(|m| m.kind == MenuKind::Downloads);
            if let Some(t) = self.download_pulse.map(|t| t.elapsed().as_secs_f32()).filter(|t| *t < 2.4) {
                // Bitti: iki kez yavaşça parlayıp söner.
                let wave = 0.5 - 0.5 * (t * std::f32::consts::TAU / 1.2).cos();
                let fade = 1.0 - t / 2.4;
                g.glow(r.cx(), r.cy(), 26.0, 26.0, ACCENT, 0.55 * wave * fade);
                g.round_alpha(r, 8.0, ACCENT, 0.25 * wave * fade);
            }
            self.tool_button(r, Hit::Downloads, open);
            if let Some(p) = self.download_progress() {
                // İlerledikçe dolan halka (saat 12'den başlar).
                g.arc(r.cx(), r.cy(), 13.0, 0.75, 0.999, LINE, 2.5);
                g.arc(r.cx(), r.cy(), 13.0, 0.75, p.max(0.03), ACCENT, 2.5);
            }
            g.text(ICON_DOWNLOAD, r, Font::Icon, INK, Align::Center);
        }

        // Sabitlenen uzantılar
        for (r, id) in l.pinned.iter().zip(self.pinned_extensions()) {
            let i = l.pinned.iter().position(|p| p == r).unwrap_or(0);
            self.tool_button(*r, Hit::Pinned(i), false);
            match self.ext_icons.get(&id) {
                Some(img) => g.image(img, r.centered(18.0, 18.0), false),
                None => {
                    let name = self.extensions.iter().find(|e| e.id == id).map(|e| e.name.clone()).unwrap_or_default();
                    menu::avatar(g, r.centered(20.0, 20.0), None, LINE, &name);
                }
            }
        }

        let open = self.menu.as_ref().map(|m| m.kind);
        self.tool_button(l.puzzle, Hit::Puzzle, open == Some(MenuKind::Extensions));
        g.text(ICON_PUZZLE, l.puzzle, Font::Icon, INK, Align::Center);

        // Profil: Google fotoğrafı ya da profil renginde baş harf.
        self.tool_button(l.profile, Hit::Profile, open == Some(MenuKind::Profile));
        let p = self.state.profile();
        let name = if p.account_name.is_empty() { p.name.clone() } else { p.account_name.clone() };
        menu::avatar(g, l.profile.centered(24.0, 24.0), self.avatar.as_deref(), menu::parse_color(&p.color), &name);

        self.tool_button(l.menu, Hit::Menu, open == Some(MenuKind::Main));
        g.text(ICON_MENU, l.menu, Font::Icon, INK, Align::Center);
    }
}

const ENGINE_MISSING: &str = "Tarayıcı motoru (WebView2) başlatılamadı. Yeniden dene, olmazsa Sqywein'i kapatıp aç.";
