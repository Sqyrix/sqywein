//! macOS/Safari'den gelen küçük ama güzel özellikler: paylaşım paneli (QR kod,
//! bağlantıyı kopyala, Windows ile paylaş, e-posta), odak modu ve görüntü
//! içinde görüntü.

use super::*;

/// Video oynatan sayfada en büyük videoyu küçük, her şeyin üstündeki pencereye alır.
const PIP: &str = r#"(async()=>{try{if(document.pictureInPictureElement){await document.exitPictureInPicture();return {k:'pip',ok:true};}
const v=[...document.querySelectorAll('video')].filter(v=>v.readyState>0).sort((a,b)=>b.clientWidth*b.clientHeight-a.clientWidth*a.clientHeight)[0];
if(!v)return {k:'pip',ok:false,error:'Bu sayfada oynatılabilir video bulunamadı.'};
if(!document.pictureInPictureEnabled||!v.requestPictureInPicture)return {k:'pip',ok:false,error:'Bu sayfa veya WebView2 sürümü görüntü içinde görüntüyü desteklemiyor.'};
v.disablePictureInPicture=false;await v.requestPictureInPicture();return {k:'pip',ok:true};
}catch(e){return {k:'pip',ok:false,error:'Video küçük pencereye alınamadı: '+e.message};}})()"#;

/// Metni panoya koyar.
pub(super) fn copy_text(owner: HWND, text: &str) -> bool {
    use windows::Win32::System::{DataExchange::*, Memory::*, Ole::CF_UNICODETEXT};
    let wide: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
    unsafe {
        if OpenClipboard(Some(owner)).is_err() {
            return false;
        }
        let _ = EmptyClipboard();
        let ok = (|| -> windows::core::Result<()> {
            let mem = GlobalAlloc(GMEM_MOVEABLE, wide.len() * 2)?;
            let ptr = GlobalLock(mem) as *mut u16;
            if ptr.is_null() {
                let _ = windows::Win32::Foundation::GlobalFree(Some(mem));
                return Err(windows::core::Error::from_thread());
            }
            std::ptr::copy_nonoverlapping(wide.as_ptr(), ptr, wide.len());
            let _ = GlobalUnlock(mem);
            if let Err(error) = SetClipboardData(CF_UNICODETEXT.0 as u32, Some(HANDLE(mem.0))) {
                let _ = windows::Win32::Foundation::GlobalFree(Some(mem)); return Err(error);
            }
            Ok(())
        })()
        .is_ok();
        let _ = CloseClipboard();
        ok
    }
}

/// Adresin QR kodu: (kenar uzunluğu, satır satır koyu hücreler).
pub(super) fn qr_cells(text: &str) -> Option<(usize, Vec<bool>)> {
    let qr = qrcodegen::QrCode::encode_text(text, qrcodegen::QrCodeEcc::Medium).ok()?;
    let n = qr.size() as usize;
    let mut cells = Vec::with_capacity(n * n);
    for y in 0..n as i32 {
        for x in 0..n as i32 {
            cells.push(qr.get_module(x, y));
        }
    }
    Some((n, cells))
}

impl App {
    fn page_url(&self) -> Option<(String, String)> {
        let tab = self.tabs.get(self.active)?;
        (tab.url.starts_with("http://") || tab.url.starts_with("https://")).then(|| (tab.url.clone(), tab.label()))
    }

    pub(super) fn share_menu_items(&self) -> Vec<Item> {
        let Some((url, title)) = self.page_url() else { return vec![Item::disabled("Bu sayfa paylaşılamaz")] };
        let mut items = vec![Item::Header(menu::Header {
            title: if title.is_empty() { url::host(&url).to_string() } else { title },
            sub: url::host(&url).trim_start_matches("www.").to_string(),
            avatar: self.icon_for(&url),
            color: accent(),
        })];
        if let Some((size, cells)) = qr_cells(&url) {
            items.push(Item::Qr { size, cells });
        }
        items.push(Item::act("Bağlantıyı kopyala", "", MenuCmd::CopyLink));
        items.push(Item::act("Windows ile paylaş…", "", MenuCmd::ShareWindows));
        items.push(Item::act("E-postayla gönder", "", MenuCmd::EmailLink));
        items
    }

    pub(super) fn copy_link(&mut self) {
        if let Some((url, _)) = self.page_url() {
            copy_text(self.hwnd, &url);
        }
    }

    pub(super) fn email_link(&self) {
        let Some((url, title)) = self.page_url() else { return };
        let mail = format!("mailto:?subject={}&body={}", url::encode(&title), url::encode(&url));
        unsafe { let _ = windows::Win32::UI::Shell::ShellExecuteW(None, w!("open"), &HSTRING::from(mail), PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL); }
    }

    /// Windows'un paylaşım penceresi (Yakındaki Paylaşım, Mail, uygulamalar).
    pub(super) fn share_windows(&mut self) {
        use windows::{
            ApplicationModel::DataTransfer::{DataRequestedEventArgs, DataTransferManager},
            Foundation::{TypedEventHandler, Uri},
            Win32::UI::Shell::IDataTransferManagerInterop,
        };
        let Some((url, title)) = self.page_url() else { return };
        let hwnd = self.hwnd;
        let result = (|| -> windows::core::Result<()> {
            let interop = windows::core::factory::<DataTransferManager, IDataTransferManagerInterop>()?;
            let manager: DataTransferManager = unsafe { interop.GetForWindow(hwnd)? };
            if let Some(token) = self.share_token.take() {
                let _ = manager.RemoveDataRequested(token);
            }
            let handler = TypedEventHandler::<DataTransferManager, DataRequestedEventArgs>::new(move |_, args| {
                if let Some(args) = args.as_ref() {
                    let data = args.Request()?.Data()?;
                    data.Properties()?.SetTitle(&HSTRING::from(if title.is_empty() { url.as_str() } else { title.as_str() }))?;
                    data.SetWebLink(&Uri::CreateUri(&HSTRING::from(url.as_str()))?)?;
                }
                Ok(())
            });
            self.share_token = Some(manager.DataRequested(&handler)?);
            unsafe { interop.ShowShareUIForWindow(hwnd) }
        })();
        if let Err(e) = result {
            crate::log::line(&format!("PAYLAŞIM açılamadı: {e}"));
        }
    }

    /// Odak modu: araç çubuğu ve sekmeler gizlenir; fare en üste gelince geri gelir.
    pub(super) fn toggle_focus_mode(&mut self) {
        self.focus_mode = !self.focus_mode;
        self.focus_reveal = false;
        self.layout_children();
        self.invalidate();
    }

    /// Araç çubuğu şu an gizli mi (tam ekran video ya da odak modu)?
    pub(super) fn chrome_hidden(&self) -> bool {
        self.full_screen.is_some() || (self.focus_mode && !self.focus_reveal)
    }

    pub(super) fn set_focus_reveal(&mut self, on: bool) {
        if self.focus_mode && self.focus_reveal != on {
            self.focus_reveal = on;
            self.layout_children();
            self.invalidate();
        }
    }

    pub(super) fn picture_in_picture(&mut self) {
        if let Some(view) = self.active_view() {
            engine::evaluate_user_gesture(&view, self.tabs[self.active].id, self.tabs[self.active].epoch, PIP);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qr_kodu() {
        let (n, cells) = qr_cells("https://github.com/Sqyrix/sqywein").unwrap();
        assert!(n >= 21);
        assert_eq!(cells.len(), n * n);
        // Sol üst konum işaretinin köşesi koyu olmalı.
        assert!(cells[0]);
    }
}
