//! Site information is bound to a tab/navigation, never to a URL supplied by HTML.
use super::*;
use serde_json::{json, Value};

impl App {
    pub(super) fn open_site_info(&mut self) {
        let Some(t) = self.tabs.get(self.active) else {
            return;
        };
        let Some(origin) = crate::passwords::origin(&t.url) else {
            return;
        };
        self.open_surface(overlay::Surface::Site {
            tab: t.id,
            epoch: t.epoch,
            origin,
        });
    }

    fn site_target(&self) -> Option<(u32, u64, String, ICoreWebView2)> {
        let overlay::Surface::Site { tab, epoch, origin } = self.surface.as_ref()? else {
            return None;
        };
        let t = self
            .tabs
            .get(self.active)
            .filter(|t| t.id == *tab && t.epoch == *epoch)?;
        let view = t.view.clone()?;
        if crate::passwords::origin(&engine::string(|p| unsafe { view.Source(p) })).as_deref()
            != Some(origin.as_str())
        {
            return None;
        }
        Some((*tab, *epoch, origin.clone(), view))
    }

    pub(super) fn refresh_site_info(&self) {
        let Some((id, epoch, origin, view)) = self.site_target() else {
            return;
        };
        let Some(t) = self.index_of(id).map(|i| &self.tabs[i]) else {
            return;
        };
        let Some(panel) = self.surface_view() else {
            return;
        };
        let value = json!({"type":"site.info","origin":origin,"security":t.security,"private":t.private,"http":t.url.starts_with("http://"),"runtime":self.env.as_ref().map(|env|engine::string(|p|unsafe{env.BrowserVersionString(p)})).unwrap_or_default()});
        engine::post_json(&panel, &value.to_string());
        engine::site_permissions(&view, id, epoch, origin);
    }

    pub(super) fn site_permissions_reply(&self, id: u32, epoch: u64, origin: &str, items: Value) {
        if self
            .site_target()
            .is_some_and(|(i, e, o, _)| i == id && e == epoch && o == origin)
        {
            if let Some(panel) = self.surface_view() {
                engine::post_json(
                    &panel,
                    &json!({"type":"site.permissions","items":items}).to_string(),
                );
            }
        }
    }

    pub(super) fn site_message(&mut self, kind: &str, msg: &Value) {
        let Some((id, epoch, origin, view)) = self.site_target() else {
            return;
        };
        match kind {
            "overlay.site.permission" => {
                if self.tabs[self.active].private {
                    return;
                }
                let (Some(k), Some(s)) = (msg["kind"].as_i64(), msg["state"].as_i64()) else {
                    return;
                };
                if ![1, 2, 3, 4, 6].contains(&k) || !(0..=2).contains(&s) {
                    return;
                }
                engine::set_site_permission(&view, id, epoch, origin, k as i32, s as i32);
            }
            "overlay.site.clear" => {
                let profile = self.state.current_profile.clone();
                if self.clearing_data.contains(&profile) {
                    return;
                }
                // Retire every earlier snapshot before asking the runtime to clear.
                if !self.tabs[self.active].private {
                    self.clear_saved_cookies();
                }
                self.clearing_data.insert(profile.clone());
                engine::clear_site(&view, profile, id, epoch, origin);
            }
            "overlay.site.refresh" => self.refresh_site_info(),
            _ => {}
        }
    }

    pub(super) fn site_cleared(
        &mut self,
        profile: &str,
        id: u32,
        epoch: u64,
        origin: &str,
        ok: bool,
    ) {
        self.clearing_data.remove(profile);
        self.finish_cookie_clear(profile);
        if self.state.current_profile == profile {
            self.snapshot_cookies();
        }
        if self
            .site_target()
            .is_some_and(|(i, e, o, _)| i == id && e == epoch && o == origin)
        {
            if let Some(panel) = self.surface_view() {
                engine::post_json(&panel, &json!({"type":"site.cleared","ok":ok}).to_string());
            }
        }
    }
}
