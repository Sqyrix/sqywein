use super::*;
use crate::accessibility::{self, Node};
use windows::Win32::UI::Accessibility::{UiaReturnRawElementProvider, UiaRootObjectId};

impl App {
    fn accessible_items(&self) -> Vec<(Hit, u32, String)> {
        if self.chrome_hidden() {
            return vec![];
        }
        let mut out = vec![
            (Hit::Back, 1, "Geri".into()),
            (Hit::Forward, 2, "İleri".into()),
            (Hit::Home, 3, "Ana sayfa".into()),
            (Hit::Address, 4, "Adres çubuğu".into()),
            (Hit::SiteInfo, 5, "Site bilgileri ve izinleri".into()),
            (Hit::Reload, 6, "Yenile veya durdur".into()),
            (Hit::Star, 7, "Yer işaretini değiştir".into()),
            (Hit::NewTab, 8, "Yeni sekme".into()),
            (Hit::Menu, 9, "Tarayıcı menüsü".into()),
            (Hit::Profile, 10, "Profil".into()),
            (Hit::Share, 11, "Sayfayı paylaş".into()),
            (Hit::Downloads, 12, "İndirmeler".into()),
            (Hit::Translate, 13, "Çeviri".into()),
            (Hit::Shield, 14, "Gizlilik ve reklam engelleme".into()),
            (Hit::Min, 15, "Küçült".into()),
            (Hit::Max, 16, "Büyüt veya geri al".into()),
            (Hit::Close, 17, "Pencereyi kapat".into()),
        ];
        for (i, t) in self.tabs.iter().enumerate() {
            out.push((
                Hit::Tab(i),
                100_000 + t.id * 2,
                format!("{}{}", t.label(), if t.private { " — gizli" } else { "" }),
            ));
            out.push((
                Hit::TabClose(i),
                100_001 + t.id * 2,
                format!("{} sekmesini kapat", t.label()),
            ));
        }
        out.retain(|(hit, _, _)| self.accessible_rect(*hit).is_some());
        out
    }
    fn accessible_rect(&self, hit: Hit) -> Option<Rect> {
        let l = self.layout();
        Some(match hit {
            Hit::Address => l.address,
            Hit::Home => l.home,
            Hit::Share => l.share,
            Hit::SiteInfo => {
                crate::passwords::origin(&self.tabs.get(self.active)?.url)?;
                Rect::new(l.address.x + 6.0, l.address.y, 28.0, l.address.h)
            }
            Hit::Translate => l.translate?,
            Hit::TabClose(i) => {
                let r = l.tabs.get(i)?;
                Rect::new(r.right() - 28.0, r.y + 4.0, 24.0, (r.h - 8.0).max(1.0))
            }
            _ => self.hit_rect(hit)?,
        })
    }
    pub(super) fn refresh_accessibility(&self) {
        if !self.a11y_active.get() {
            return;
        }
        let mut origin = POINT::default();
        unsafe {
            let _ = ClientToScreen(self.hwnd, &mut origin);
        }
        let nodes = self
            .accessible_items()
            .into_iter()
            .filter_map(|(hit, key, name)| {
                let mut rect = self.px(self.accessible_rect(hit)?);
                rect.left += origin.x;
                rect.right += origin.x;
                rect.top += origin.y;
                rect.bottom += origin.y;
                let enabled = match hit {
                    Hit::Back => self.tabs[self.active].can_back,
                    Hit::Forward => self.tabs[self.active].can_forward,
                    _ => true,
                };
                Some(Node {
                    key,
                    name,
                    rect,
                    tab: matches!(hit, Hit::Tab(_)),
                    selected: hit == Hit::Tab(self.active),
                    enabled,
                })
            })
            .collect();
        self.accessibility.update(
            "Sqywein — araç çubuğu ve sekmeler",
            nodes,
            self.chrome_focus,
        );
    }
    pub(super) fn accessibility_message(
        &mut self,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> Option<LRESULT> {
        if msg == WM_GETOBJECT && lparam.0 as i32 == UiaRootObjectId {
            self.a11y_active.set(true);
            self.refresh_accessibility();
            let provider = self.accessibility.root(self.accessibility_host);
            return Some(unsafe {
                UiaReturnRawElementProvider(self.accessibility_host, wparam, lparam, &provider)
            });
        }
        if msg == accessibility::WM_INVOKE || msg == accessibility::WM_FOCUS {
            if let Some((hit, key, _)) = self
                .accessible_items()
                .into_iter()
                .find(|(_, key, _)| *key == wparam.0 as u32)
            {
                if msg == accessibility::WM_INVOKE {
                    if let Hit::Tab(i) = hit {
                        self.activate(i)
                    } else {
                        self.click(hit)
                    }
                } else {
                    self.focus_chrome_key(key);
                }
            }
            return Some(LRESULT(0));
        }
        if msg == WM_KEYDOWN
            && unsafe { GetFocus() } == self.accessibility_host
            && self.chrome_focus.is_some()
        {
            match wparam.0 as u32 {
                0x09 | 0x25..=0x28 => {
                    let items = self.accessible_items();
                    if items.is_empty() {
                        return Some(LRESULT(0));
                    }
                    let at = items
                        .iter()
                        .position(|(_, k, _)| Some(*k) == self.chrome_focus)
                        .unwrap_or(0);
                    let back = keys::modifiers().shift || matches!(wparam.0, 0x25 | 0x26);
                    let next = if back {
                        (at + items.len() - 1) % items.len()
                    } else {
                        (at + 1) % items.len()
                    };
                    self.focus_chrome_key(items[next].1);
                    return Some(LRESULT(0));
                }
                0x0d | 0x20 => {
                    if let Some((hit, _, _)) = self
                        .accessible_items()
                        .into_iter()
                        .find(|(_, key, _)| Some(*key) == self.chrome_focus)
                    {
                        if let Hit::Tab(i) = hit {
                            self.activate(i)
                        } else {
                            self.click(hit)
                        }
                    }
                    return Some(LRESULT(0));
                }
                0x1b => {
                    self.chrome_focus = None;
                    self.focus_content();
                    self.invalidate();
                    return Some(LRESULT(0));
                }
                _ => {}
            }
        }
        None
    }
    fn focus_chrome_key(&mut self, key: u32) {
        self.a11y_active.set(true);
        self.chrome_focus = Some(key);
        unsafe {
            let _ = SetFocus(Some(self.accessibility_host));
        }
        // WM_SETFOCUS ordinarily forwards into the page; reclaim for chrome traversal.
        self.refresh_accessibility();
        self.accessibility.focus_event(self.accessibility_host, key);
        self.invalidate();
    }
    pub(super) fn cycle_chrome(&mut self, back: bool) {
        let current = if unsafe { GetFocus() } == self.accessibility_host {
            self.chrome_focus
        } else {
            None
        };
        let in_tabs = current.is_some_and(|k| k >= 100_000);
        if (in_tabs && !back) || (current.is_some() && !in_tabs && back) {
            self.chrome_focus = None;
            self.focus_content();
            self.invalidate();
        } else if (current.is_none() && back) || (current.is_some() && !in_tabs) {
            if let Some(t) = self.tabs.get(self.active) {
                self.focus_chrome_key(100_000 + t.id * 2);
            }
        } else {
            self.focus_chrome_key(1);
        }
    }
    pub(super) fn paint_chrome_focus(&self) {
        if unsafe { GetFocus() } != self.accessibility_host {
            return;
        }
        if let Some(r) = self
            .accessible_items()
            .into_iter()
            .find(|(_, key, _)| Some(*key) == self.chrome_focus)
            .and_then(|(hit, _, _)| self.accessible_rect(hit))
        {
            self.gfx.round_outline(
                Rect::new(
                    r.x + 2.0,
                    r.y + 2.0,
                    (r.w - 4.0).max(1.0),
                    (r.h - 4.0).max(1.0),
                ),
                5.0,
                system_blue(),
                2.0,
            );
        }
    }
}
