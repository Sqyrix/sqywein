//! Direct2D + DirectWrite ile arayüz çizimi. Koordinatlar DIP cinsindendir.
//!
//! Hedef yazılım (CPU) ile çizilir: sekme şeridi gibi küçük bir alan için GPU
//! cihazı açmak sürece onlarca MB ekler, kazandırdığı hız ise fark edilmez.
//! Netlik için: hedef opak (ClearType çalışsın diye) ve tüm dikdörtgenler
//! cihaz piksellerine hizalanır.

use crate::{image::Image, theme::Rgb};
use std::{
    cell::{Cell, RefCell},
};
use windows::{
    core::*,
    Win32::{
        Foundation::*,
        Graphics::{
            Direct2D::{Common::*, *},
            DirectWrite::*,
            Dxgi::Common::*,
        },
    },
};
use windows_numerics::{Matrix3x2, Vector2};

#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }
    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && px < self.x + self.w && py >= self.y && py < self.y + self.h
    }
    pub fn right(&self) -> f32 {
        self.x + self.w
    }
    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }
    pub fn cx(&self) -> f32 {
        self.x + self.w / 2.0
    }
    pub fn cy(&self) -> f32 {
        self.y + self.h / 2.0
    }
    pub fn inset(&self, dx: f32, dy: f32) -> Self {
        Self::new(self.x + dx, self.y + dy, (self.w - 2.0 * dx).max(0.0), (self.h - 2.0 * dy).max(0.0))
    }
    /// Ortası aynı kalarak verilen boyuta getirilmiş kare/dikdörtgen.
    pub fn centered(&self, w: f32, h: f32) -> Self {
        Self::new(self.cx() - w / 2.0, self.cy() - h / 2.0, w, h)
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum Align {
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy)]
pub enum Font {
    Ui,
    UiBold,
    Small,
    /// Araç çubuğu simgeleri (Segoe Fluent Icons 16 px için tasarlanmıştır).
    Icon,
    /// Adres çubuğu içindeki küçük simgeler (12 px).
    MidIcon,
    /// Pencere düğmeleri ve sekme kapatma (10 px).
    SmallIcon,
    Big,
    /// Renkli emoji (hava durumu simgeleri).
    Emoji,
    EmojiSmall,
    /// Hava durumu kartındaki dev sıcaklık.
    Giant,
}

fn color(rgb: Rgb, alpha: f32) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: ((rgb >> 16) & 0xFF) as f32 / 255.0,
        g: ((rgb >> 8) & 0xFF) as f32 / 255.0,
        b: (rgb & 0xFF) as f32 / 255.0,
        a: alpha,
    }
}

fn v2(x: f32, y: f32) -> Vector2 {
    Vector2 { X: x, Y: y }
}

struct Formats {
    ui: IDWriteTextFormat,
    ui_bold: IDWriteTextFormat,
    small: IDWriteTextFormat,
    icon: IDWriteTextFormat,
    mid_icon: IDWriteTextFormat,
    small_icon: IDWriteTextFormat,
    big: IDWriteTextFormat,
    emoji: IDWriteTextFormat,
    emoji_small: IDWriteTextFormat,
    giant: IDWriteTextFormat,
}

pub struct Gfx {
    factory: ID2D1Factory,
    dwrite: IDWriteFactory,
    fmt: Formats,
    target: Option<ID2D1HwndRenderTarget>,
    brush: Option<ID2D1SolidColorBrush>,
    round_caps: Option<ID2D1StrokeStyle>,
    dpi: f32,
    /// Sonraki çizimlerin saydamlığı (animasyonlar için).
    opacity: Cell<f32>,
    /// Resimlerin bu hedefe yüklenmiş halleri; hedef yenilenince boşaltılır.
    bitmaps: RefCell<crate::cache::Cache<u64, ID2D1Bitmap>>,
    pub hardware: bool,
    /// Pencere camla çiziliyor: hedef saydamlık taşır, yazılar gri tonlamalı.
    glass: bool,
}

impl Gfx {
    pub fn new() -> Result<Self> {
        unsafe {
            let factory: ID2D1Factory = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
            let dwrite: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
            let icon_family = if has_font(&dwrite, w!("Segoe Fluent Icons")) {
                w!("Segoe Fluent Icons")
            } else {
                w!("Segoe MDL2 Assets")
            };
            let ui_family = if has_font(&dwrite, w!("Segoe UI Variable Text")) {
                w!("Segoe UI Variable Text")
            } else {
                w!("Segoe UI")
            };
            let display_family = if has_font(&dwrite, w!("Segoe UI Variable Display")) {
                w!("Segoe UI Variable Display")
            } else {
                w!("Segoe UI")
            };
            let fmt = Formats {
                // Tam sayı boyutlar: harfler piksel ızgarasına oturur, kenarlar net kalır.
                ui: text_format(&dwrite, ui_family, DWRITE_FONT_WEIGHT_NORMAL, 13.0)?,
                ui_bold: text_format(&dwrite, ui_family, DWRITE_FONT_WEIGHT_SEMI_BOLD, 13.0)?,
                small: text_format(&dwrite, ui_family, DWRITE_FONT_WEIGHT_NORMAL, 12.0)?,
                icon: text_format(&dwrite, icon_family, DWRITE_FONT_WEIGHT_NORMAL, 16.0)?,
                mid_icon: text_format(&dwrite, icon_family, DWRITE_FONT_WEIGHT_NORMAL, 12.0)?,
                small_icon: text_format(&dwrite, icon_family, DWRITE_FONT_WEIGHT_NORMAL, 10.0)?,
                big: text_format(&dwrite, display_family, DWRITE_FONT_WEIGHT_SEMI_BOLD, 30.0)?,
                emoji: text_format(&dwrite, w!("Segoe UI Emoji"), DWRITE_FONT_WEIGHT_NORMAL, 44.0)?,
                emoji_small: text_format(&dwrite, w!("Segoe UI Emoji"), DWRITE_FONT_WEIGHT_NORMAL, 17.0)?,
                giant: text_format(&dwrite, display_family, DWRITE_FONT_WEIGHT_LIGHT, 64.0)?,
            };
            let props = D2D1_STROKE_STYLE_PROPERTIES {
                startCap: D2D1_CAP_STYLE_ROUND,
                endCap: D2D1_CAP_STYLE_ROUND,
                dashCap: D2D1_CAP_STYLE_ROUND,
                lineJoin: D2D1_LINE_JOIN_ROUND,
                miterLimit: 1.0,
                dashStyle: D2D1_DASH_STYLE_SOLID,
                dashOffset: 0.0,
            };
            let round_caps = factory.CreateStrokeStyle(&props, None).ok();
            Ok(Self { factory, dwrite, fmt, target: None, brush: None, round_caps, dpi: 96.0, opacity: Cell::new(1.0), bitmaps: RefCell::new(crate::cache::Cache::new(192)), hardware: false, glass: false })
        }
    }

    pub fn set_dpi(&mut self, dpi: u32) {
        self.dpi = dpi as f32;
        if let Some(t) = &self.target {
            unsafe { t.SetDpi(self.dpi, self.dpi) };
        }
    }

    pub fn is_glass(&self) -> bool { self.glass }

    /// Cam açılıp kapanınca hedef yeniden kurulur (saydamlık modu değişir).
    pub fn set_glass(&mut self, on: bool) {
        if self.glass != on {
            self.glass = on;
            self.target = None;
            self.brush = None;
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if let Some(t) = &self.target {
            if unsafe { t.Resize(&D2D_SIZE_U { width, height }) }.is_err() {
                self.target = None;
                self.brush = None;
            }
        }
    }

    /// Çizime başlar; hedef yoksa oluşturur. Çizilemiyorsa `false`.
    pub fn begin(&mut self, hwnd: HWND, width: u32, height: u32) -> bool {
        if self.target.is_none() {
            let props = D2D1_RENDER_TARGET_PROPERTIES {
                r#type: if self.hardware { D2D1_RENDER_TARGET_TYPE_DEFAULT } else { D2D1_RENDER_TARGET_TYPE_SOFTWARE },
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    // Opak hedefte ClearType; camda saydam (önceden çarpılmış) hedef.
                    alphaMode: if self.glass { D2D1_ALPHA_MODE_PREMULTIPLIED } else { D2D1_ALPHA_MODE_IGNORE },
                },
                dpiX: self.dpi,
                dpiY: self.dpi,
                usage: D2D1_RENDER_TARGET_USAGE_GDI_COMPATIBLE,
                minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
            };
            let hwnd_props = D2D1_HWND_RENDER_TARGET_PROPERTIES {
                hwnd,
                pixelSize: D2D_SIZE_U { width, height },
                presentOptions: D2D1_PRESENT_OPTIONS_NONE,
            };
            let Ok(target) = (unsafe { self.factory.CreateHwndRenderTarget(&props, &hwnd_props) }) else {
                return false;
            };
            // ClearType saydam yüzeyde renkli saçak bırakır; camda gri tonlama.
            unsafe { target.SetTextAntialiasMode(if self.glass { D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE } else { D2D1_TEXT_ANTIALIAS_MODE_CLEARTYPE }) };
            if !self.glass {if let Ok(params)=unsafe{self.dwrite.CreateRenderingParams()}{unsafe{target.SetTextRenderingParams(&params);}}}
            if self.glass {
                // Gri tonlamada netlik: harf aralıkları piksel ızgarasına oturur (GDI uyumlu
                // ölçüm), kıvrımlar iki yönde de yumuşatılır (simetrik), kontrast ölçülü artırılır.
                // Windows'un kendi ekran ayarları (gamma, kontrast) kullanılır; yalnızca
                // ClearType kapalı ve yumuşatma iki yönlü (simetrik).
                if let (Ok(f1), Ok(system)) = (self.dwrite.cast::<IDWriteFactory1>(), unsafe { self.dwrite.CreateRenderingParams() }) {
                    let (gamma, contrast) = unsafe { (system.GetGamma(), system.GetEnhancedContrast()) };
                    if let Ok(params) = unsafe { f1.CreateCustomRenderingParams(gamma, contrast, contrast.max(1.0), 0.0, DWRITE_PIXEL_GEOMETRY_FLAT, DWRITE_RENDERING_MODE_NATURAL_SYMMETRIC) } {
                        unsafe { target.SetTextRenderingParams(&params) };
                    }
                }
            }
            self.brush = unsafe { target.CreateSolidColorBrush(&color(0, 1.0), None) }.ok();
            self.bitmaps.borrow_mut().trim_to(0);
            self.target = Some(target);
        }
        self.opacity.set(1.0);
        unsafe { self.target.as_ref().unwrap().BeginDraw() };
        true
    }

    pub fn end(&mut self) {
        if let Some(t) = &self.target {
            if unsafe { t.EndDraw(None, None) }.is_err() {
                // D2DERR_RECREATE_TARGET vb.: bir sonraki çizimde yeniden oluşturulur.
                self.target = None;
                self.brush = None;
            }
        }
    }

    pub fn set_opacity(&self, a: f32) {
        self.opacity.set(a.clamp(0.0, 1.0));
    }

    /// DIP değerini en yakın cihaz pikseline yuvarlar.
    pub fn snap(&self, v: f32) -> f32 {
        let s = self.dpi / 96.0;
        (v * s).round() / s
    }

    fn snap_rect(&self, r: Rect) -> D2D_RECT_F {
        let (l, t) = (self.snap(r.x), self.snap(r.y));
        let (rt, b) = (self.snap(r.right()), self.snap(r.bottom()));
        D2D_RECT_F { left: l, top: t, right: rt, bottom: b }
    }

    fn paint(&self, rgb: Rgb, alpha: f32) -> Option<(&ID2D1HwndRenderTarget, &ID2D1SolidColorBrush)> {
        let (t, b) = (self.target.as_ref()?, self.brush.as_ref()?);
        unsafe { b.SetColor(&color(rgb, alpha * self.opacity.get())) };
        Some((t, b))
    }

    pub fn clear(&self, rgb: Rgb) {
        if let Some(t) = &self.target {
            unsafe { t.Clear(Some(&color(rgb, 1.0))) };
        }
    }

    /// Tamamen saydam temizler: altındaki Windows camı görünür.
    pub fn clear_transparent(&self) {
        if let Some(t) = &self.target {
            unsafe { t.Clear(Some(&D2D1_COLOR_F { r: 0.0, g: 0.0, b: 0.0, a: 0.0 })) };
        }
    }

    pub fn fill_alpha(&self, r: Rect, rgb: Rgb, alpha: f32) {
        if let Some((t, b)) = self.paint(rgb, alpha) {
            unsafe { t.FillRectangle(&self.snap_rect(r), b) };
        }
    }

    pub fn fill(&self, r: Rect, rgb: Rgb) {
        if let Some((t, b)) = self.paint(rgb, 1.0) {
            unsafe { t.FillRectangle(&self.snap_rect(r), b) };
        }
    }

    pub fn round(&self, r: Rect, radius: f32, rgb: Rgb) {
        self.round_alpha(r, radius, rgb, 1.0);
    }

    pub fn round_alpha(&self, r: Rect, radius: f32, rgb: Rgb, alpha: f32) {
        if let Some((t, b)) = self.paint(rgb, alpha) {
            let rr = D2D1_ROUNDED_RECT { rect: self.snap_rect(r), radiusX: radius, radiusY: radius };
            unsafe { t.FillRoundedRectangle(&rr, b) };
        }
    }

    pub fn round_outline(&self, r: Rect, radius: f32, rgb: Rgb, width: f32) {
        self.round_outline_alpha(r, radius, rgb, width, 1.0);
    }

    pub fn round_outline_alpha(&self, r: Rect, radius: f32, rgb: Rgb, width: f32, alpha: f32) {
        if let Some((t, b)) = self.paint(rgb, alpha) {
            // Çizgi piksel ortasına otursun diye yarım kalınlık içeri alınır.
            let mut rect = self.snap_rect(r);
            let h = width / 2.0;
            rect.left += h;
            rect.top += h;
            rect.right -= h;
            rect.bottom -= h;
            let rr = D2D1_ROUNDED_RECT { rect, radiusX: radius, radiusY: radius };
            unsafe { t.DrawRoundedRectangle(&rr, b, width, None) };
        }
    }

    pub fn circle(&self, cx: f32, cy: f32, radius: f32, rgb: Rgb) {
        if let Some((t, b)) = self.paint(rgb, 1.0) {
            let e = D2D1_ELLIPSE { point: v2(cx, cy), radiusX: radius, radiusY: radius };
            unsafe { t.FillEllipse(&e, b) };
        }
    }

    /// Dönen yükleme halkası: `turn` 0..1 arasında bir tam tur.
    pub fn spinner(&self, cx: f32, cy: f32, radius: f32, turn: f32, rgb: Rgb, width: f32) {
        self.arc(cx, cy, radius, turn, 0.72, rgb, width);
    }

    /// Yay: `start` ve `sweep` tur cinsinden (0..1). İlerleme halkası için de kullanılır.
    #[allow(clippy::too_many_arguments)]
    pub fn arc(&self, cx: f32, cy: f32, radius: f32, start: f32, sweep: f32, rgb: Rgb, width: f32) {
        let sweep = sweep.clamp(0.0, 0.999);
        if sweep <= 0.001 {
            return;
        }
        let Some((t, b)) = self.paint(rgb, 1.0) else { return };
        let start = start * std::f32::consts::TAU;
        let sweep = sweep * std::f32::consts::TAU;
        let p = |a: f32| v2(cx + radius * a.cos(), cy + radius * a.sin());
        unsafe {
            let Ok(path) = self.factory.CreatePathGeometry() else { return };
            let Ok(sink) = path.Open() else { return };
            sink.BeginFigure(p(start), D2D1_FIGURE_BEGIN_HOLLOW);
            sink.AddArc(&D2D1_ARC_SEGMENT {
                point: p(start + sweep),
                size: D2D_SIZE_F { width: radius, height: radius },
                rotationAngle: 0.0,
                sweepDirection: D2D1_SWEEP_DIRECTION_CLOCKWISE,
                arcSize: if sweep > std::f32::consts::PI { D2D1_ARC_SIZE_LARGE } else { D2D1_ARC_SIZE_SMALL },
            });
            sink.EndFigure(D2D1_FIGURE_END_OPEN);
            if sink.Close().is_err() {
                return;
            }
            t.DrawGeometry(&path, b, width, self.round_caps.as_ref());
        }
    }

    /// Yuvarlak köşeli, yukarıdan aşağı (ya da çapraz) gradyan dolgu.
    pub fn round_gradient(&self, r: Rect, radius: f32, from: Rgb, to: Rgb, diagonal: bool) {
        self.round_gradient_alpha(r, radius, (from, 1.0), (to, 1.0), diagonal);
    }

    /// Saydamlıklı gradyan (cam parlaması gibi katmanlar için).
    pub fn round_gradient_alpha(&self, r: Rect, radius: f32, from: (Rgb, f32), to: (Rgb, f32), diagonal: bool) {
        let Some(t) = &self.target else { return };
        let a = self.opacity.get();
        unsafe {
            let stops = [
                D2D1_GRADIENT_STOP { position: 0.0, color: color(from.0, from.1 * a) },
                D2D1_GRADIENT_STOP { position: 1.0, color: color(to.0, to.1 * a) },
            ];
            let Ok(coll) = t.CreateGradientStopCollection(&stops, D2D1_GAMMA_2_2, D2D1_EXTEND_MODE_CLAMP) else { return };
            let props = D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES {
                startPoint: v2(r.x, r.y),
                endPoint: if diagonal { v2(r.right(), r.bottom()) } else { v2(r.x, r.bottom()) },
            };
            let Ok(brush) = t.CreateLinearGradientBrush(&props, None, &coll) else { return };
            let rr = D2D1_ROUNDED_RECT { rect: self.snap_rect(r), radiusX: radius, radiusY: radius };
            t.FillRoundedRectangle(&rr, &brush);
        }
    }

    /// Noktaları birleştiren yumuşak uçlu çizgi (küçük grafikler için).
    pub fn polyline(&self, points: &[(f32, f32)], rgb: Rgb, alpha: f32, width: f32) {
        if points.len() < 2 { return; }
        let Some((t, b)) = self.paint(rgb, alpha) else { return };
        unsafe {
            let Ok(path) = self.factory.CreatePathGeometry() else { return };
            let Ok(sink) = path.Open() else { return };
            sink.BeginFigure(v2(points[0].0, points[0].1), D2D1_FIGURE_BEGIN_HOLLOW);
            for p in &points[1..] { sink.AddLine(v2(p.0, p.1)); }
            sink.EndFigure(D2D1_FIGURE_END_OPEN);
            if sink.Close().is_err() { return; }
            t.DrawGeometry(&path, b, width, self.round_caps.as_ref());
        }
    }

    /// Ortadan dışa doğru sönen yumuşak ışık.
    pub fn glow(&self, cx: f32, cy: f32, rx: f32, ry: f32, rgb: Rgb, alpha: f32) {
        let Some(t) = &self.target else { return };
        let a = alpha * self.opacity.get();
        unsafe {
            let stops = [
                D2D1_GRADIENT_STOP { position: 0.0, color: color(rgb, a) },
                D2D1_GRADIENT_STOP { position: 1.0, color: color(rgb, 0.0) },
            ];
            let Ok(collection) = t.CreateGradientStopCollection(&stops, D2D1_GAMMA_2_2, D2D1_EXTEND_MODE_CLAMP) else {
                return;
            };
            let props = D2D1_RADIAL_GRADIENT_BRUSH_PROPERTIES {
                center: v2(cx, cy),
                gradientOriginOffset: v2(0.0, 0.0),
                radiusX: rx,
                radiusY: ry,
            };
            let Ok(brush) = t.CreateRadialGradientBrush(&props, None, &collection) else { return };
            let e = D2D1_ELLIPSE { point: v2(cx, cy), radiusX: rx, radiusY: ry };
            t.FillEllipse(&e, &brush);
        }
    }

    /// Resmi dikdörtgene sığdırır; `circle` ise yuvarlak kırpar (profil fotoğrafı).
    pub fn trim_images(&self) { self.bitmaps.borrow_mut().trim_to(32); }

    pub fn image(&self, img: &Image, r: Rect, circle: bool) {
        let Some(t) = &self.target else { return };
        let mut cache = self.bitmaps.borrow_mut();
        let bitmap = match cache.get(&img.id) {
            Some(b) => b.clone(),
            None => {
                let props = D2D1_BITMAP_PROPERTIES {
                    pixelFormat: D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED },
                    dpiX: 96.0,
                    dpiY: 96.0,
                };
                let size = D2D_SIZE_U { width: img.width, height: img.height };
                let Ok(b) = (unsafe { t.CreateBitmap(size, Some(img.pixels.as_ptr() as _), img.width * 4, &props) }) else { return };
                cache.insert(img.id, b.clone());
                b
            }
        };
        let dest = self.snap_rect(r);
        unsafe {
            if circle {
                let Ok(brush) = t.CreateBitmapBrush(&bitmap, None, None) else { return };
                let (sx, sy) = (r.w / img.width as f32, r.h / img.height as f32);
                brush.SetTransform(&Matrix3x2 { M11: sx, M12: 0.0, M21: 0.0, M22: sy, M31: r.x, M32: r.y });
                brush.SetOpacity(self.opacity.get());
                let e = D2D1_ELLIPSE { point: v2(r.cx(), r.cy()), radiusX: r.w / 2.0, radiusY: r.h / 2.0 };
                t.FillEllipse(&e, &brush);
            } else {
                t.DrawBitmap(&bitmap, Some(&dest), self.opacity.get(), D2D1_BITMAP_INTERPOLATION_MODE_LINEAR, None);
            }
        }
    }

    /// Metnin DIP cinsinden genişliği.
    pub fn measure(&self, s: &str, font: Font) -> f32 {
        let wide: Vec<u16> = s.encode_utf16().collect();
        unsafe {
            // Çizimle aynı ölçüm modu: GDI uyumlu yazılar aynı genişlikte ölçülür, kesilmez.
            let gdi = matches!(font, Font::Icon | Font::MidIcon | Font::SmallIcon | Font::Ui | Font::UiBold | Font::Small);
            let layout = if gdi {
                self.dwrite.CreateGdiCompatibleTextLayout(&wide, self.format(font), 10_000.0, 100.0, self.dpi / 96.0, None, true)
            } else {
                self.dwrite.CreateTextLayout(&wide, self.format(font), 10_000.0, 100.0)
            };
            let Ok(layout) = layout else { return 0.0 };
            let mut m = DWRITE_TEXT_METRICS::default();
            if layout.GetMetrics(&mut m).is_ok() { m.widthIncludingTrailingWhitespace } else { 0.0 }
        }
    }

    fn format(&self, font: Font) -> &IDWriteTextFormat {
        match font {
            Font::Ui => &self.fmt.ui,
            Font::UiBold => &self.fmt.ui_bold,
            Font::Small => &self.fmt.small,
            Font::Icon => &self.fmt.icon,
            Font::MidIcon => &self.fmt.mid_icon,
            Font::SmallIcon => &self.fmt.small_icon,
            Font::Big => &self.fmt.big,
            Font::Emoji => &self.fmt.emoji,
            Font::EmojiSmall => &self.fmt.emoji_small,
            Font::Giant => &self.fmt.giant,
        }
    }

    pub fn text(&self, s: &str, r: Rect, font: Font, rgb: Rgb, align: Align) {
        let Some((t, b)) = self.paint(rgb, 1.0) else { return };
        let fmt = self.format(font);
        let wide: Vec<u16> = s.encode_utf16().collect();
        unsafe {
            let _ = fmt.SetTextAlignment(match align {
                Align::Left => DWRITE_TEXT_ALIGNMENT_LEADING,
                Align::Center => DWRITE_TEXT_ALIGNMENT_CENTER,
                Align::Right => DWRITE_TEXT_ALIGNMENT_TRAILING,
            });
            // Simgeler GDI uyumlu ölçümle piksel ızgarasına oturur, daha keskin görünür.
            // Simgeler ve küçük arayüz yazıları (sekme adı, adres) GDI uyumlu ölçümle
            // piksel ızgarasına oturur; büyük başlıklar doğal ölçümle kalır.
            let mode = match font {
                Font::Icon | Font::MidIcon | Font::SmallIcon | Font::Ui | Font::UiBold | Font::Small => DWRITE_MEASURING_MODE_GDI_NATURAL,
                _ => DWRITE_MEASURING_MODE_NATURAL,
            };
            let options = if matches!(font, Font::Emoji | Font::EmojiSmall) { D2D1_DRAW_TEXT_OPTIONS_CLIP | D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT } else { D2D1_DRAW_TEXT_OPTIONS_CLIP };
            t.DrawText(&wide, fmt, &self.snap_rect(r), b, options, mode);
        }
    }
}

unsafe fn text_format(dw: &IDWriteFactory, family: PCWSTR, weight: DWRITE_FONT_WEIGHT, size: f32) -> Result<IDWriteTextFormat> {
    let f = dw.CreateTextFormat(family, None, weight, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_STRETCH_NORMAL, size, w!("tr-tr"))?;
    f.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
    f.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
    // Sığmayan başlıklar "…" ile kesilir.
    let sign = dw.CreateEllipsisTrimmingSign(&f)?;
    let trimming = DWRITE_TRIMMING { granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER, delimiter: 0, delimiterCount: 0 };
    f.SetTrimming(&trimming, &sign)?;
    Ok(f)
}

unsafe fn has_font(dw: &IDWriteFactory, family: PCWSTR) -> bool {
    let mut collection = None;
    if dw.GetSystemFontCollection(&mut collection, false).is_err() {
        return false;
    }
    let Some(collection) = collection else { return false };
    let (mut index, mut exists) = (0u32, BOOL::default());
    collection.FindFamilyName(family, &mut index, &mut exists).is_ok() && exists.as_bool()
}
