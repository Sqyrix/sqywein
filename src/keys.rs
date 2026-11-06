//! Klavye kısayolları. Hem pencere hem sayfa odaktayken aynı tablo kullanılır.

use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, VK_CONTROL, VK_MENU, VK_SHIFT};

#[derive(Clone, Copy, Default, PartialEq, Debug)]
pub struct Mods {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}

pub fn modifiers() -> Mods {
    let down = |vk: u16| unsafe { GetKeyState(vk as i32) } < 0;
    Mods { ctrl: down(VK_CONTROL.0), shift: down(VK_SHIFT.0), alt: down(VK_MENU.0) }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Cmd {
    NewTab,
    CloseTab,
    ReopenTab,
    FocusAddress,
    NextTab,
    PrevTab,
    GoToTab(usize),
    LastTab,
    Back,
    Forward,
    Reload,
    DevTools,
    Bookmark,
    History,
    Downloads,
    Bookmarks,
    PrivateTab,
    /// Adres çubuğunda: yazılanı aç.
    Go,
    /// Adres çubuğunda: değişikliği geri al, sayfaya dön.
    Cancel,
}

const VK_TAB: u32 = 0x09;
const VK_RETURN: u32 = 0x0D;
const VK_ESCAPE: u32 = 0x1B;
const VK_LEFT: u32 = 0x25;
const VK_RIGHT: u32 = 0x27;
const VK_F4: u32 = 0x73;
const VK_F5: u32 = 0x74;
const VK_F12: u32 = 0x7B;

/// `in_address`: tuş adres çubuğuna mı geldi?
pub fn command(vk: u32, m: Mods, in_address: bool) -> Option<Cmd> {
    let key = char::from_u32(vk).filter(|c| c.is_ascii_alphanumeric());
    Some(match (m.ctrl, m.shift, m.alt, key, vk) {
        (_, _, _, _, VK_RETURN) if in_address => Cmd::Go,
        (_, _, _, _, VK_ESCAPE) if in_address => Cmd::Cancel,
        (true, true, false, Some('T'), _) => Cmd::ReopenTab,
        (true, true, false, Some('N'), _) => Cmd::PrivateTab,
        (true, true, false, Some('O'), _) => Cmd::Bookmarks,
        (true, false, false, Some('D'), _) => Cmd::Bookmark,
        (true, false, false, Some('H'), _) => Cmd::History,
        (true, false, false, Some('J'), _) => Cmd::Downloads,
        (true, false, false, Some('T'), _) => Cmd::NewTab,
        (true, false, false, Some('W'), _) | (true, false, false, _, VK_F4) => Cmd::CloseTab,
        (true, false, false, Some('L' | 'K' | 'E'), _) | (false, false, true, Some('D'), _) => Cmd::FocusAddress,
        (true, false, false, _, VK_TAB) => Cmd::NextTab,
        (true, true, false, _, VK_TAB) => Cmd::PrevTab,
        (true, false, false, Some('9'), _) => Cmd::LastTab,
        (true, false, false, Some(c @ '1'..='8'), _) => Cmd::GoToTab(c as usize - '1' as usize),
        (false, false, true, _, VK_LEFT) if !in_address => Cmd::Back,
        (false, false, true, _, VK_RIGHT) if !in_address => Cmd::Forward,
        (false, false, false, _, VK_F5) | (true, false, false, Some('R'), _) => Cmd::Reload,
        (false, false, false, _, VK_F12) => Cmd::DevTools,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const CTRL: Mods = Mods { ctrl: true, shift: false, alt: false };
    const CTRL_SHIFT: Mods = Mods { ctrl: true, shift: true, alt: false };
    const NONE: Mods = Mods { ctrl: false, shift: false, alt: false };

    #[test]
    fn kisayollar() {
        assert_eq!(command('T' as u32, CTRL, false), Some(Cmd::NewTab));
        assert_eq!(command('T' as u32, CTRL_SHIFT, false), Some(Cmd::ReopenTab));
        assert_eq!(command('3' as u32, CTRL, false), Some(Cmd::GoToTab(2)));
        assert_eq!(command('9' as u32, CTRL, false), Some(Cmd::LastTab));
        assert_eq!(command(VK_RETURN, NONE, true), Some(Cmd::Go));
        assert_eq!(command(VK_RETURN, NONE, false), None);
        assert_eq!(command('A' as u32, NONE, false), None);
        assert_eq!(command('D' as u32, CTRL, false), Some(Cmd::Bookmark));
        assert_eq!(command('N' as u32, CTRL_SHIFT, false), Some(Cmd::PrivateTab));
    }
}
