//! Sqywein: hafif ve kendine has bir tarayıcı.
#![windows_subsystem = "windows"]

mod adblock;
mod app;
mod cache;
mod engine;
mod ocr;
mod widgets;
mod gfx;
mod hello;
mod history;
mod image;
mod integration;
mod keys;
mod log;
mod memory;
mod menu;
mod net;
mod passwords;
mod setup;
mod state;
mod theme;
mod ui;
mod update;
mod url;

use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};

fn main() {
    // SqyweinSetup.exe / --install / --uninstall: tarayıcı açılmadan kurulum yapılır.
    if let Some(mode) = setup::mode() {
        std::process::exit(setup::run(mode));
    }
    integration::identify_process();
    if std::env::args().skip(1).any(|arg| arg == "--register-browser") {
        if let Err(error) = integration::register_browser() { eprintln!("{error}"); std::process::exit(1); }
        return;
    }
    if log::enabled() {
        std::panic::set_hook(Box::new(|info| log::line(&format!("PANIC {info}"))));
    }
    unsafe {
        // WebView2 tek iş parçacıklı daire (STA) ister.
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }
    // Güncellemeden sonra açıldıysak önceki sürümün kapanmasını bekle.
    app::wait_for_previous();
    let _instance = match integration::acquire_or_forward() { Ok(Some(i)) => i, Ok(None) => return, Err(e) => { unsafe { windows::Win32::UI::WindowsAndMessaging::MessageBoxW(None, &windows::core::HSTRING::from(format!("Açık Sqywein penceresine ulaşılamadı: {e}")), windows::core::w!("Sqywein"), windows::Win32::UI::WindowsAndMessaging::MB_ICONERROR); } return } };
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
