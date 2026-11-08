//! Local Windows OCR: no network service, clipboard or disk image export.
use crate::{engine, image::Image};
use windows::{Graphics::Imaging::{BitmapAlphaMode, BitmapPixelFormat, SoftwareBitmap}, Media::Ocr::OcrEngine, Storage::Streams::DataWriter};

pub fn recognize(image: Image, tab: u32, epoch: u64, request: u64) {
    std::thread::spawn(move || {
        let init = unsafe { windows::Win32::System::WinRT::RoInitialize(windows::Win32::System::WinRT::RO_INIT_MULTITHREADED) };
        let initialized = init.is_ok();
        let result = (|| -> windows::core::Result<String> {
            init?;
            recognize_text(&image)
        })().map_err(|_| "Windows metin tanıma kullanılamadı. Windows dil ayarlarında OCR dil paketinin yüklü olduğunu kontrol et.".to_string());
        if initialized { unsafe { windows::Win32::System::WinRT::RoUninitialize(); } }
        engine::push_background(engine::Background::Ocr(tab, epoch, request, result));
    });
}

fn recognize_text(image: &Image) -> windows::core::Result<String> {
    let writer = DataWriter::new()?;
    writer.WriteBytes(&image.pixels)?;
    let buffer = writer.DetachBuffer()?;
    let bitmap = SoftwareBitmap::CreateCopyWithAlphaFromBuffer(&buffer, BitmapPixelFormat::Bgra8, image.width as i32, image.height as i32, BitmapAlphaMode::Premultiplied)?;
    let engine = OcrEngine::TryCreateFromUserProfileLanguages().or_else(|_| {
        let languages = OcrEngine::AvailableRecognizerLanguages()?;
        let language = languages.GetAt(0)?;
        OcrEngine::TryCreateFromLanguage(&language)
    })?;
    Ok(engine.RecognizeAsync(&bitmap)?.join()?.Text()?.to_string())
}

/// Crop the visible image using normalized CSS viewport coordinates.
pub fn crop(image: Image, rect: [f64; 4]) -> Option<Image> {
    if rect.iter().any(|n| !n.is_finite()) { return None; }
    let (w, h) = (image.width, image.height);
    let x = (rect[0].clamp(0.0, 1.0) * w as f64) as u32;
    let y = (rect[1].clamp(0.0, 1.0) * h as f64) as u32;
    let right = ((rect[0] + rect[2]).clamp(0.0, 1.0) * w as f64) as u32;
    let bottom = ((rect[1] + rect[3]).clamp(0.0, 1.0) * h as f64) as u32;
    let (cw, ch) = (right.saturating_sub(x), bottom.saturating_sub(y));
    if cw == 0 || ch == 0 { return None; }
    let mut pixels = Vec::with_capacity((cw * ch * 4) as usize);
    for row in y..bottom {
        let offset = ((row * w + x) * 4) as usize;
        pixels.extend_from_slice(&image.pixels[offset..offset + (cw * 4) as usize]);
    }
    Some(Image { id: image.id, width: cw, height: ch, pixels })
}

#[cfg(test)]
mod tests {
    #[test]
    #[ignore = "requires SQYWEIN_OCR_TEST_IMAGE and a Windows OCR language pack"]
    fn windows_ocr_fixture() {
        unsafe { windows::Win32::System::WinRT::RoInitialize(windows::Win32::System::WinRT::RO_INIT_MULTITHREADED).unwrap(); }
        let path = std::env::var("SQYWEIN_OCR_TEST_IMAGE").expect("fixture path");
        let image = crate::image::from_file(std::path::Path::new(&path), 2500).unwrap();
        let text = super::recognize_text(&image).unwrap();
        assert!(text.contains("12345"), "recognized: {text}");
        unsafe { windows::Win32::System::WinRT::RoUninitialize(); }
    }
    #[test]
    fn bounded_crop() {
        let image = || crate::image::Image { id: 1, width: 10, height: 10, pixels: vec![255; 400] };
        let crop = super::crop(image(), [0.2, 0.3, 0.4, 0.5]).unwrap();
        assert_eq!((crop.width, crop.height, crop.pixels.len()), (4, 5, 80));
        assert!(super::crop(image(), [f64::NAN, 0.0, 1.0, 1.0]).is_none());
        assert!(super::crop(image(), [2.0, 2.0, 1.0, 1.0]).is_none());
    }
}
