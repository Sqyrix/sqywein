//! Uzantılar: Chrome Web Mağazası'ndan kurma, yapboz menüsü, sabitleme ve
//! uzantının küçük penceresi (popup).
//!
//! WebView2 uzantıları paketlenmemiş klasörden yükler. Mağazadaki "Chrome'a
//! ekle" bizde çalışmadığı için, mağaza sayfasında adres çubuğuna kendi
//! düğmemizi koyuyoruz: paket (CRX) Google'ın güncelleme sunucusundan iner,
//! içindeki zip açılır ve klasör motora verilir.

use super::*;
use crate::engine::Background;
use serde_json::Value;
use std::{
    io::Cursor,
    path::{Path, PathBuf},
};
use windows::Win32::{
    System::Com::{CoCreateInstance, CoInitializeEx, CoTaskMemFree, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED},
    UI::Shell::{FileOpenDialog, IFileOpenDialog, FOS_FORCEFILESYSTEM, FOS_PICKFOLDERS, SIGDN_FILESYSPATH},
};

pub const POPUP_CLASS: PCWSTR = w!("SqyweinPopup");
const POPUP_W: f32 = 380.0;
const POPUP_H: f32 = 520.0;
pub(super) const TIMER_EXPAND: usize = 9;

/// Önizlemenin sekmeye büyüme animasyonu (ekran pikseli).
pub(super) struct Expand {
    start: Instant,
    from: RECT,
    to: RECT,
    tab: u32,
}

/// Ağdan dosya indirir (arka plan iş parçacığında çağrılır).
pub fn download_file(url: &str, path: &Path) -> bool {
    crate::net::fetch(url, 64 * 1024 * 1024).is_some_and(|bytes| std::fs::write(path, bytes).is_ok())
}

/// Mağaza sayfasındaysa uzantının mağaza kimliği.
pub(super) fn store_id(url: &str) -> Option<String> {
    let rest = url.strip_prefix("https://chromewebstore.google.com/detail/")?;
    let rest = rest.split(['?', '#']).next()?;
    let id = rest.trim_end_matches('/').rsplit('/').next()?;
    (id.len() == 32 && id.bytes().all(|b| (b'a'..=b'p').contains(&b))).then(|| id.to_string())
}

/// CRX paketinden zip kısmını çıkarır (CRX2 ve CRX3).
pub(super) fn crx_zip(bytes: &[u8]) -> Option<&[u8]> {
    let u32_at = |i: usize| bytes.get(i..i + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize);
    if bytes.get(0..4)? != b"Cr24" {
        // Bazen doğrudan zip gelir.
        return bytes.starts_with(b"PK").then_some(bytes);
    }
    let start = match u32_at(4)? {
        3 => 12 + u32_at(8)?,
        2 => 16 + u32_at(8)? + u32_at(12)?,
        _ => return None,
    };
    bytes.get(start..)
}

// CRX3 stores signing public keys in protobuf proofs. Preserve the package's
// identity in manifest.key so updates from a fresh staging directory keep the ID.
fn varint(bytes: &mut &[u8]) -> Option<u64> {
    let mut value = 0u64;
    for shift in (0..=63).step_by(7) {
        let (&byte, rest) = bytes.split_first()?; *bytes = rest;
        if shift == 63 && byte > 1 { return None; }
        value |= ((byte & 127) as u64) << shift;
        if byte & 128 == 0 { return Some(value); }
    }
    None
}
fn protobuf_fields(mut bytes: &[u8]) -> Option<Vec<(u64, &[u8])>> {
    let mut out = Vec::new();
    while !bytes.is_empty() {
        let tag = varint(&mut bytes)?;
        match tag & 7 {
            0 => { varint(&mut bytes)?; }
            1 => { bytes = bytes.get(8..)?; }
            2 => { let len = usize::try_from(varint(&mut bytes)?).ok()?; let data = bytes.get(..len)?; bytes = bytes.get(len..)?; out.push((tag >> 3, data)); }
            5 => { bytes = bytes.get(4..)?; }
            _ => return None,
        }
    }
    Some(out)
}
fn store_public_key<'a>(bytes: &'a [u8], store: &str) -> Option<&'a [u8]> {
    let number = |offset| bytes.get(offset..offset+4).map(|b| u32::from_le_bytes(b.try_into().unwrap()) as usize);
    if bytes.get(..4)? != b"Cr24" { return None; }
    let keys = match number(4)? {
        2 => vec![bytes.get(16..16usize.checked_add(number(8)?)?)?],
        3 => {
            let header = bytes.get(12..12usize.checked_add(number(8)?)?)?;
            protobuf_fields(header)?.into_iter().filter(|(tag,_)| *tag == 2 || *tag == 3).filter_map(|(_, proof)| protobuf_fields(proof)?.into_iter().find(|(tag,_)| *tag == 1).map(|(_, key)| key)).collect()
        }
        _ => return None,
    };
    keys.into_iter().find(|key| {
        let hex = crate::update::sha256_hex(key);
        let id: String = hex[..32].chars().map(|c| (b'a' + c.to_digit(16).unwrap() as u8) as char).collect();
        id == store
    })
}
fn bind_store_identity(bytes: &[u8], store: &str, dir: &Path) -> std::result::Result<(), String> {
    let key = store_public_key(bytes, store).ok_or("Paketin mağaza kimliği doğrulanamadı")?;
    let mut m = manifest(dir).ok_or("Manifest okunamadı")?;
    m["key"] = Value::String(crate::image::base64(key));
    std::fs::write(dir.join("manifest.json"), serde_json::to_vec(&m).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

fn unpack(bytes: &[u8], dir: &Path) -> std::result::Result<(), String> {
    let zip = crx_zip(bytes).ok_or("Paket biçimi tanınmadı")?;
    let mut archive = zip::ZipArchive::new(Cursor::new(zip)).map_err(|e| e.to_string())?;
    if archive.len() > 10000 { return Err("Paket çok fazla dosya içeriyor".into()); }
    let mut expanded = 0u64;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| e.to_string())?;
        expanded = expanded.checked_add(file.size()).ok_or("Paket çok büyük")?;
        if expanded > 256 * 1024 * 1024 { return Err("Açılmış paket 256 MB sınırını aşıyor".into()); }
        let Some(rel) = file.enclosed_name() else { return Err("Güvensiz paket yolu".into()) };
        // "_metadata" mağazanın imza klasörüdür; paketlenmemiş yüklemede reddedilir.
        if rel.components().next().is_some_and(|c| c.as_os_str() == "_metadata") {
            continue;
        }
        let out = dir.join(rel);
        if file.is_dir() {
            std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
            continue;
        }
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut f = std::fs::File::create(&out).map_err(|e| e.to_string())?;
        std::io::copy(&mut file, &mut f).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn manifest(folder: &Path) -> Option<Value> {
    let text = std::fs::read(folder.join("manifest.json")).ok()?;
    let text = text.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(&text);
    serde_json::from_slice(text).ok()
}

/// 32 piksele en yakın simge.
fn icon_path(m: &Value) -> Option<String> {
    let pick = |icons: &Value| -> Option<String> {
        if let Some(s) = icons.as_str() {
            return Some(s.to_string());
        }
        let obj = icons.as_object()?;
        obj.iter()
            .filter_map(|(k, v)| Some((k.parse::<i32>().ok()?, v.as_str()?.to_string())))
            // Eşitlikte büyük olan seçilir: küçültmek büyütmekten daha net.
            .min_by_key(|(size, _)| ((size - 32).abs(), -size))
            .map(|(_, p)| p)
    };
    let action = m.get("action").or_else(|| m.get("browser_action"));
    action.and_then(|a| a.get("default_icon")).and_then(pick).or_else(|| m.get("icons").and_then(pick))
}

fn popup_path(m: &Value) -> Option<String> {
    let action = m.get("action").or_else(|| m.get("browser_action"))?;
    action.get("default_popup")?.as_str().filter(|s| !s.is_empty()).map(str::to_string)
}

fn options_path(m: &Value) -> Option<String> {
    m.get("options_page")
        .and_then(Value::as_str)
        .or_else(|| m.get("options_ui").and_then(|o| o.get("page")).and_then(Value::as_str))
        .map(str::to_string)
}

impl App {
    pub(super) fn store_page_id(&self) -> Option<String> {
        store_id(&self.tabs.get(self.active)?.url)
    }

    /// Sabitlenmiş ve şu an yüklü, açık uzantılar (araç çubuğunda görünenler).
    pub(super) fn pinned_extensions(&self) -> Vec<String> {
        self.state
            .pinned_extensions
            .iter()
            .filter(|id| self.extensions.iter().any(|e| &e.id == *id && e.enabled))
            .cloned()
            .collect()
    }

    pub(super) fn refresh_extensions(&self) {
        if let Some(view) = self.any_view() {
            engine::list_extensions(&view);
        }
    }

    pub(super) fn on_extensions(&mut self, mut list: Vec<ExtensionInfo>) {
        // Motorun kendi bileşenleri (PDF görüntüleyici vb.) listelenmez; yalnızca
        // kullanıcının eklediği uzantılar görünür.
        list.retain(|e| self.state.extension_folders.contains_key(&e.id));
        for e in &list {
            if self.ext_icons.contains_key(&e.id) {
                continue;
            }
            let Some(folder) = self.state.extension_folders.get(&e.id).map(PathBuf::from) else { continue };
            if let Some(img) = manifest(&folder).and_then(|m| icon_path(&m)).and_then(|p| crate::image::from_file(&folder.join(p), 32)) {
                self.ext_icons.insert(e.id.clone(), Rc::new(img));
            }
        }
        self.extensions = list;
        if self.menu.as_ref().is_some_and(|m| m.kind == MenuKind::Extensions) {
            let items = self.extension_menu_items();
            if let Some(m) = &mut self.menu {
                m.set_items(items);
            }
        }
        self.notify_pages("extensions");
        self.layout_children();
        self.invalidate();
    }

    pub(super) fn extension_menu_items(&self) -> Vec<Item> {
        let mut items = Vec::new();
        if self.extensions.is_empty() {
            items.push(Item::disabled("Henüz uzantı yok"));
        }
        for (i, e) in self.extensions.iter().enumerate() {
            let pinned = self.state.pinned_extensions.contains(&e.id);
            items.push(Item::Action {
                label: e.name.clone(),
                hint: if !e.enabled { "Kapalı".into() } else if pinned { "Sabitlendi".into() } else { "Sabitle".into() },
                cmd: MenuCmd::OpenExtension(i),
                enabled: e.enabled,
                side: Some(MenuCmd::PinExtension(i)),
                icon: self.ext_icons.get(&e.id).cloned(),
            });
        }
        items.push(Item::Sep);
        items.push(Item::act("Uzantıları yönet", "", MenuCmd::Extensions));
        items.push(Item::act("Chrome Web Mağazası", "", MenuCmd::Store));
        items
    }

    pub(super) fn toggle_pin(&mut self, i: usize) {
        let Some(id) = self.extensions.get(i).map(|e| e.id.clone()) else { return };
        self.set_pinned(&id, !self.state.pinned_extensions.contains(&id));
    }

    pub(super) fn set_pinned(&mut self, id: &str, pinned: bool) {
        self.state.pinned_extensions.retain(|p| p != id);
        if pinned {
            self.state.pinned_extensions.push(id.to_string());
        }
        self.dirty = true;
        self.layout_children();
        self.notify_pages("extensions");
        self.invalidate();
    }

    /// Mağazadaki uzantıyı indirip kurar.
    pub(super) fn install_store(&mut self, id: String) { self.install_store_package(id, false); }
    pub(super) fn update_extension(&mut self, installed: &str) {
        let store = self.state.store_installs.iter().find(|(_, id)| id.as_str() == installed).map(|(store, _)| store.clone());
        if let Some(store) = store { self.install_store_package(store, true); }
    }
    fn install_store_package(&mut self, id: String, update: bool) {
        if id.len() != 32 || !id.bytes().all(|b| (b'a'..=b'p').contains(&b)) || self.installing || (!update && self.state.store_installs.get(&id).is_some_and(|installed| self.extensions.iter().any(|e| &e.id == installed))) {
            return;
        }
        self.pending_update_enabled = if update { self.state.store_installs.get(&id).and_then(|installed| self.extensions.iter().find(|e| &e.id == installed)).map(|e| e.enabled) } else { None };
        self.installing = true;
        self.animate();
        let major = if self.browser_major > 0 { self.browser_major } else { 130 };
        let dir = state::root().join("extensions").join(format!("{id}-{}", crate::history::now_iso().replace([':', '.'], "-")));
        self.pending_install = Some((id.clone(), dir.clone()));
        std::thread::spawn(move || {
            let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
            let url = format!(
                "https://clients2.google.com/service/update2/crx?response=redirect&prodversion={major}.0&acceptformat=crx2,crx3&x=id%3D{id}%26uc"
            );
            let temp = std::env::temp_dir().join(format!("sqywein-{id}.crx"));
            let result = if let Some(bytes) = crate::net::fetch(&url, 64 * 1024 * 1024) {
                unpack(&bytes, &dir).and_then(|_| bind_store_identity(&bytes, &id, &dir)).map(|_| dir)
            } else {
                Err("İndirilemedi".into())
            };
            let _ = std::fs::remove_file(&temp);
            engine::push_background(Background::ExtensionUnpacked(result));
        });
        self.invalidate();
    }

    fn extension_status(&self, ok: bool, message: &str) {
        let json = serde_json::json!({"type":"extensions.status","ok":ok,"message":message}).to_string();
        for t in &self.tabs { if t.url == url::internal("extensions") { if let Some(v) = &t.view { engine::post_json(v, &json); } } }
    }
    pub(super) fn on_extension_unpacked(&mut self, result: std::result::Result<PathBuf, String>) {
        if let Err(error) = &result { self.extension_status(false, error); }
        match (result, self.any_view()) {
            (Ok(dir), Some(_)) => {
                let Some(m) = manifest(&dir) else { self.installing = false; self.pending_install = None; return };
                let name = m["name"].as_str().unwrap_or("Uzantı");
                let privileges: Vec<_> = ["permissions", "host_permissions"].iter().flat_map(|k| m[*k].as_array().into_iter().flatten()).filter_map(Value::as_str).collect();
                let text = format!("{} kurulacak.\n\nİstenen izinler:\n{}\n\nKurulsun mu?", name, if privileges.is_empty() { "Ek izin yok".into() } else { privileges.join("\n") });
                let hwnd = self.hwnd;
                defer(move || {
                    let allowed = unsafe { MessageBoxW(Some(hwnd), &HSTRING::from(&text), w!("Uzantı izinleri"), MB_YESNO | MB_ICONQUESTION) } == IDYES;
                    with_app(|app| {
                        if allowed { if let Some(v) = app.any_view() { engine::add_extension(&v, &dir); return; } }
                        app.installing = false; app.pending_install = None; app.invalidate();
                    });
                });
            },
            _ => {
                self.installing = false;
                self.pending_install = None;
                self.invalidate();
            }
        }
    }

    pub(super) fn on_extension_added(&mut self, extension: Option<ICoreWebView2BrowserExtension>) {
        if let (Some(ext), Some(enabled)) = (&extension, self.pending_update_enabled.take()) { engine::enable_extension(ext, enabled); }
        let id = extension.map(|ext| engine::string(|p| unsafe { ext.Id(p) }));
        self.installing = false;
        if id.is_none() { self.pending_update_enabled = None; self.extension_status(false, "Uzantı kurulamadı. WebView2 desteğini ve uzantı manifestini kontrol et."); }
        else { self.extension_status(true, "Uzantı kuruldu veya güncellendi."); }
        if let (Some(id), Some((store, dir))) = (id, self.pending_install.take()) {
            if !store.is_empty() {
                if let Some(old) = self.state.store_installs.insert(store, id.clone()).filter(|old| old != &id) {
                    if let Some(e) = self.extensions.iter().find(|e| e.id == old) { engine::remove_extension(&e.ext); }
                    for pinned in &mut self.state.pinned_extensions { if pinned == &old { *pinned = id.clone(); } }
                }
            }
            self.state.extension_folders.insert(id, dir.to_string_lossy().into_owned());
            self.dirty = true;
        }
        self.refresh_extensions();
        self.invalidate();
    }

    /// Bilgisayardaki bir uzantı klasörünü yükler.
    pub(super) fn load_unpacked(&mut self) {
        let hwnd = self.hwnd;
        defer(move || {
            let folder = unsafe { pick_folder(hwnd) };
            if let Some(folder) = folder {
                with_app(|app| {
                    if let Some(view) = app.any_view() {
                        let _ = view;
                        app.pending_install = Some((String::new(), folder.clone()));
                        app.installing = true;
                        app.on_extension_unpacked(Ok(folder));
                    }
                });
            }
        });
    }

    pub(super) fn remove_extension(&mut self, id: &str) {
        if let Some(e) = self.extensions.iter().find(|e| e.id == id) {
            engine::remove_extension(&e.ext);
        }
        self.state.pinned_extensions.retain(|p| p != id);
        self.state.store_installs.retain(|_, v| v != id);
        if let Some(folder) = self.state.extension_folders.remove(id) {
            // Yalnızca bizim indirdiğimiz klasörler silinir.
            let ours = state::root().join("extensions");
            if Path::new(&folder).starts_with(&ours) {
                let _ = std::fs::remove_dir_all(&folder);
            }
        }
        self.ext_icons.remove(id);
        self.dirty = true;
        self.refresh_extensions();
    }

    pub(super) fn enable_extension(&mut self, id: &str, on: bool) {
        if let Some(e) = self.extensions.iter().find(|e| e.id == id) {
            engine::enable_extension(&e.ext, on);
        }
        self.refresh_extensions();
    }

    // ---------- Uzantı penceresi ----------

    /// Uzantının küçük penceresini simgesinin altında açar; yoksa ayar sayfasını sekmede açar.
    pub(super) fn open_extension_popup(&mut self, id: &str, pinned_index: Option<usize>) {
        self.close_popup();
        let m = self.state.extension_folders.get(id).and_then(|f| manifest(Path::new(f)));
        let page = m.as_ref().and_then(popup_path);
        let Some(page) = page else {
            if let Some(opt) = m.as_ref().and_then(options_path) {
                self.new_tab(format!("chrome-extension://{id}/{}", opt.trim_start_matches('/')), None);
            }
            return;
        };
        let l = self.layout();
        let anchor = pinned_index.and_then(|i| l.pinned.get(i).copied()).unwrap_or(l.puzzle);
        let r = self.px(anchor);
        let s = self.scale();
        let (w, h) = ((POPUP_W * s) as i32, (POPUP_H * s) as i32);
        let mut pt = POINT { x: r.right, y: r.bottom + (4.0 * s) as i32 };
        unsafe { let _ = ClientToScreen(self.hwnd, &mut pt); }
        let Ok(instance) = (unsafe { GetModuleHandleW(None) }) else { return };
        let Ok(hwnd) = (unsafe {
            CreateWindowExW(
                WS_EX_TOOLWINDOW,
                POPUP_CLASS,
                w!(""),
                WS_POPUP | WS_CLIPCHILDREN,
                pt.x - w,
                pt.y,
                w,
                h,
                Some(self.hwnd),
                None,
                Some(instance.into()),
                None,
            )
        }) else {
            return;
        };
        unsafe {
            let dark: i32 = i32::from(is_dark());
            let _ = DwmSetWindowAttribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE, &dark as *const _ as _, 4);
            let corner = DWMWCP_ROUND;
            let _ = DwmSetWindowAttribute(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE, &corner as *const _ as _, 4);
        }
        let url = format!("chrome-extension://{id}/{}", page.trim_start_matches('/'));
        self.popup = Some(Popup { hwnd, ctrl: None, url, gfx: None, glass: false });
        if let Some(env) = self.env.clone() {
            let _ = engine::create_view(&env, hwnd, POPUP_VIEW, &self.engine_profile(), false);
        }
        defer(move || unsafe { let _ = ShowWindow(hwnd, SW_SHOW); });
    }

    pub(super) fn on_popup_view_ready(&mut self, ctrl: Option<ICoreWebView2Controller>) {
        let Some(ctrl) = ctrl else { return };
        let Some(popup) = &mut self.popup else {
            unsafe { let _ = ctrl.Close(); }
            return;
        };
        let mut parent = HWND::default();
        unsafe { let _ = ctrl.ParentWindow(&mut parent); }
        if parent != popup.hwnd { unsafe { let _ = ctrl.Close(); } return; }
        let preview = matches!(self.surface, Some(super::overlay::Surface::Link(_) | super::overlay::Surface::File(_)));
        let configured = if preview { engine::configure_preview(&ctrl, POPUP_VIEW, raised(), &self.state.settings, &popup.url) } else { engine::configure(&ctrl, POPUP_VIEW, raised(), &self.state.settings) };
        let Ok(view) = configured else { return };
        if self.surface.is_some() && !preview {
            if popup.glass {
                if let Ok(c2) = ctrl.cast::<ICoreWebView2Controller2>() { unsafe { let _ = c2.SetDefaultBackgroundColor(COREWEBVIEW2_COLOR { A: 0, R: 0, G: 0, B: 0 }); } }
            }
            let setup = format!("window.__sqGlass={};window.__sqMotion={};", popup.glass, motion_enabled());
            unsafe { let _ = view.AddScriptToExecuteOnDocumentCreated(&HSTRING::from(setup), &webview2_com::AddScriptToExecuteOnDocumentCreatedCompletedHandler::create(Box::new(|_, _| Ok(())))); }
        }
        let mut rc = RECT::default();
        unsafe {
            let _ = GetClientRect(popup.hwnd, &mut rc);
            if preview { rc.top = (48.0 * self.dpi as f32 / 96.0) as i32; }
            let _ = ctrl.SetBounds(rc);
            let _ = ctrl.SetIsVisible(true);
            let _ = view.Navigate(&wide(&popup.url));
            if IsWindowVisible(popup.hwnd).as_bool() { let _ = ctrl.MoveFocus(COREWEBVIEW2_MOVE_FOCUS_REASON_PROGRAMMATIC); }
        }
        popup.ctrl = Some(ctrl);
    }

    pub(super) fn cleanup_auth_popups(&mut self, all: bool) {
        if all { for (_, popup) in self.auth_popups.drain() { self.retired_auth.push((Instant::now(), popup)); } }
        let mut keep = Vec::new();
        for (when, popup) in self.retired_auth.drain(..) {
            if !all && when.elapsed() < Duration::from_secs(1) { keep.push((when, popup)); continue; }
            if let Some(c) = popup.ctrl { unsafe { let _ = c.Close(); } }
            let hwnd = popup.hwnd; defer(move || unsafe { let _ = DestroyWindow(hwnd); });
        }
        self.retired_auth = keep;
    }

    /// "Sekmede aç": sayfa arkada yeni sekmede yüklenirken önizleme penceresi
    /// sekmenin içerik alanına doğru büyür; sekme hazır olunca sessizce kapanır.
    pub(super) fn expand_preview(&mut self, target: String) {
        let Some(hwnd) = self.popup.as_ref().map(|p| p.hwnd) else { return };
        let mut from = RECT::default();
        unsafe { let _ = GetWindowRect(hwnd, &mut from); }
        let l = self.layout();
        let c = self.px(l.content);
        let (mut tl, mut br) = (POINT { x: c.left, y: c.top }, POINT { x: c.right, y: c.bottom });
        unsafe { let _ = ClientToScreen(self.hwnd, &mut tl); let _ = ClientToScreen(self.hwnd, &mut br); }
        // Başlık şeridi sekme şeridinin üstüne oturur; sayfa tam içerik alanına denk gelir.
        let header = (48.0 * self.scale()) as i32;
        let to = RECT { left: tl.x, top: tl.y - header, right: br.x, bottom: br.y };
        self.new_tab(target, None);
        let tab = self.tabs[self.active].id;
        if !motion_enabled() {
            self.close_popup();
            self.focus_content();
            return;
        }
        self.expand = Some(Expand { start: Instant::now(), from, to, tab });
        unsafe {
            let _ = windows::Win32::Media::timeBeginPeriod(1);
            SetTimer(Some(self.hwnd), TIMER_EXPAND, 8, None);
        }
    }

    pub(super) fn expand_tick(&mut self) {
        let Some(e) = &self.expand else { unsafe { let _ = KillTimer(Some(self.hwnd), TIMER_EXPAND); } return };
        let elapsed = e.start.elapsed().as_secs_f32();
        let t = (elapsed / 0.26).min(1.0);
        // Yumuşak başlayıp yumuşak biten geçiş.
        let k = if t < 0.5 { 4.0 * t * t * t } else { 1.0 - (-2.0 * t + 2.0).powi(3) / 2.0 };
        let lerp = |a: i32, b: i32| a + ((b - a) as f32 * k).round() as i32;
        let r = RECT { left: lerp(e.from.left, e.to.left), top: lerp(e.from.top, e.to.top), right: lerp(e.from.right, e.to.right), bottom: lerp(e.from.bottom, e.to.bottom) };
        let (tab, done) = (e.tab, t >= 1.0);
        if let Some(popup) = &self.popup {
            unsafe {
                let _ = SetWindowPos(popup.hwnd, None, r.left, r.top, r.right - r.left, r.bottom - r.top, SWP_NOZORDER | SWP_NOACTIVATE);
                if let Some(ctrl) = &popup.ctrl {
                    let header = (48.0 * self.scale()) as i32;
                    let _ = ctrl.SetBounds(RECT { left: 0, top: header, right: r.right - r.left, bottom: r.bottom - r.top });
                }
                let _ = InvalidateRect(Some(popup.hwnd), None, false);
            }
        }
        // Büyüme bitince arkadaki sekme yüklenmişse (en geç 1,2 sn) önizleme kalkar.
        let ready = self.index_of(tab).is_none_or(|i| self.tabs[i].has_loaded && !self.tabs[i].loading);
        if done && (ready || elapsed > 1.2) {
            self.expand = None;
            unsafe {
                let _ = KillTimer(Some(self.hwnd), TIMER_EXPAND);
                let _ = windows::Win32::Media::timeEndPeriod(1);
            }
            self.close_popup();
            self.focus_content();
        }
    }

    pub(super) fn close_popup(&mut self) {
        if let Some(cancel) = self.preview_cancel.take() { cancel.store(true, std::sync::atomic::Ordering::Relaxed); }
        unsafe { let _ = KillTimer(Some(self.hwnd), super::overlay::TIMER_THUMBNAIL); }
        self.thumbnail_queue.clear(); self.thumbnail_inflight = None;
        let overview=matches!(self.surface,Some(super::overlay::Surface::Tabs));
        self.surface = None;
        if overview && self.tabs.get(self.active).is_some_and(|t|!t.url.is_empty()&&t.error.is_none()){if let Some(c)=self.tabs[self.active].ctrl.as_ref(){unsafe{let _=c.SetIsVisible(true);}}}
        self.overlay_generation = self.overlay_generation.wrapping_add(1);
        if let Some(p) = self.popup.take() {
            if let Some(c) = p.ctrl {
                unsafe { let _ = c.Close(); }
            }
            let hwnd = p.hwnd;
            defer(move || unsafe { let _ = DestroyWindow(hwnd); });
        }
    }

    pub(super) fn popup_message(&mut self, hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> Option<LRESULT> {
        if self.popup.as_ref().map(|p| p.hwnd) != Some(hwnd) {
            return None;
        }
        if matches!(self.surface, Some(super::overlay::Surface::Link(_) | super::overlay::Surface::File(_))) {
            match msg {
                WM_LBUTTONUP => {
                    let scale = self.scale();
                    let x = (lparam.0 & 0xffff) as i16 as f32 / scale;
                    let y = ((lparam.0 >> 16) & 0xffff) as i16 as f32 / scale;
                    let mut rc = RECT::default(); unsafe { let _ = GetClientRect(hwnd, &mut rc); }
                    let width = rc.right as f32 / scale;
                    if y < 48.0 && x > width - 46.0 { self.close_popup(); self.focus_content(); }
                    else if y < 48.0 && x > width - 175.0 {
                        let target = match &self.surface { Some(super::overlay::Surface::Link(target) | super::overlay::Surface::File(target)) => Some(target.clone()), _ => None };
                        if let Some(target) = target { self.expand_preview(target); }
                    }
                    return Some(LRESULT(0));
                }
                WM_PAINT => {
                    let mut ps = PAINTSTRUCT::default(); let mut rc = RECT::default();
                    unsafe { BeginPaint(hwnd, &mut ps); let _ = GetClientRect(hwnd, &mut rc); }
                    if let Some(popup) = &mut self.popup {
                        if let Some(g) = &mut popup.gfx {
                            if g.begin(hwnd, rc.right as u32, rc.bottom as u32) {
                                let width = rc.right as f32 * 96.0 / self.dpi as f32;
                                g.clear(raised());
                                g.text(&if popup.url.starts_with("file://") { "PDF önizlemesi".to_string() } else { format!("Önizleme · {}", url::host(&popup.url)) }, Rect::new(16.0, 0.0, (width - 195.0).max(10.0), 48.0), Font::Ui, ink(), Align::Left);
                                g.text("Sekmede aç", Rect::new(width - 175.0, 0.0, 129.0, 48.0), Font::UiBold, accent(), Align::Center);
                                g.text(ICON_CLOSE, Rect::new(width - 46.0, 0.0, 40.0, 48.0), Font::SmallIcon, ink(), Align::Center); g.end();
                            }
                        }
                    }
                    unsafe { let _ = EndPaint(hwnd, &ps); } return Some(LRESULT(0));
                }
                _ => {},
            }
        }
        match msg {
            WM_ACTIVATE if (wparam.0 & 0xFFFF) as u32 == WA_INACTIVE && self.expand.is_some() => Some(LRESULT(0)),
            WM_ACTIVATE if (wparam.0 & 0xFFFF) as u32 == WA_INACTIVE => {
                if matches!(self.surface,Some(super::overlay::Surface::Tabs)) {return Some(LRESULT(0));}
                self.close_popup();
                Some(LRESULT(0))
            }
            WM_KEYDOWN if wparam.0 as u16 == VK_ESCAPE.0 => {
                self.close_popup();
                Some(LRESULT(0))
            }
            WM_PAINT => {
                let mut ps = PAINTSTRUCT::default();
                unsafe {
                    let hdc = BeginPaint(hwnd, &mut ps);
                    if !self.popup.as_ref().is_some_and(|p| p.glass) {
                        let brush = CreateSolidBrush(COLORREF(colorref(raised())));
                        FillRect(hdc, &ps.rcPaint, brush);
                        let _ = DeleteObject(brush.into());
                    }
                    let _ = EndPaint(hwnd, &ps);
                }
                Some(LRESULT(0))
            }
            _ => None,
        }
    }

    pub(super) fn extensions_json(&self) -> Value {
        let items: Vec<Value> = self
            .extensions
            .iter()
            .map(|e| {
                let icon = self
                    .state
                    .extension_folders
                    .get(&e.id)
                    .and_then(|f| {
                        let folder = PathBuf::from(f);
                        manifest(&folder).and_then(|m| icon_path(&m)).map(|p| crate::image::png_data_url(&folder.join(p)))
                    })
                    .unwrap_or_default();
                serde_json::json!({
                    "id": e.id,
                    "name": e.name,
                    "enabled": e.enabled,
                    "pinned": self.state.pinned_extensions.contains(&e.id),
                    "icon": icon,
                    "updatable": self.state.store_installs.values().any(|id| id == &e.id),
                })
            })
            .collect();
        serde_json::json!({ "type": "extensions.list", "supported": true, "items": items })
    }
}

unsafe fn pick_folder(owner: HWND) -> Option<PathBuf> {
    let dialog: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).ok()?;
    let options = dialog.GetOptions().ok()?;
    dialog.SetOptions(options | FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM).ok()?;
    let _ = dialog.SetTitle(w!("Uzantı klasörünü seç"));
    dialog.Show(Some(owner)).ok()?;
    let item = dialog.GetResult().ok()?;
    let name = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
    let path = name.to_string().ok().map(PathBuf::from);
    CoTaskMemFree(Some(name.0 as _));
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_truncated_package_metadata() {
        assert!(protobuf_fields(&[0x12, 0xff]).is_none());
        assert!(store_public_key(b"Cr24", "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").is_none());
        assert!(varint(&mut &[0xff; 10][..]).is_none());
    }

    #[test]
    fn magaza_kimligi() {
        let id = "cjpalhdlnbpafiamejdnhcphjbkeiagm";
        assert_eq!(store_id(&format!("https://chromewebstore.google.com/detail/ublock-origin/{id}")).as_deref(), Some(id));
        assert_eq!(store_id(&format!("https://chromewebstore.google.com/detail/x/{id}?hl=tr")).as_deref(), Some(id));
        assert!(store_id("https://chromewebstore.google.com/").is_none());
        assert!(store_id("https://example.com/detail/x/abc").is_none());
    }

    #[test]
    fn crx_baslik() {
        let mut crx3 = b"Cr24".to_vec();
        crx3.extend(3u32.to_le_bytes());
        crx3.extend(2u32.to_le_bytes());
        crx3.extend([0, 0]);
        crx3.extend(b"PK..");
        assert_eq!(crx_zip(&crx3), Some(&b"PK.."[..]));
        assert_eq!(crx_zip(b"PKzip"), Some(&b"PKzip"[..]));
        assert!(crx_zip(b"nope").is_none());
    }

    /// Ağ gerektirir: `cargo test -- --ignored magazadan_indir`
    #[test]
    #[ignore]
    fn magazadan_indir() {
        let id = "eimadpbcbfnmbkopoojfekhnkhdbieeh"; // Dark Reader
        let url = format!("https://clients2.google.com/service/update2/crx?response=redirect&prodversion=156.0&acceptformat=crx2,crx3&x=id%3D{id}%26uc");
        let dir = std::env::temp_dir().join("sqywein-crx-test");
        let crx = dir.with_extension("crx");
        assert!(download_file(&url, &crx), "indirilemedi");
        let bytes = std::fs::read(&crx).unwrap();
        unpack(&bytes, &dir).unwrap();
        assert!(store_public_key(&bytes, id).is_some(), "mağaza kimliği eşleşmiyor");
        bind_store_identity(&bytes, id, &dir).unwrap();
        let m = manifest(&dir).expect("manifest yok");
        assert!(icon_path(&m).is_some());
        assert!(popup_path(&m).is_some());
        assert!(!dir.join("_metadata").exists());
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_file(&crx);
    }

    #[test]
    fn manifest_simge_ve_pencere() {
        let m: Value = serde_json::from_str(r#"{"icons":{"16":"a.png","48":"b.png","128":"c.png"},"action":{"default_popup":"popup.html"}}"#).unwrap();
        assert_eq!(icon_path(&m).as_deref(), Some("b.png"));
        assert_eq!(popup_path(&m).as_deref(), Some("popup.html"));
    }
}
