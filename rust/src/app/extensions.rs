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
    System::Com::Urlmon::URLDownloadToFileW,
    UI::Shell::{FileOpenDialog, IFileOpenDialog, FOS_FORCEFILESYSTEM, FOS_PICKFOLDERS, SIGDN_FILESYSPATH},
};

pub const POPUP_CLASS: PCWSTR = w!("SqyweinPopup");
const POPUP_W: f32 = 380.0;
const POPUP_H: f32 = 520.0;

/// Ağdan dosya indirir (arka plan iş parçacığında çağrılır).
pub fn download_file(url: &str, path: &Path) -> bool {
    unsafe { URLDownloadToFileW(None, &HSTRING::from(url), &HSTRING::from(path.as_os_str()), 0, None).is_ok() }
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

fn unpack(bytes: &[u8], dir: &Path) -> std::result::Result<(), String> {
    let zip = crx_zip(bytes).ok_or("Paket biçimi tanınmadı")?;
    let mut archive = zip::ZipArchive::new(Cursor::new(zip)).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_dir_all(dir);
    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| e.to_string())?;
        let Some(rel) = file.enclosed_name() else { continue };
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
    pub(super) fn install_store(&mut self, id: String) {
        if self.installing || self.state.store_installs.contains_key(&id) {
            return;
        }
        self.installing = true;
        self.animate();
        let major = if self.browser_major > 0 { self.browser_major } else { 130 };
        let dir = state::root().join("extensions").join(&id);
        self.pending_install = Some((id.clone(), dir.clone()));
        std::thread::spawn(move || {
            let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
            let url = format!(
                "https://clients2.google.com/service/update2/crx?response=redirect&prodversion={major}.0&acceptformat=crx2,crx3&x=id%3D{id}%26uc"
            );
            let temp = std::env::temp_dir().join(format!("sqywein-{id}.crx"));
            let result = if download_file(&url, &temp) {
                std::fs::read(&temp).map_err(|e| e.to_string()).and_then(|bytes| unpack(&bytes, &dir)).map(|_| dir)
            } else {
                Err("İndirilemedi".into())
            };
            let _ = std::fs::remove_file(&temp);
            engine::push_background(Background::ExtensionUnpacked(result));
        });
        self.invalidate();
    }

    pub(super) fn on_extension_unpacked(&mut self, result: std::result::Result<PathBuf, String>) {
        match (result, self.any_view()) {
            (Ok(dir), Some(view)) => engine::add_extension(&view, &dir),
            _ => {
                self.installing = false;
                self.pending_install = None;
                self.invalidate();
            }
        }
    }

    pub(super) fn on_extension_added(&mut self, id: Option<String>) {
        self.installing = false;
        if let (Some(id), Some((store, dir))) = (id, self.pending_install.take()) {
            if !store.is_empty() {
                self.state.store_installs.insert(store, id.clone());
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
                        app.pending_install = Some((String::new(), folder.clone()));
                        engine::add_extension(&view, &folder);
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
            let dark: i32 = 1;
            let _ = DwmSetWindowAttribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE, &dark as *const _ as _, 4);
            let corner = DWMWCP_ROUND;
            let _ = DwmSetWindowAttribute(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE, &corner as *const _ as _, 4);
        }
        let url = format!("chrome-extension://{id}/{}", page.trim_start_matches('/'));
        self.popup = Some(Popup { hwnd, ctrl: None, url });
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
        let Ok(view) = engine::configure(&ctrl, POPUP_VIEW, RAISED, &self.state.settings) else { return };
        let mut rc = RECT::default();
        unsafe {
            let _ = GetClientRect(popup.hwnd, &mut rc);
            let _ = ctrl.SetBounds(rc);
            let _ = ctrl.SetIsVisible(true);
            let _ = view.Navigate(&wide(&popup.url));
            let _ = ctrl.MoveFocus(COREWEBVIEW2_MOVE_FOCUS_REASON_PROGRAMMATIC);
        }
        popup.ctrl = Some(ctrl);
    }

    pub(super) fn close_popup(&mut self) {
        if let Some(p) = self.popup.take() {
            if let Some(c) = p.ctrl {
                unsafe { let _ = c.Close(); }
            }
            let hwnd = p.hwnd;
            defer(move || unsafe { let _ = DestroyWindow(hwnd); });
        }
    }

    pub(super) fn popup_message(&mut self, hwnd: HWND, msg: u32, wparam: WPARAM, _lparam: LPARAM) -> Option<LRESULT> {
        if self.popup.as_ref().map(|p| p.hwnd) != Some(hwnd) {
            return None;
        }
        match msg {
            WM_ACTIVATE if (wparam.0 & 0xFFFF) as u32 == WA_INACTIVE => {
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
                    let brush = CreateSolidBrush(COLORREF(colorref(RAISED)));
                    FillRect(hdc, &ps.rcPaint, brush);
                    let _ = DeleteObject(brush.into());
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
