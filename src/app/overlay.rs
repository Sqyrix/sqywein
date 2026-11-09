//! On-demand utility surfaces. Their single WebView is destroyed when dismissed.
use super::*;
use serde_json::{json, Value};
pub(super) const TIMER_THUMBNAIL: usize = 8;

#[derive(Clone)]
pub(super) enum Surface {
    Spotlight,
    Tabs,
    Download(u32),
    Text(String),
    Link(String),
    File(String),
    Notice(String),
    Site { tab:u32, epoch:u64, origin:String },
}

impl App {
    pub(super) fn open_surface(&mut self, surface: Surface) {
        self.close_menu();
        self.close_popup();
        self.overlay_generation = self.overlay_generation.wrapping_add(1);
        let kind = match &surface { Surface::Site{..}=>"site", Surface::Spotlight => "spotlight", Surface::Tabs => "tabs", Surface::Download(_) => "download", Surface::Text(_) => "text", Surface::Notice(_) => "notice", Surface::Link(_) | Surface::File(_) => "link" };
        let scale = self.scale();
        let mut owner = RECT::default();
        unsafe { let _ = GetWindowRect(self.hwnd, &mut owner); }
        let width = ((if kind == "tabs" { 980.0 } else { 680.0 }) * scale) as i32;
        let height = ((if kind == "spotlight" { 490.0 } else { 640.0 }) * scale) as i32;
        let monitor = unsafe { MonitorFromWindow(self.hwnd, MONITOR_DEFAULTTONEAREST) };
        let mut info = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        unsafe { let _ = GetMonitorInfoW(monitor, &mut info); }
        let work = info.rcWork;
        let width = width.min((work.right - work.left - 24).max(300));
        let height = height.min((work.bottom - work.top - 24).max(200));
        let x = ((owner.left + owner.right - width) / 2).clamp(work.left, (work.right - width).max(work.left));
        let y = ((owner.top + owner.bottom - height) / 2).clamp(work.top, (work.bottom - height).max(work.top));
        let Ok(instance) = (unsafe { GetModuleHandleW(None) }) else { return };
        let Ok(hwnd) = (unsafe { CreateWindowExW(WS_EX_TOOLWINDOW, extensions::POPUP_CLASS, w!("Sqywein"), WS_POPUP | WS_CLIPCHILDREN, x, y, width, height, Some(self.hwnd), None, Some(instance.into()), None) }) else { return };
        unsafe {
            let dark = i32::from(is_dark());
            let _ = DwmSetWindowAttribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE, &dark as *const _ as _, 4);
            let corner = DWMWCP_ROUND;
            let _ = DwmSetWindowAttribute(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE, &corner as *const _ as _, 4);
        }
        let glass = apply_backdrop(hwnd, self.state.settings.glass && crate::theme::system_transparency());
        self.surface = Some(surface);
        let target = match &self.surface { Some(Surface::Link(target) | Surface::File(target)) => target.clone(), _ => format!("{}#{kind}", url::internal("overlay")) };
        let gfx = if kind == "link" { Gfx::new().ok().map(|mut g| { g.set_dpi(self.dpi); g.hardware = self.gfx.hardware; g }) } else { None };
        self.popup = Some(Popup { hwnd, ctrl: None, url: target, gfx, glass });
        self.ensure_env();
        if let Some(env) = &self.env { let _ = engine::create_view(env, hwnd, POPUP_VIEW, &self.engine_profile(), false); }
        if unsafe { IsWindowVisible(self.hwnd) }.as_bool() { defer(move || unsafe { let _ = ShowWindow(hwnd, SW_SHOW); }); }
    }

    pub(super) fn surface_view(&self) -> Option<ICoreWebView2> {
        if self.surface.is_none() || matches!(self.surface, Some(Surface::Link(_) | Surface::File(_))) { return None; }
        self.popup.as_ref()?.ctrl.as_ref().and_then(|c| unsafe { c.CoreWebView2().ok() })
    }

    pub(super) fn surface_message(&mut self, tab: u32, msg: &Value) {
        if tab != POPUP_VIEW || self.surface.is_none() { return; }
        let kind = msg["type"].as_str().unwrap_or("");
        if kind.starts_with("overlay.site.") { self.site_message(kind,msg); return; }
        match kind {
            "overlay.close" => { self.close_popup(); self.focus_content(); }
            "overlay.activate" => {
                if let Some(i) = msg["id"].as_u64().and_then(|id| u32::try_from(id).ok()).and_then(|id| self.index_of(id)) {
                    self.close_popup(); self.activate(i); self.focus_content();
                }
            }
            "overlay.open" => {
                if let Some(target) = msg["url"].as_str().and_then(|s| url::to_url_with(s, self.state.settings.search_url())) {
                    self.close_popup(); self.new_tab(target, None);
                }
            }
            "overlay.copy" => {
                let ok = match &self.surface { Some(Surface::Text(text)) => extras::copy_text(self.hwnd, text), _ => false };
                if let Some(view) = self.surface_view() { engine::post_json(&view, &json!({"type":"overlay.copy","ok":ok}).to_string()); }
            }
            "overlay.command" => {
                let cmd = match msg["command"].as_str().unwrap_or("") {
                    "settings" => Some(MenuCmd::Settings), "downloads" => Some(MenuCmd::Downloads),
                    "history" => Some(MenuCmd::History), "bookmarks" => Some(MenuCmd::Bookmarks),
                    "focus" => Some(MenuCmd::FocusMode), "translate" => Some(MenuCmd::Translate),
                    "share" => Some(MenuCmd::OpenShare), "pip" => Some(MenuCmd::PictureInPicture),
                    "new" => Some(MenuCmd::NewTab), _ => None,
                };
                if let Some(cmd) = cmd { self.close_popup(); self.menu_command(cmd); }
            }
            "overlay.get" => {
                if matches!(self.surface,Some(Surface::Site{..})) { self.refresh_site_info(); return; }
                let query: String = msg["query"].as_str().unwrap_or("").chars().take(200).collect();
                let query = query.to_lowercase();
                let private = self.tabs[self.active].private;
                let tabs: Vec<Value> = self.tabs.iter().filter(|t| t.private == private).take(500).map(|t| json!({"id":t.id,"title":t.label(),"url":t.url,"sleeping":t.mem != Mem::Awake,"active":t.id==self.tabs[self.active].id})).collect();
                let history: Vec<Value> = if private { vec![] } else { self.history.recent(&query, 20).into_iter().map(|h| json!({"title":h.title,"url":h.url})).collect() };
                let bookmarks: Vec<Value> = self.state.favorites.iter().filter(|b| format!("{} {}",b.title,b.url).to_lowercase().contains(&query)).take(20).map(|b| json!({"title":b.title,"url":b.url})).collect();
                let mut reply = json!({"type":"overlay.get","query":query,"tabs":tabs,"history":history,"bookmarks":bookmarks});
                match self.surface.clone() {
                    Some(Surface::Download(id)) => { reply["preview"] = json!({"loading":true}); self.request_download_preview(id); },
                    Some(Surface::Text(text) | Surface::Notice(text)) => reply["text"] = json!(text),
                    _ => {},
                }
                if let Some(view) = self.surface_view() { engine::post_json(&view, &reply.to_string()); }
                if matches!(self.surface, Some(Surface::Tabs)) && msg["thumbnails"].as_bool() == Some(true) && self.thumbnail_inflight.is_none() && self.thumbnail_queue.is_empty() {
                        self.thumbnail_queue = self.tabs.iter().filter(|t| t.private == private && t.mem == Mem::Awake && t.view.is_some()).take(24).map(|t| t.id).collect();
                        if let Some(at) = self.thumbnail_queue.iter().position(|id| *id == self.tabs[self.active].id) { let id = self.thumbnail_queue.remove(at).unwrap(); self.thumbnail_queue.push_front(id); }
                        self.next_thumbnail();
                }
            }
            _ => {},
        }
    }

    fn next_thumbnail(&mut self) {
        while let Some(id) = self.thumbnail_queue.pop_front() {
            let view = self.index_of(id).filter(|i| self.tabs[*i].mem == Mem::Awake).and_then(|i| self.tabs[i].view.clone());
            if let Some(view) = view {
                self.thumbnail_inflight = Some(id);
                unsafe { SetTimer(Some(self.hwnd), TIMER_THUMBNAIL, 3000, None); }
                engine::capture_thumbnail(&view, self.overlay_generation, id); return;
            }
        }
    }
    pub(super) fn on_thumbnail(&mut self, generation: u64, id: u32, image: String) {
        if generation != self.overlay_generation || self.thumbnail_inflight != Some(id) || !matches!(self.surface, Some(Surface::Tabs)) { return; }
        self.thumbnail_inflight = None;
        unsafe { let _ = KillTimer(Some(self.hwnd), TIMER_THUMBNAIL); }
        if let Some(view) = self.surface_view() { engine::post_json(&view, &json!({"type":"overlay.thumbnail","id":id,"image":image}).to_string()); }
        self.next_thumbnail();
    }

    pub(super) fn thumbnail_timeout(&mut self) {
        unsafe { let _ = KillTimer(Some(self.hwnd), TIMER_THUMBNAIL); }
        let mut unavailable: Vec<u32> = self.thumbnail_queue.drain(..).collect();
        if let Some(id) = self.thumbnail_inflight.take() { unavailable.push(id); }
        if let Some(view) = self.surface_view() {
            for id in unavailable { engine::post_json(&view, &json!({"type":"overlay.thumbnail","id":id,"image":""}).to_string()); }
        }
    }

    fn request_download_preview(&mut self, id: u32) {
        let record = self.download_record(id).filter(|r| r.status == "done" && (!r.private || self.tabs[self.active].private)).cloned();
        let Some(record) = record else { engine::push_background(engine::Background::Preview(self.overlay_generation, json!({"error":"Dosya bulunamadı veya indirme tamamlanmadı."}))); return; };
        if let Some(old) = self.preview_cancel.take() { old.store(true, std::sync::atomic::Ordering::Relaxed); }
        let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)); self.preview_cancel = Some(cancel.clone());
        let generation = self.overlay_generation;
        std::thread::spawn(move || {
            // File I/O and WIC decoding never block the browser's UI thread.
            static WORK: std::sync::Mutex<()> = std::sync::Mutex::new(());
            let Ok(_work) = WORK.lock() else { return };
            if cancel.load(std::sync::atomic::Ordering::Relaxed) { return; }
            crate::net::init_thread();
            let preview = Self::download_preview(&record);
            if !cancel.load(std::sync::atomic::Ordering::Relaxed) { engine::push_background(engine::Background::Preview(generation, preview)); }
        });
    }
    pub(super) fn on_preview(&mut self, generation: u64, preview: Value) {
        if generation != self.overlay_generation || !matches!(self.surface, Some(Surface::Download(_))) { return; }
        if let Some(target) = preview["pdf"].as_str() { self.open_surface(Surface::File(target.into())); return; }
        if let Some(view) = self.surface_view() { engine::post_json(&view, &json!({"type":"overlay.preview","preview":preview}).to_string()); }
    }
    fn download_preview(record: &state::DownloadRecord) -> Value {
        let path = std::path::Path::new(&record.path);
        let Ok(meta) = path.metadata() else { return json!({"name":record.name,"error":"Dosya taşınmış ya da silinmiş."}); };
        let mut value = json!({"name":record.name,"size":meta.len()});
        if meta.len() > 16 * 1024 * 1024 { value["error"] = json!("Bu dosya hızlı önizleme sınırından büyük (16 MB)."); return value; }
        let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("").to_ascii_lowercase();
        if ext == "pdf" {
            if let Some(target) = url::to_url_with(&record.path, url::SEARCH_URL) { value["pdf"] = json!(target); return value; }
        }
        if ["png","jpg","jpeg","webp","bmp","gif","ico"].contains(&ext.as_str()) {
            if let Some(image) = crate::image::from_file(path, 1600) { value["image"] = json!(crate::image::to_data_url(&image)); }
            else { value["error"] = json!("Bu resim çözülemedi."); }
        } else if ["txt","md","csv","json","log","rs","js","css","html","xml","toml","ini","yaml","yml"].contains(&ext.as_str()) {
            use std::io::Read;
            let mut bytes = Vec::new();
            match std::fs::File::open(path).and_then(|f| f.take(512 * 1024).read_to_end(&mut bytes)) {
                Ok(_) => { value["text"] = json!(String::from_utf8_lossy(&bytes)); if meta.len() > bytes.len() as u64 { value["note"] = json!("İlk 512 KB gösteriliyor."); } }
                Err(_) => value["error"] = json!("Dosya okunamadı."),
            }
        } else { value["error"] = json!("Bu dosya türünde hızlı önizleme henüz desteklenmiyor."); }
        value
    }
}
