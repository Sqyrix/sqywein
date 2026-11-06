//! Sqywein: hafif ve kendine has bir tarayıcı.
#![windows_subsystem = "windows"]

mod adblock;
mod app;
mod engine;
mod gfx;
mod hello;
mod history;
mod image;
mod keys;
mod log;
mod memory;
mod menu;
mod net;
mod passwords;
mod state;
mod theme;
mod ui;
mod update;
mod url;

use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};

fn main() {
    unsafe {
        // WebView2 tek iş parçacıklı daire (STA) ister.
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }
    // Güncellemeden sonra açıldıysak önceki sürümün kapanmasını bekle.
    app::wait_for_previous();
    update::cleanup();
    if let Err(e) = app::run() {
        let text = windows::core::HSTRING::from(format!("Sqywein başlatılamadı:\n{e}"));
        unsafe {
            windows::Win32::UI::WindowsAndMessaging::MessageBoxW(
                None,
                &text,
                windows::core::w!("Sqywein"),
                windows::Win32::UI::WindowsAndMessaging::MB_ICONERROR,
            );
        }
    }
}
