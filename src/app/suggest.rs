//! Adres çubuğu (ve ana sayfa araması) önerileri: yazdığın şey, geçmiş, yer
//! işaretleri ve arama motorunun önerileri. Liste odak çalmayan bir açılır
//! pencerede durur; klavye yazmaya devam eder, oklarla seçilir.

use super::*;
use crate::{
    history::Favorite,
    net,
};

pub(super) const SUGGEST_CLASS: PCWSTR = w!("SqyweinSuggest");
pub(super) const TIMER_SUGGEST: usize = 5;
const ROW_H: f32 = 36.0;
const MAX_ROWS: usize = 8;

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Search,
    Url,
    History,
    Bookmark,
    /// Açık bir sekmeye geç (kimliği).
    Tab(u32),
    /// Tarayıcı komutu (komut tablosundaki sırası).
    Command(usize),
    /// Sık kullanılan bir site (geçmişte yoksa).
    Site,
}

fn is_site(k: Kind) -> bool {
    matches!(k, Kind::History | Kind::Bookmark | Kind::Site)
}

/// Adres çubuğundan çalıştırılabilen komutlar: (simge, ad, anahtar sözcükler).
const COMMANDS: [(&str, &str, &str); 10] = [
    ("\u{E713}", "Ayarlar", "ayar settings seçenek tercih"),
    ("\u{E81C}", "Geçmiş", "geçmiş gecmis history"),
    ("\u{E896}", "İndirmeler", "indir indirme download"),
    ("\u{E734}", "Yer işaretleri", "yer işaret favori bookmark"),
    ("\u{E8D7}", "Şifreler", "şifre sifre parola password"),
    ("\u{EA86}", "Uzantılar", "uzantı uzanti eklenti extension"),
    ("\u{E727}", "Yeni gizli sekme", "gizli incognito private"),
    ("\u{E7A7}", "Kapatılan sekmeyi geri aç", "geri aç kapatılan reopen"),
    ("\u{E735}", "Bu sayfayı yer işaretlerine ekle", "yıldız yildiz ekle kaydet bookmark"),
    ("\u{EC7A}", "Geliştirici araçları", "geliştirici gelistirici devtools konsol"),
];

/// Geçmiş boşken de site önerebilmek için sık kullanılan siteler:
/// (aranan sözcükler, adres). Sözcüklerden biri yazılanla başlarsa önerilir.
const POPULAR: &[(&str, &str)] = &[
    ("instagram insta", "https://www.instagram.com/"),
    ("youtube yt", "https://www.youtube.com/"),
    ("google", "https://www.google.com/"),
    ("gmail mail", "https://mail.google.com/"),
    ("drive", "https://drive.google.com/"),
    ("maps harita", "https://maps.google.com/"),
    ("translate çeviri ceviri", "https://translate.google.com/"),
    ("x twitter", "https://x.com/"),
    ("facebook fb", "https://www.facebook.com/"),
    ("whatsapp wp", "https://web.whatsapp.com/"),
    ("telegram", "https://web.telegram.org/"),
    ("tiktok", "https://www.tiktok.com/"),
    ("netflix", "https://www.netflix.com/"),
    ("spotify", "https://open.spotify.com/"),
    ("twitch", "https://www.twitch.tv/"),
    ("discord", "https://discord.com/app"),
    ("reddit", "https://www.reddit.com/"),
    ("pinterest", "https://www.pinterest.com/"),
    ("linkedin", "https://www.linkedin.com/"),
    ("github", "https://github.com/"),
    ("stackoverflow", "https://stackoverflow.com/"),
    ("chatgpt openai", "https://chatgpt.com/"),
    ("claude", "https://claude.ai/"),
    ("gemini", "https://gemini.google.com/"),
    ("outlook hotmail", "https://outlook.live.com/"),
    ("office", "https://www.office.com/"),
    ("wikipedia vikipedi", "https://tr.wikipedia.org/"),
    ("eksisozluk ekşi eksi", "https://eksisozluk.com/"),
    ("trendyol", "https://www.trendyol.com/"),
    ("hepsiburada", "https://www.hepsiburada.com/"),
    ("amazon", "https://www.amazon.com.tr/"),
    ("n11", "https://www.n11.com/"),
    ("sahibinden", "https://www.sahibinden.com/"),
    ("yemeksepeti", "https://www.yemeksepeti.com/"),
    ("getir", "https://getir.com/"),
    ("akakce akakçe", "https://www.akakce.com/"),
    ("cimri", "https://www.cimri.com/"),
    ("epey", "https://www.epey.com/"),
    ("edevlet e-devlet turkiye", "https://www.turkiye.gov.tr/"),
    ("osym ösym", "https://www.osym.gov.tr/"),
    ("meb", "https://www.meb.gov.tr/"),
    ("ptt", "https://www.ptt.gov.tr/"),
    ("hurriyet hürriyet", "https://www.hurriyet.com.tr/"),
    ("sozcu sözcü", "https://www.sozcu.com.tr/"),
    ("haberturk habertürk", "https://www.haberturk.com/"),
    ("ntv", "https://www.ntv.com.tr/"),
    ("milliyet", "https://www.milliyet.com.tr/"),
    ("sabah", "https://www.sabah.com.tr/"),
    ("bbc", "https://www.bbc.com/turkce"),
    ("ziraat", "https://www.ziraatbank.com.tr/"),
    ("garanti", "https://www.garantibbva.com.tr/"),
    ("isbank işbank is bankasi", "https://www.isbank.com.tr/"),
    ("yapikredi yapı kredi", "https://www.yapikredi.com.tr/"),
    ("akbank", "https://www.akbank.com/"),
    ("steam", "https://store.steampowered.com/"),
    ("epic epicgames", "https://store.epicgames.com/"),
    ("udemy", "https://www.udemy.com/"),
    ("coursera", "https://www.coursera.org/"),
    ("duolingo", "https://www.duolingo.com/"),
    ("booking", "https://www.booking.com/"),
    ("airbnb", "https://www.airbnb.com/"),
    ("obilet", "https://www.obilet.com/"),
    ("thy turkish türk hava", "https://www.turkishairlines.com/"),
    ("pegasus", "https://www.flypgs.com/"),
    ("apple", "https://www.apple.com/tr/"),
    ("microsoft", "https://www.microsoft.com/tr-tr"),
    ("canva", "https://www.canva.com/"),
    ("figma", "https://www.figma.com/"),
    ("notion", "https://www.notion.so/"),
    ("dropbox", "https://www.dropbox.com/"),
    ("icloud", "https://www.icloud.com/"),
    ("yandex", "https://yandex.com.tr/"),
    ("bing", "https://www.bing.com/"),
    ("duckduckgo", "https://duckduckgo.com/"),
    ("imdb", "https://www.imdb.com/"),
    ("letterboxd", "https://letterboxd.com/"),
    ("disney", "https://www.disneyplus.com/"),
    ("exxen", "https://www.exxen.com/"),
    ("blutv", "https://www.blutv.com/"),
    ("kick", "https://kick.com/"),
];
const MAX_SITES: usize = 3;

/// Türkçe büyük/küçük harf ve aksan farkı gözetmeden karşılaştırma için.
fn fold(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'İ' | 'I' | 'ı' => 'i',
            'Ş' | 'ş' => 's',
            'Ğ' | 'ğ' => 'g',
            'Ü' | 'ü' => 'u',
            'Ö' | 'ö' => 'o',
            'Ç' | 'ç' => 'c',
            c => c.to_ascii_lowercase(),
        })
        .collect()
}

#[derive(Clone)]
struct Row {
    kind: Kind,
    text: String,
    url: String,
}

pub(super) struct Suggest {
    pub(super) hwnd: HWND,
    gfx: Gfx,
    /// Önerinin bağlı olduğu metin kutusu (adres çubuğu ya da ana sayfa).
    edit: HWND,
    query: String,
    rows: Vec<Row>,
    selected: Option<usize>,
    width: f32,
}

impl App {
    /// Metin kutusunda yazı değişti.
    pub(super) fn on_edit_change(&mut self, edit: HWND) {
        if unsafe { GetFocus() } != edit || (edit != self.edit && edit != self.start_edit) {
            return;
        }
        let text = Self::text_of(edit);
        if text.trim().is_empty() {
            self.close_suggest();
            return;
        }
        self.suggest_gen += 1;
        let rows = self.local_rows(&text);
        self.show_suggest(edit, &text, rows);

        unsafe { let _ = KillTimer(Some(self.hwnd), TIMER_SUGGEST); }
        if self.state.settings.search_suggestions && !self.tabs[self.active].private && !text.contains("://") {
            unsafe { SetTimer(Some(self.hwnd), TIMER_SUGGEST, 300, None); }
        }
    }

    pub(super) fn fetch_suggestions(&mut self) {
        unsafe { let _ = KillTimer(Some(self.hwnd), TIMER_SUGGEST); }
        let Some(sg) = &self.suggest else { return };
        if self.suggest_busy || !self.state.settings.search_suggestions || self.tabs[self.active].private { return; }
        let (generation, query) = (self.suggest_gen, sg.query.clone());
        let endpoint = self.state.settings.suggest_url();
        self.suggest_busy = true;
        std::thread::spawn(move || {
            net::init_thread();
            let list = net::fetch(&format!("{endpoint}{}", url::encode(&query)), 64 * 1024).map(|b| net::parse_suggestions(&b)).unwrap_or_default();
            engine::push_background(engine::Background::Suggestions(generation, list));
        });
    }

    fn local_rows(&self, text: &str) -> Vec<Row> {
        let search = self.state.settings.search_url();
        let typed = text.trim().to_string();
        let target = url::to_url_with(&typed, search).unwrap_or_default();
        let mut rows = vec![if target.starts_with(search) {
            Row { kind: Kind::Search, text: typed.clone(), url: target }
        } else {
            Row { kind: Kind::Url, text: typed.clone(), url: target }
        }];
        // Açık sekmeler ve komutlar: yazılan en az iki harfse aranır.
        let q = fold(&typed);
        if q.chars().count() >= 2 {
            for t in self.tabs.iter().filter(|t| t.id != self.tabs[self.active].id && !t.url.is_empty()) {
                if rows.iter().filter(|r| matches!(r.kind, Kind::Tab(_))).count() >= 2 {
                    break;
                }
                if fold(&t.label()).contains(&q) || fold(&t.url).contains(&q) {
                    rows.push(Row { kind: Kind::Tab(t.id), text: t.label(), url: t.url.clone() });
                }
            }
            for (k, (_, name, words)) in COMMANDS.iter().enumerate() {
                if rows.iter().filter(|r| matches!(r.kind, Kind::Command(_))).count() >= 2 {
                    break;
                }
                let hay = fold(&format!("{name} {words}"));
                if hay.split_whitespace().any(|w| w.starts_with(&q)) || fold(name).contains(&q) {
                    rows.push(Row { kind: Kind::Command(k), text: name.to_string(), url: String::new() });
                }
            }
        }
        let favorites: Vec<Favorite> = self.state.favorites.iter().map(|b| Favorite { title: b.title.clone(), url: b.url.clone() }).collect();
        // Siteler: önce geçmiş ve yer işaretleri, sonra sık kullanılanlar; hep listenin altında.
        let host_of = |u: &str| url::host(u).trim_start_matches("www.").to_string();
        for s in self.history.suggest(&typed, &favorites, "", 5) {
            if rows.iter().filter(|r| is_site(r.kind)).count() >= MAX_SITES { break; }
            if rows.iter().any(|r| r.url == s.url || (is_site(r.kind) && host_of(&r.url) == host_of(&s.url))) {
                continue;
            }
            let kind = if self.is_bookmarked(&s.url) { Kind::Bookmark } else { Kind::History };
            rows.push(Row { kind, text: s.title, url: s.url });
        }
        if q.chars().count() >= 2 && !typed.contains(' ') {
            for (words, site) in POPULAR {
                if rows.iter().filter(|r| is_site(r.kind)).count() >= MAX_SITES { break; }
                let host = host_of(site);
                let hit = fold(words).split_whitespace().any(|w| w.starts_with(&q)) || host.starts_with(&q);
                if hit && !rows.iter().any(|r| host_of(&r.url) == host) {
                    rows.push(Row { kind: Kind::Site, text: host, url: site.to_string() });
                }
            }
        }
        rows
    }

    pub(super) fn on_suggestions(&mut self, generation: u64, list: Vec<String>) {
        self.suggest_busy = false;
        if generation != self.suggest_gen {
            if self.suggest.is_some() { unsafe { SetTimer(Some(self.hwnd), TIMER_SUGGEST, 300, None); } }
            return;
        }
        let search = self.state.settings.search_url();
        let Some(sg) = &mut self.suggest else { return };
        // Aramalar sitelerin üstüne girer; siteler her zaman en altta kalır.
        let (sites, mut head): (Vec<Row>, Vec<Row>) = sg.rows.drain(..).partition(|r| is_site(r.kind));
        let room = MAX_ROWS.saturating_sub(head.len() + sites.len());
        let mut added = 0;
        for q in list {
            if added >= room {
                break;
            }
            if q.eq_ignore_ascii_case(&sg.query) || head.iter().any(|r| r.kind == Kind::Search && r.text == q) {
                continue;
            }
            let u = format!("{search}{}", url::encode(&q));
            head.push(Row { kind: Kind::Search, text: q, url: u });
            added += 1;
        }
        head.extend(sites);
        sg.rows = head;
        let (edit, query, rows) = (sg.edit, sg.query.clone(), sg.rows.clone());
        self.show_suggest(edit, &query, rows);
    }

    fn suggest_anchor(&self, edit: HWND) -> Rect {
        let l = self.layout();
        if edit == self.start_edit {
            let s = self.start_layout(l.content).search;
            Rect::new(s.x + 8.0, s.bottom() + 6.0, s.w - 16.0, 0.0)
        } else {
            Rect::new(l.address.x, l.address.bottom() + 4.0, l.address.w, 0.0)
        }
    }

    fn show_suggest(&mut self, edit: HWND, query: &str, mut rows: Vec<Row>) {
        rows.truncate(MAX_ROWS);
        let anchor = self.suggest_anchor(edit);
        let s = self.scale();
        let h = rows.len() as f32 * ROW_H + 12.0;
        let r = self.px(Rect::new(anchor.x, anchor.y, anchor.w, h));
        let mut pt = POINT { x: r.left, y: r.top };
        unsafe { let _ = ClientToScreen(self.hwnd, &mut pt); }
        let (w, hp) = (r.right - r.left, r.bottom - r.top);
        let selected = self.suggest.as_ref().filter(|sg| sg.query == query).and_then(|sg| sg.selected);
        match &mut self.suggest {
            Some(sg) => {
                sg.edit = edit;
                sg.query = query.to_string();
                sg.rows = rows;
                sg.selected = selected.filter(|&i| i < sg.rows.len());
                sg.width = anchor.w;
                let hwnd = sg.hwnd;
                unsafe {
                    let _ = SetWindowPos(hwnd, Some(HWND_TOP), pt.x, pt.y, w, hp, SWP_NOACTIVATE | SWP_SHOWWINDOW);
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
            }
            None => {
                let Ok(instance) = (unsafe { GetModuleHandleW(None) }) else { return };
                let Ok(hwnd) = (unsafe {
                    CreateWindowExW(WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE, SUGGEST_CLASS, w!(""), WS_POPUP, pt.x, pt.y, w, hp, Some(self.hwnd), None, Some(instance.into()), None)
                }) else {
                    return;
                };
                unsafe {
                    let dark: i32 = i32::from(is_dark());
                    let _ = DwmSetWindowAttribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE, &dark as *const _ as _, 4);
                    let corner = DWMWCP_ROUND;
                    let _ = DwmSetWindowAttribute(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE, &corner as *const _ as _, 4);
                }
                let Ok(mut gfx) = Gfx::new() else { return };
                gfx.set_dpi(self.dpi);
                // Öneri listesi opaktır: yazılar ClearType ile keskin çizilir.
                self.suggest = Some(Suggest { hwnd, gfx, edit, query: query.to_string(), rows, selected: None, width: anchor.w });
                defer(move || unsafe { let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE); });
            }
        }
        let _ = s;
    }

    pub(super) fn close_suggest(&mut self) {
        if let Some(sg) = self.suggest.take() {
            let hwnd = sg.hwnd;
            defer(move || unsafe { let _ = DestroyWindow(hwnd); });
        }
        self.suggest_gen += 1;
        unsafe { let _ = KillTimer(Some(self.hwnd), TIMER_SUGGEST); }
    }

    /// Öneri listesi açıkken ok, Enter ve Esc tuşları. Tüketildiyse `true`.
    pub(super) fn suggest_key(&mut self, edit: HWND, vk: u32) -> bool {
        let Some(sg) = &mut self.suggest else { return false };
        if sg.edit != edit || sg.rows.is_empty() {
            return false;
        }
        let n = sg.rows.len();
        match vk {
            0x28 => sg.selected = Some(sg.selected.map_or(0, |i| (i + 1) % n)),
            0x26 => sg.selected = Some(sg.selected.map_or(n - 1, |i| (i + n - 1) % n)),
            0x0D => {
                let row = sg.selected.and_then(|i| sg.rows.get(i).cloned());
                match row {
                    Some(row) => self.open_row(row),
                    None => return false, // Seçim yoksa normal Enter: yazılanı aç.
                }
                return true;
            }
            0x1B => {
                self.close_suggest();
                return true;
            }
            _ => return false,
        }
        unsafe { let _ = InvalidateRect(Some(sg.hwnd), None, false); }
        true
    }

    fn open_row(&mut self, row: Row) {
        self.close_suggest();
        match row.kind {
            Kind::Tab(id) => {
                if let Some(i) = self.index_of(id) {
                    let current = self.tabs[self.active].url.clone();
                    Self::set_text(self.edit, &url::display(&current));
                    self.activate(i);
                }
            }
            Kind::Command(k) => {
                let current = self.tabs[self.active].url.clone();
                Self::set_text(self.edit, &url::display(&current));
                match k {
                    0 => self.open_page("settings"),
                    1 => self.open_page("history"),
                    2 => self.open_page("downloads"),
                    3 => self.open_page("bookmarks"),
                    4 => self.open_page("passwords"),
                    5 => self.open_page("extensions"),
                    6 => self.new_private_tab(),
                    7 => self.run(Cmd::ReopenTab),
                    8 => self.run(Cmd::Bookmark),
                    _ => self.run(Cmd::DevTools),
                }
            }
            _ => self.navigate(&row.url),
        }
    }

    fn suggest_row_at(&self, lparam: LPARAM) -> Option<usize> {
        let sg = self.suggest.as_ref()?;
        let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as f32 / self.scale();
        let i = ((y - 6.0) / ROW_H).floor();
        (i >= 0.0 && (i as usize) < sg.rows.len()).then_some(i as usize)
    }

    pub(super) fn suggest_message(&mut self, hwnd: HWND, msg: u32, _wparam: WPARAM, lparam: LPARAM) -> Option<LRESULT> {
        if self.suggest.as_ref().map(|s| s.hwnd) != Some(hwnd) {
            return None;
        }
        match msg {
            // Tıklamak odağı metin kutusundan almasın.
            WM_MOUSEACTIVATE => Some(LRESULT(MA_NOACTIVATE as isize)),
            WM_PAINT => {
                let mut ps = PAINTSTRUCT::default();
                unsafe { BeginPaint(hwnd, &mut ps) };
                self.paint_suggest();
                unsafe { let _ = EndPaint(hwnd, &ps); }
                Some(LRESULT(0))
            }
            WM_MOUSEMOVE => {
                let i = self.suggest_row_at(lparam);
                if let Some(sg) = &mut self.suggest {
                    if sg.selected != i && i.is_some() {
                        sg.selected = i;
                        unsafe { let _ = InvalidateRect(Some(hwnd), None, false); }
                    }
                }
                Some(LRESULT(0))
            }
            WM_LBUTTONUP => {
                if let Some(row) = self.suggest_row_at(lparam).and_then(|i| self.suggest.as_ref()?.rows.get(i).cloned()) {
                    self.open_row(row);
                }
                Some(LRESULT(0))
            }
            _ => None,
        }
    }

    fn paint_suggest(&mut self) {
        let icons: Vec<Option<Rc<Image>>> = self
            .suggest
            .as_ref()
            .map(|sg| {
                sg.rows
                    .iter()
                    .map(|r| match r.kind {
                        Kind::History | Kind::Bookmark | Kind::Site => self.icon_for(&r.url),
                        Kind::Tab(id) => self.index_of(id).and_then(|i| self.tabs[i].icon.clone()).or_else(|| self.icon_for(&r.url)),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default();
        let Some(sg) = &mut self.suggest else { return };
        let h = sg.rows.len() as f32 * ROW_H + 12.0;
        // Pencerenin gerçek boyutu: liste uzayıp kısalınca çizim yüzeyi de onunla
        // büyümeli, yoksa eski boyutta kalan yüzey gerilir ve yazılar bozulur.
        let mut rc = RECT::default();
        unsafe { let _ = GetClientRect(sg.hwnd, &mut rc); }
        let (wp, hp) = ((rc.right - rc.left).max(1) as u32, (rc.bottom - rc.top).max(1) as u32);
        sg.gfx.resize(wp, hp);
        if !sg.gfx.begin(sg.hwnd, wp, hp) {
            return;
        }
        let g = &sg.gfx;
        if sg.gfx.is_glass() {
            g.clear_transparent();
            let (c, a) = crate::theme::menu_tint();
            g.fill_alpha(Rect::new(0.0, 0.0, sg.width, h), c, a);
        } else {
            g.clear(raised());
        }
        g.round_outline_alpha(Rect::new(0.0, 0.0, sg.width, h), 8.0, line(), 1.0, 0.8);
        for (i, row) in sg.rows.iter().enumerate() {
            let r = Rect::new(6.0, 6.0 + i as f32 * ROW_H, sg.width - 12.0, ROW_H);
            if sg.selected == Some(i) {
                g.round(r, 8.0, hover());
            }
            let icon = Rect::new(r.x + 10.0, r.cy() - 8.0, 16.0, 16.0);
            match (&icons.get(i).cloned().flatten(), row.kind) {
                (Some(img), _) => g.image(img, icon, false),
                (None, Kind::Search) => g.text(ICON_SEARCH, icon, Font::MidIcon, muted(), Align::Center),
                (None, Kind::Bookmark) => g.text(ICON_STAR_FILLED, icon, Font::MidIcon, accent(), Align::Center),
                (None, Kind::Command(k)) => g.text(COMMANDS[k].0, icon, Font::MidIcon, accent(), Align::Center),
                (None, _) => g.text(ICON_GLOBE, icon, Font::MidIcon, muted(), Align::Center),
            }
            let x = icon.right() + 12.0;
            let avail = r.right() - x - 10.0;
            match row.kind {
                Kind::Search | Kind::Url => {
                    g.text(&row.text, Rect::new(x, r.y, avail, r.h), Font::Ui, ink(), Align::Left);
                }
                Kind::Tab(_) | Kind::Command(_) => {
                    let tw = g.measure(&row.text, Font::Ui).min(avail * 0.7);
                    g.text(&row.text, Rect::new(x, r.y, tw + 2.0, r.h), Font::Ui, ink(), Align::Left);
                    let hint = if matches!(row.kind, Kind::Tab(_)) { "— Sekmeye geç" } else { "— Komut" };
                    g.text(hint, Rect::new(x + tw + 8.0, r.y, (avail - tw - 8.0).max(0.0), r.h), Font::Small, accent(), Align::Left);
                }
                Kind::Site => {
                    let tw = g.measure(&row.text, Font::Ui).min(avail * 0.7);
                    g.text(&row.text, Rect::new(x, r.y, tw + 2.0, r.h), Font::Ui, ink(), Align::Left);
                    g.text("— Siteye git", Rect::new(x + tw + 8.0, r.y, (avail - tw - 8.0).max(0.0), r.h), Font::Small, faint(), Align::Left);
                }
                Kind::History | Kind::Bookmark => {
                    let title = if row.text.is_empty() { url::display(&row.url) } else { row.text.clone() };
                    let tw = g.measure(&title, Font::Ui).min(avail * 0.6);
                    g.text(&title, Rect::new(x, r.y, tw + 2.0, r.h), Font::Ui, ink(), Align::Left);
                    let host = format!("— {}", url::host(&row.url));
                    g.text(&host, Rect::new(x + tw + 8.0, r.y, (avail - tw - 8.0).max(0.0), r.h), Font::Small, faint(), Align::Left);
                }
            }
        }
        sg.gfx.end();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turkce_katlama_ve_komutlar() {
        assert_eq!(fold("Şifreler"), "sifreler");
        assert_eq!(fold("İNDİRMELER"), "indirmeler");
        let q = fold("şifr");
        assert!(COMMANDS.iter().any(|(_, n, w)| fold(&format!("{n} {w}")).split_whitespace().any(|x| x.starts_with(&q))));
    }
}
