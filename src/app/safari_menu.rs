//! Safari'nin menü çubuğu, Windows'ta tek bir menüde: üstte en sık kullanılanlar,
//! altında Dosya, Düzen, Görünüm, Geçmiş, Yer İşaretleri, Geliştir, Pencere ve
//! Yardım alt menüleri. Alt menüler aynı pencerede açılır ("‹ Geri" ile dönülür).

use super::*;
use menu::{Category, Item};

/// Okuyucu: makalenin metnini sade, okunaklı bir görünümde gösterir; tekrar seçince kapanır.
/// Sayfanın kendi içeriği değiştirilmez, üstüne ayrı bir katman konur.
const READER: &str = r#"(()=>{const id='__sqReader';const old=document.getElementById(id);if(old){old.remove();document.documentElement.style.overflow='';return 'off';}
const score=n=>{let t=0;for(const p of n.querySelectorAll('p'))t+=p.textContent.trim().length;return t;};
let best=document.querySelector('article')||document.querySelector('main');
if(!best||score(best)<400){let max=0;for(const n of document.querySelectorAll('div,section')){const s=score(n);if(s>max){max=s;best=n;}}}
if(!best||score(best)<200)return 'none';
const box=document.createElement('div');box.id=id;const dark=matchMedia('(prefers-color-scheme: dark)').matches;
box.style.cssText='position:fixed;inset:0;z-index:2147483647;overflow:auto;background:'+(dark?'#1c1c1e':'#f8f7f4')+';color:'+(dark?'#e5e5ea':'#1d1d1f');
const inner=document.createElement('div');inner.style.cssText='max-width:680px;margin:0 auto;padding:56px 28px 120px;font:20px/1.65 Charter,Georgia,"Segoe UI Variable Text",serif';
const h=document.createElement('h1');h.textContent=document.title;h.style.cssText='font:700 34px/1.2 "Segoe UI Variable Display","Segoe UI",sans-serif;margin:0 0 6px';inner.append(h);
const host=document.createElement('div');host.textContent=location.hostname.replace(/^www\./,'');host.style.cssText='opacity:.6;font:14px "Segoe UI",sans-serif;margin-bottom:32px';inner.append(host);
for(const n of best.querySelectorAll('h1,h2,h3,h4,p,li,blockquote,pre,img,figcaption')){
 if(n.closest('nav,aside,footer,header,form,[role=navigation],[aria-hidden=true]'))continue;
 if(n.tagName==='IMG'){if(n.naturalWidth<200)continue;const i=document.createElement('img');i.src=n.currentSrc||n.src;i.alt=n.alt||'';i.style.cssText='max-width:100%;height:auto;border-radius:10px;margin:18px 0';inner.append(i);continue;}
 const t=n.textContent.trim();if(t.length<2)continue;if(n.tagName==='LI'&&n.closest('li')!==n)continue;
 const e=document.createElement(/^H/.test(n.tagName)?'h2':n.tagName==='BLOCKQUOTE'?'blockquote':n.tagName==='PRE'?'pre':'p');e.textContent=t;
 if(e.tagName==='H2')e.style.cssText='font:700 24px/1.3 "Segoe UI Variable Display","Segoe UI",sans-serif;margin:34px 0 10px';
 if(e.tagName==='BLOCKQUOTE')e.style.cssText='border-left:3px solid #0a84ff;margin:18px 0;padding-left:16px;opacity:.85';
 if(e.tagName==='PRE')e.style.cssText='white-space:pre-wrap;font:15px/1.5 Consolas,monospace';
 inner.append(e);}
const close=document.createElement('button');close.textContent='✕ Okuyucuyu kapat';close.style.cssText='position:fixed;top:14px;right:18px;border:0;border-radius:16px;padding:7px 14px;cursor:pointer;font:13px "Segoe UI",sans-serif;background:'+(dark?'#3a3a3c':'#e5e5ea')+';color:inherit';
close.onclick=()=>{box.remove();document.documentElement.style.overflow='';};
box.append(inner,close);document.documentElement.style.overflow='hidden';document.body.append(box);
box.addEventListener('keydown',e=>{if(e.key==='Escape')close.click();});box.tabIndex=-1;box.focus();return 'on';})()"#;

impl App {
    fn page_item(&self, label: &str, hint: &str, cmd: MenuCmd) -> Item {
        let mut item = Item::act(label, hint, cmd);
        if let Item::Action { enabled, .. } = &mut item {
            *enabled = self.tabs[self.active].view.is_some() && !self.tabs[self.active].url.is_empty();
        }
        item
    }

    /// Ana menü: en sık kullanılanlar ve Safari'nin menüleri.
    pub(super) fn safari_menu_items(&self) -> Vec<Item> {
        let sub = |label: &str, c: Category| Item::act(label, "›", MenuCmd::Category(c));
        vec![
            Item::act("Yeni sekme", "Ctrl+T", MenuCmd::NewTab),
            Item::act("Yeni pencere", "Ctrl+N", MenuCmd::NewWindow),
            Item::act("Yeni gizli pencere", "Ctrl+Shift+N", MenuCmd::PrivateWindow),
            Item::Sep,
            sub("Dosya", Category::File),
            sub("Düzen", Category::Edit),
            sub("Görünüm", Category::View),
            sub("Geçmiş", Category::History),
            sub("Yer İşaretleri", Category::Bookmarks),
            sub("Geliştir", Category::Develop),
            sub("Pencere", Category::Window),
            sub("Yardım", Category::Help),
            Item::Sep,
            Item::Zoom,
            Item::Sep,
            self.page_item("Bu web sitesi için ayarlar…", "", MenuCmd::SitePermissions),
            Item::act("Uzantılar…", "", MenuCmd::Extensions),
            Item::act("Şifreler…", "", MenuCmd::Passwords),
            Item::act("Ayarlar…", "Ctrl+,", MenuCmd::Settings),
            Item::Sep,
            Item::act("Sqywein'den çık", "", MenuCmd::Exit),
        ]
    }

    /// Safari'nin alt menüleri.
    pub(super) fn category_items(&self, c: Category) -> Vec<Item> {
        let back = Item::act("‹ Geri", "", MenuCmd::Category(Category::Main));
        let tab = &self.tabs[self.active];
        let mut items = vec![back, Item::Sep];
        match c {
            Category::Main => return self.safari_menu_items(),
            Category::File => items.extend([
                Item::act("Yeni sekme", "Ctrl+T", MenuCmd::NewTab),
                Item::act("Yeni pencere", "Ctrl+N", MenuCmd::NewWindow),
                Item::act("Yeni gizli pencere", "Ctrl+Shift+N", MenuCmd::PrivateWindow),
                Item::act("Konum aç…", "Ctrl+L", MenuCmd::OpenLocation),
                Item::Sep,
                Item::act("Sekmeyi kapat", "Ctrl+W", MenuCmd::CloseTab),
                self.page_item("Farklı kaydet…", "Ctrl+S", MenuCmd::SavePage),
                Item::Sep,
                self.page_item("Paylaş…", "", MenuCmd::OpenShare),
                self.page_item("Yazdır…", "Ctrl+P", MenuCmd::Print),
            ]),
            Category::Edit => items.extend([
                Item::act("Geri al", "Ctrl+Z", MenuCmd::Edit(0)),
                Item::act("Kes", "Ctrl+X", MenuCmd::Edit(1)),
                Item::act("Kopyala", "Ctrl+C", MenuCmd::Edit(2)),
                Item::act("Yapıştır", "Ctrl+V", MenuCmd::Edit(3)),
                Item::act("Tümünü seç", "Ctrl+A", MenuCmd::Edit(4)),
                Item::Sep,
                self.page_item("Bul…", "Ctrl+F", MenuCmd::Find),
                Item::act("Otomatik doldurma ve şifreler…", "", MenuCmd::Passwords),
            ]),
            Category::View => items.extend([
                Item::act("Sekmelere genel bakış", "Ctrl+Shift+A", MenuCmd::Overview),
                self.page_item(if self.reader_on { "Okuyucuyu gizle" } else { "Okuyucuyu göster" }, "Ctrl+Shift+R", MenuCmd::Reader),
                self.page_item(if tab.translated { "Orijinal dili göster" } else { "Sayfayı çevir" }, "Ctrl+Shift+Y", MenuCmd::Translate),
                Item::Sep,
                Item::act(if self.state.settings.show_bookmarks_bar { "Yer işaretleri çubuğunu gizle" } else { "Yer işaretleri çubuğunu göster" }, "", MenuCmd::ToggleBookmarksBar),
                Item::act(if self.focus_mode { "Araç çubuğunu göster" } else { "Araç çubuğunu gizle" }, "Ctrl+Shift+F", MenuCmd::FocusMode),
                Item::act(if self.full_screen.is_some() { "Tam ekrandan çık" } else { "Tam ekran" }, "F11", MenuCmd::FullScreen),
                Item::Sep,
                Item::act("Yakınlaştır", "Ctrl +", MenuCmd::ZoomIn),
                Item::act("Uzaklaştır", "Ctrl −", MenuCmd::ZoomOut),
                self.page_item("Yeniden yükle", "F5", MenuCmd::Reload),
            ]),
            Category::History => {
                items.extend([
                    Item::act("Geri", "Alt+←", MenuCmd::Back),
                    Item::act("İleri", "Alt+→", MenuCmd::Forward),
                    Item::act("Ana sayfa", "", MenuCmd::Home),
                    Item::act("Kapatılan son sekmeyi yeniden aç", "Ctrl+Shift+T", MenuCmd::ReopenTab),
                    Item::Sep,
                ]);
                for (i, e) in self.history.recent("", RECENT).into_iter().enumerate() {
                    let label = if e.title.is_empty() { url::host(&e.url).to_string() } else { e.title.chars().take(42).collect() };
                    items.push(Item::Action { label, hint: String::new(), cmd: MenuCmd::OpenMenuLink(i), enabled: true, side: None, icon: self.icon_for(&e.url) });
                }
                items.extend([Item::Sep, Item::act("Tüm geçmişi göster", "Ctrl+H", MenuCmd::History), Item::act("Geçmişi temizle…", "", MenuCmd::ClearData)]);
            }
            Category::Bookmarks => {
                items.extend([
                    Item::act("Yer işaretlerini göster", "Ctrl+Shift+O", MenuCmd::Bookmarks),
                    self.page_item("Yer işareti ekle…", "Ctrl+D", MenuCmd::BookmarkPage),
                    Item::act("Tüm sekmeleri yer işaretlerine ekle", "", MenuCmd::BookmarkAll),
                    Item::Sep,
                    self.page_item("Okuma Listesine ekle", "Ctrl+Shift+D", MenuCmd::AddReading),
                    Item::act(format!("Okuma Listesi ({})", self.state.reading_list.len()), "›", MenuCmd::Category(Category::Reading)),
                    Item::Sep,
                ]);
                for (i, b) in self.state.favorites.iter().take(RECENT).enumerate() {
                    let label = if b.title.is_empty() { url::host(&b.url).to_string() } else { b.title.chars().take(42).collect() };
                    items.push(Item::Action { label, hint: String::new(), cmd: MenuCmd::OpenBookmark(i), enabled: true, side: None, icon: self.icon_for(&b.url) });
                }
            }
            Category::Reading => {
                items[0] = Item::act("‹ Geri", "", MenuCmd::Category(Category::Bookmarks));
                if self.state.reading_list.is_empty() {
                    items.push(Item::disabled("Okuma Listesi boş"));
                }
                for (i, b) in self.state.reading_list.iter().enumerate().rev().take(12) {
                    let label = if b.title.is_empty() { url::host(&b.url).to_string() } else { b.title.chars().take(42).collect() };
                    items.push(Item::Action { label, hint: "Kaldır".into(), cmd: MenuCmd::OpenReading(i), enabled: true, side: Some(MenuCmd::RemoveReading(i)), icon: self.icon_for(&b.url) });
                }
            }
            Category::Develop => items.extend([
                self.page_item("Web denetçisi", "F12", MenuCmd::DevTools),
                self.page_item("Sayfanın kaynağını göster", "Ctrl+U", MenuCmd::ViewSource),
                self.page_item("Yeniden yükle", "F5", MenuCmd::Reload),
            ]),
            Category::Window => items.extend([
                Item::act("Küçült", "", MenuCmd::Minimize),
                Item::act("Büyüt", "", MenuCmd::Maximize),
                Item::act(if self.full_screen.is_some() { "Tam ekrandan çık" } else { "Tam ekran" }, "F11", MenuCmd::FullScreen),
                Item::Sep,
                Item::act("Sekmelere genel bakış", "Ctrl+Shift+A", MenuCmd::Overview),
                self.page_item("Görüntü içinde görüntü", "", MenuCmd::PictureInPicture),
                Item::act("İndirmeler", "Ctrl+J", MenuCmd::Downloads),
            ]),
            Category::Help => items.extend([
                Item::act("Sqywein yardımı", "", MenuCmd::Help),
                Item::act("Sorun bildir…", "", MenuCmd::ReportIssue),
                Item::act("Güncellemeleri denetle", "", MenuCmd::CheckUpdates),
                Item::act("Sqywein hakkında", "", MenuCmd::About),
            ]),
        }
        items
    }

    /// Safari menülerine özgü komutlar; işlendiyse `true`.
    pub(super) fn safari_command(&mut self, cmd: MenuCmd) -> bool {
        match cmd {
            MenuCmd::Category(c) => {
                let items = self.category_items(c);
                if let Some(m) = &mut self.menu { m.set_items(items); }
                return true;
            }
            MenuCmd::RemoveReading(i) => {
                if i < self.state.reading_list.len() { self.state.reading_list.remove(i); self.dirty = true; }
                let items = self.category_items(Category::Reading);
                if let Some(m) = &mut self.menu { m.set_items(items); }
                return true;
            }
            _ => {}
        }
        let handled = matches!(cmd, MenuCmd::NewWindow | MenuCmd::PrivateWindow | MenuCmd::FullScreen | MenuCmd::Edit(_) | MenuCmd::SavePage
            | MenuCmd::ClearData | MenuCmd::ToggleBookmarksBar | MenuCmd::BookmarkAll | MenuCmd::BookmarkPage | MenuCmd::ReopenTab | MenuCmd::OpenMenuLink(_)
            | MenuCmd::AddReading | MenuCmd::OpenReading(_) | MenuCmd::Reader | MenuCmd::About | MenuCmd::Help | MenuCmd::ReportIssue
            | MenuCmd::OpenLocation | MenuCmd::CloseTab | MenuCmd::Back | MenuCmd::Forward | MenuCmd::Home | MenuCmd::Reload | MenuCmd::ViewSource
            | MenuCmd::Minimize | MenuCmd::Maximize | MenuCmd::CheckUpdates);
        if !handled { return false; }
        self.close_menu();
        let hwnd = self.hwnd;
        match cmd {
            MenuCmd::NewWindow | MenuCmd::PrivateWindow => {
                let profile = self.state.current_profile.clone();
                if let Err(e) = crate::windows_session::spawn(cmd == MenuCmd::PrivateWindow, &profile) {
                    crate::log::line(&format!("YENİ PENCERE açılamadı: {e}"));
                }
            }
            MenuCmd::FullScreen => { let on = self.full_screen.is_none(); self.set_full_screen(on); }
            MenuCmd::Edit(k) => self.send_edit_keys(k),
            MenuCmd::SavePage => {
                if let Some(v) = self.active_view().and_then(|v| v.cast::<ICoreWebView2_25>().ok()) {
                    let handler = webview2_com::ShowSaveAsUICompletedHandler::create(Box::new(|_, _| Ok(())));
                    unsafe { let _ = v.ShowSaveAsUI(&handler); }
                }
            }
            MenuCmd::ClearData => self.new_tab(format!("{}#privacy", url::internal("settings")), None),
            MenuCmd::ToggleBookmarksBar => {
                self.state.settings.show_bookmarks_bar = !self.state.settings.show_bookmarks_bar;
                self.dirty = true; self.layout_children(); self.invalidate();
            }
            MenuCmd::BookmarkAll => {
                let urls: Vec<(String, String)> = self.tabs.iter().filter(|t| !t.private && t.url.starts_with("http")).map(|t| (t.label(), t.url.clone())).collect();
                for (title, u) in urls {
                    if !self.state.favorites.iter().any(|b| b.url == u) { self.state.favorites.push(state::Bookmark { title, url: u }); }
                }
                self.dirty = true; self.layout_children(); self.invalidate();
            }
            MenuCmd::BookmarkPage => self.run(Cmd::Bookmark),
            MenuCmd::ReopenTab => self.run(Cmd::ReopenTab),
            MenuCmd::OpenMenuLink(i) => {
                if let Some(u) = self.history.recent("", RECENT).get(i).map(|e| e.url.clone()) { self.navigate(&u); }
            }
            MenuCmd::AddReading => {
                let t = &self.tabs[self.active];
                if t.url.starts_with("http") && !t.private && !self.state.reading_list.iter().any(|b| b.url == t.url) {
                    let item = state::Bookmark { title: t.label(), url: t.url.clone() };
                    self.state.reading_list.push(item);
                    if self.state.reading_list.len() > 200 { self.state.reading_list.remove(0); }
                    self.dirty = true;
                }
            }
            MenuCmd::OpenReading(i) => {
                if let Some(u) = self.state.reading_list.get(i).map(|b| b.url.clone()) { self.navigate(&u); }
            }
            MenuCmd::Reader => self.toggle_reader(),
            MenuCmd::About => self.new_tab(format!("{}#about", url::internal("settings")), None),
            MenuCmd::CheckUpdates => { self.check_updates(); self.new_tab(format!("{}#update", url::internal("settings")), None); }
            MenuCmd::Help => self.new_tab("https://github.com/Sqyrix/sqywein#readme".into(), None),
            MenuCmd::ReportIssue => self.new_tab("https://github.com/Sqyrix/sqywein/issues/new".into(), None),
            MenuCmd::OpenLocation => self.run(Cmd::FocusAddress),
            MenuCmd::CloseTab => self.run(Cmd::CloseTab),
            MenuCmd::Back => self.run(Cmd::Back),
            MenuCmd::Forward => self.run(Cmd::Forward),
            MenuCmd::Home => self.go_home(),
            MenuCmd::Reload => self.run(Cmd::Reload),
            MenuCmd::ViewSource => {
                let u = self.tabs[self.active].url.clone();
                if u.starts_with("http") { self.new_tab(format!("view-source:{u}"), None); }
            }
            MenuCmd::Minimize => defer(move || unsafe { let _ = ShowWindow(hwnd, SW_MINIMIZE); }),
            MenuCmd::Maximize => defer(move || unsafe { let _ = ShowWindow(hwnd, if IsZoomed(hwnd).as_bool() { SW_RESTORE } else { SW_MAXIMIZE }); }),
            _ => {}
        }
        true
    }

    pub(super) fn toggle_reader(&mut self) {
        if let Some(v) = self.active_view() {
            engine::execute(&v, READER);
            self.reader_on = !self.reader_on;
        }
    }

    /// Düzen menüsü: sayfaya klavye kısayolunu gönderir (seçili metin, form alanı vb.).
    fn send_edit_keys(&mut self, k: u8) {
        use windows::Win32::UI::Input::KeyboardAndMouse::*;
        let key = match k { 0 => b'Z', 1 => b'X', 2 => b'C', 3 => b'V', _ => b'A' } as u16;
        self.focus_content();
        defer(move || unsafe {
            let input = |vk: u16, up: bool| INPUT { r#type: INPUT_KEYBOARD, Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: VIRTUAL_KEY(vk), dwFlags: if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) }, ..Default::default() } } };
            let seq = [input(VK_CONTROL.0, false), input(key, false), input(key, true), input(VK_CONTROL.0, true)];
            SendInput(&seq, std::mem::size_of::<INPUT>() as i32);
        });
    }
}

/// Geçmiş ve yer işaretleri alt menülerinde gösterilen öğe sayısı.
const RECENT: usize = 8;
