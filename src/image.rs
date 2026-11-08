//! Küçük resimler (site simgeleri, profil fotoğrafı, uzantı simgeleri).
//!
//! WIC ile çözülür ve istenen boyuta küçültülür; bellekte yalnızca
//! premultiplied BGRA pikseller tutulur. Direct2D bitmap'leri çizimde üretilir.

use std::{
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};
use windows::{
    core::*,
    Win32::{
        Graphics::Imaging::*,
        System::Com::{CoCreateInstance, IStream, CLSCTX_INPROC_SERVER},
    },
};

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug)]
pub struct Image {
    /// Çizim önbelleğinin anahtarı.
    pub id: u64,
    pub width: u32,
    pub height: u32,
    /// Premultiplied BGRA, satır satır.
    pub pixels: Vec<u8>,
}

fn factory() -> Result<IWICImagingFactory> {
    unsafe { CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER) }
}

fn decode_frame(factory: &IWICImagingFactory, decoder: &IWICBitmapDecoder, max: u32) -> Result<Image> {
    unsafe {
        let frame = decoder.GetFrame(0)?;
        let (mut w, mut h) = (0u32, 0u32);
        frame.GetSize(&mut w, &mut h)?;
        if w == 0 || h == 0 {
            return Err(Error::from_hresult(windows::Win32::Foundation::E_FAIL));
        }
        // Büyük resimler bellekte tam boy tutulmaz.
        let source: IWICBitmapSource = if w > max || h > max {
            let scale = max as f32 / w.max(h) as f32;
            let (nw, nh) = (((w as f32 * scale).round() as u32).max(1), ((h as f32 * scale).round() as u32).max(1));
            let scaler = factory.CreateBitmapScaler()?;
            scaler.Initialize(&frame, nw, nh, WICBitmapInterpolationModeHighQualityCubic)?;
            w = nw;
            h = nh;
            scaler.cast()?
        } else {
            frame.cast()?
        };
        let converter = factory.CreateFormatConverter()?;
        converter.Initialize(&source, &GUID_WICPixelFormat32bppPBGRA, WICBitmapDitherTypeNone, None, 0.0, WICBitmapPaletteTypeMedianCut)?;
        let stride = w * 4;
        let mut pixels = vec![0u8; (stride * h) as usize];
        converter.CopyPixels(std::ptr::null(), stride, &mut pixels)?;
        Ok(Image { id: NEXT_ID.fetch_add(1, Ordering::Relaxed), width: w, height: h, pixels })
    }
}

pub fn from_stream(stream: &IStream, max: u32) -> Option<Image> {
    let f = factory().ok()?;
    unsafe {
        let decoder = f.CreateDecoderFromStream(stream, std::ptr::null(), WICDecodeMetadataCacheOnDemand).ok()?;
        decode_frame(&f, &decoder, max).ok()
    }
}

pub fn from_bytes(bytes: &[u8], max: u32) -> Option<Image> {
    let stream = unsafe { windows::Win32::UI::Shell::SHCreateMemStream(Some(bytes)) }?;
    from_stream(&stream, max)
}

pub fn from_file(path: &Path, max: u32) -> Option<Image> {
    let f = factory().ok()?;
    unsafe {
        let decoder = f
            .CreateDecoderFromFilename(&HSTRING::from(path.as_os_str()), None, windows::Win32::Foundation::GENERIC_READ, WICDecodeMetadataCacheOnDemand)
            .ok()?;
        decode_frame(&f, &decoder, max).ok()
    }
}

/// Uzantılar sayfası için PNG'yi data: URL'sine çevirir.
pub fn png_data_url(path: &Path) -> String {
    match std::fs::read(path) {
        Ok(bytes) => format!("data:image/png;base64,{}", base64(&bytes)),
        Err(_) => String::new(),
    }
}

pub fn base64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    out
}

/// Dosyanın Windows'taki simgesi: o türü açan programın simgesi (ör. .rar için WinRAR).
/// Dosya henüz yoksa uzantıya göre bulunur.
pub fn file_icon(path: &str, exists: bool) -> Option<Image> {
    use windows::Win32::{
        Storage::FileSystem::FILE_ATTRIBUTE_NORMAL,
        UI::{
            Shell::{SHGetFileInfoW, SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON, SHGFI_USEFILEATTRIBUTES},
            WindowsAndMessaging::DestroyIcon,
        },
    };
    unsafe {
        let mut info = SHFILEINFOW::default();
        let mut flags = SHGFI_ICON | SHGFI_LARGEICON;
        if !exists {
            flags |= SHGFI_USEFILEATTRIBUTES;
        }
        let ok = SHGetFileInfoW(&HSTRING::from(path), FILE_ATTRIBUTE_NORMAL, Some(&mut info), std::mem::size_of::<SHFILEINFOW>() as u32, flags);
        if ok == 0 || info.hIcon.is_invalid() {
            return None;
        }
        let f = factory().ok();
        let img = f.as_ref().and_then(|f| {
            let bitmap = f.CreateBitmapFromHICON(info.hIcon).ok()?;
            let source: IWICBitmapSource = bitmap.cast().ok()?;
            let converter = f.CreateFormatConverter().ok()?;
            converter.Initialize(&source, &GUID_WICPixelFormat32bppPBGRA, WICBitmapDitherTypeNone, None, 0.0, WICBitmapPaletteTypeMedianCut).ok()?;
            let (mut w, mut h) = (0u32, 0u32);
            converter.GetSize(&mut w, &mut h).ok()?;
            let mut pixels = vec![0u8; (w * h * 4) as usize];
            converter.CopyPixels(std::ptr::null(), w * 4, &mut pixels).ok()?;
            Some(Image { id: NEXT_ID.fetch_add(1, Ordering::Relaxed), width: w, height: h, pixels })
        });
        let _ = DestroyIcon(info.hIcon);
        img
    }
}

/// Resmi PNG data: URL'sine çevirir (iç sayfalarda göstermek için).
pub fn to_data_url(img: &Image) -> String {
    let encode = || -> Option<Vec<u8>> {
        unsafe {
            let f = factory().ok()?;
            let bitmap = f
                .CreateBitmapFromMemory(img.width, img.height, &GUID_WICPixelFormat32bppPBGRA, img.width * 4, &img.pixels)
                .ok()?;
            let stream = windows::Win32::UI::Shell::SHCreateMemStream(None)?;
            let encoder = f.CreateEncoder(&GUID_ContainerFormatPng, std::ptr::null()).ok()?;
            encoder.Initialize(&stream, WICBitmapEncoderNoCache).ok()?;
            let mut frame = None;
            encoder.CreateNewFrame(&mut frame, std::ptr::null_mut()).ok()?;
            let frame = frame?;
            frame.Initialize(None).ok()?;
            frame.WriteSource(&bitmap, std::ptr::null()).ok()?;
            frame.Commit().ok()?;
            encoder.Commit().ok()?;
            let mut size = 0u64;
            stream.Seek(0, windows::Win32::System::Com::STREAM_SEEK_END, Some(&mut size)).ok()?;
            stream.Seek(0, windows::Win32::System::Com::STREAM_SEEK_SET, None).ok()?;
            let mut bytes = vec![0u8; size as usize];
            let mut read = 0u32;
            let _ = stream.Read(bytes.as_mut_ptr() as _, size as u32, Some(&mut read));
            bytes.truncate(read as usize);
            Some(bytes)
        }
    };
    encode().map(|b| format!("data:image/png;base64,{}", base64(&b))).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    #[test]
    fn dosya_simgeleri() {
        let _ = unsafe { windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_APARTMENTTHREADED) };
        for ext in ["rar", "txt", "pdf", "zip"] {
            let img = super::file_icon(&format!("dosya.{ext}"), false).expect(ext);
            assert!(img.width >= 16 && img.pixels.iter().any(|&b| b != 0), "{ext} boş");
            let url = super::to_data_url(&img);
            assert!(url.starts_with("data:image/png;base64,") && url.len() > 100, "{ext} png");
        }
    }

    #[test]
    fn base64_kodlama() {
        assert_eq!(super::base64(b"Man"), "TWFu");
        assert_eq!(super::base64(b"Ma"), "TWE=");
        assert_eq!(super::base64(b"M"), "TQ==");
    }
}
