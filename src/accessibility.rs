//! UI Automation for the custom-drawn browser chrome. No web content is copied.
#![allow(non_upper_case_globals)]
use std::sync::{Arc, RwLock};
use windows::{
    core::*,
    Win32::{
        Foundation::*,
        System::{
            Com::SAFEARRAY,
            Ole::{SafeArrayCreateVector, SafeArrayPutElement},
            Variant::{VARIANT, VT_I4},
        },
        UI::{Accessibility::*, WindowsAndMessaging::*},
    },
};

pub const WM_INVOKE: u32 = WM_APP + 120;
pub const WM_FOCUS: u32 = WM_APP + 121;

#[derive(Clone)]
pub struct Node {
    pub key: u32,
    pub name: String,
    pub rect: RECT,
    pub tab: bool,
    pub selected: bool,
    pub enabled: bool,
}

#[derive(Default)]
struct Snapshot {
    name: String,
    nodes: Vec<Node>,
    focus: Option<u32>,
}

#[derive(Clone, Default)]
pub struct Tree(Arc<RwLock<Snapshot>>);
impl Tree {
    pub fn update(&self, name: &str, nodes: Vec<Node>, focus: Option<u32>) {
        if let Ok(mut s) = self.0.write() {
            *s = Snapshot {
                name: name.into(),
                nodes,
                focus,
            };
        }
    }
    pub fn root(&self, hwnd: HWND) -> IRawElementProviderSimple {
        Provider {
            hwnd: hwnd.0 as isize,
            tree: self.clone(),
            key: None,
        }
        .into()
    }
    pub fn focus_event(&self, hwnd: HWND, key: u32) {
        let provider: IRawElementProviderSimple = Provider {
            hwnd: hwnd.0 as isize,
            tree: self.clone(),
            key: Some(key),
        }
        .into();
        unsafe {
            let _ = UiaRaiseAutomationEvent(&provider, UIA_AutomationFocusChangedEventId);
        }
    }
}

#[implement(
    IRawElementProviderSimple,
    IRawElementProviderFragment,
    IRawElementProviderFragmentRoot,
    IInvokeProvider,
    ISelectionItemProvider,
    ISelectionProvider
)]
struct Provider {
    hwnd: isize,
    tree: Tree,
    key: Option<u32>,
}

fn absent<T>() -> Result<T> {
    Err(Error::from_hresult(S_FALSE))
}
impl Provider {
    fn node(&self) -> Result<Node> {
        self.tree
            .0
            .read()
            .ok()
            .and_then(|s| s.nodes.iter().find(|n| Some(n.key) == self.key).cloned())
            .ok_or_else(|| Error::from_hresult(HRESULT(UIA_E_ELEMENTNOTAVAILABLE as i32)))
    }
    fn fragment(&self, key: Option<u32>) -> IRawElementProviderFragment {
        Provider {
            hwnd: self.hwnd,
            tree: self.tree.clone(),
            key,
        }
        .into()
    }
}
impl IRawElementProviderSimple_Impl for Provider_Impl {
    fn ProviderOptions(&self) -> Result<ProviderOptions> {
        Ok(ProviderOptions_ServerSideProvider)
    }
    fn GetPatternProvider(&self, id: UIA_PATTERN_ID) -> Result<IUnknown> {
        if id == UIA_SelectionItemPatternId && self.node().is_ok_and(|n| n.tab) {
            return self
                .fragment(self.key)
                .cast::<ISelectionItemProvider>()?
                .cast();
        }
        if id == UIA_SelectionPatternId
            && self.key.is_none()
            && self
                .tree
                .0
                .read()
                .is_ok_and(|s| s.nodes.iter().any(|n| n.tab))
        {
            return self.fragment(None).cast::<ISelectionProvider>()?.cast();
        }
        if self.key.is_some() && id == UIA_InvokePatternId {
            self.fragment(self.key).cast::<IInvokeProvider>()?.cast()
        } else {
            absent()
        }
    }
    fn GetPropertyValue(&self, id: UIA_PROPERTY_ID) -> Result<VARIANT> {
        let node = if self.key.is_some() {
            Some(self.node()?)
        } else {
            None
        };
        Ok(match id {
            UIA_NamePropertyId => VARIANT::from(
                node.as_ref()
                    .map(|n| n.name.clone())
                    .unwrap_or_else(|| {
                        self.tree
                            .0
                            .read()
                            .map(|s| s.name.clone())
                            .unwrap_or_default()
                    })
                    .as_str(),
            ),
            UIA_AutomationIdPropertyId => {
                VARIANT::from(format!("sqywein-{}", self.key.unwrap_or(0)).as_str())
            }
            UIA_ControlTypePropertyId => VARIANT::from(if node.as_ref().is_some_and(|n| n.tab) {
                UIA_TabItemControlTypeId.0
            } else if node.is_some() {
                UIA_ButtonControlTypeId.0
            } else {
                UIA_PaneControlTypeId.0
            }),
            UIA_IsControlElementPropertyId | UIA_IsContentElementPropertyId => VARIANT::from(true),
            UIA_IsEnabledPropertyId => VARIANT::from(node.as_ref().is_none_or(|n| n.enabled)),
            UIA_IsKeyboardFocusablePropertyId => VARIANT::from(node.is_some()),
            UIA_IsSelectionItemPatternAvailablePropertyId => {
                VARIANT::from(node.as_ref().is_some_and(|n| n.tab))
            }
            UIA_HasKeyboardFocusPropertyId => VARIANT::from(
                self.key.is_some() && self.tree.0.read().is_ok_and(|s| s.focus == self.key),
            ),
            UIA_IsInvokePatternAvailablePropertyId => VARIANT::from(node.is_some()),
            UIA_ItemStatusPropertyId => {
                VARIANT::from(if node.as_ref().is_some_and(|n| n.selected) {
                    "Seçili"
                } else {
                    ""
                })
            }
            _ => VARIANT::default(),
        })
    }
    fn HostRawElementProvider(&self) -> Result<IRawElementProviderSimple> {
        if self.key.is_none() {
            unsafe { UiaHostProviderFromHwnd(HWND(self.hwnd as _)) }
        } else {
            absent()
        }
    }
}
impl IRawElementProviderFragment_Impl for Provider_Impl {
    fn Navigate(&self, d: NavigateDirection) -> Result<IRawElementProviderFragment> {
        let s = self
            .tree
            .0
            .read()
            .map_err(|_| Error::from_hresult(HRESULT(UIA_E_ELEMENTNOTAVAILABLE as i32)))?;
        let key = match (self.key, d) {
            (Some(_), NavigateDirection_Parent) => return Ok(self.fragment(None)),
            (None, NavigateDirection_FirstChild) => s.nodes.first().map(|n| n.key),
            (None, NavigateDirection_LastChild) => s.nodes.last().map(|n| n.key),
            (Some(key), NavigateDirection_NextSibling) => s
                .nodes
                .iter()
                .position(|n| n.key == key)
                .and_then(|i| s.nodes.get(i + 1))
                .map(|n| n.key),
            (Some(key), NavigateDirection_PreviousSibling) => s
                .nodes
                .iter()
                .position(|n| n.key == key)
                .and_then(|i| i.checked_sub(1))
                .and_then(|i| s.nodes.get(i))
                .map(|n| n.key),
            _ => None,
        };
        key.map(|k| self.fragment(Some(k))).map_or_else(absent, Ok)
    }
    fn GetRuntimeId(&self) -> Result<*mut SAFEARRAY> {
        let Some(key) = self.key else {
            return Ok(std::ptr::null_mut());
        };
        unsafe {
            let array = SafeArrayCreateVector(VT_I4, 0, 2);
            if array.is_null() {
                return Err(E_OUTOFMEMORY.into());
            }
            for (i, value) in [UiaAppendRuntimeId as i32, key as i32].iter().enumerate() {
                SafeArrayPutElement(array, &(i as i32), value as *const _ as _)?;
            }
            Ok(array)
        }
    }
    fn BoundingRectangle(&self) -> Result<UiaRect> {
        let r = if self.key.is_some() {
            self.node()?.rect
        } else {
            let snapshot = self
                .tree
                .0
                .read()
                .map_err(|_| Error::from_hresult(E_FAIL))?;
            let mut r = RECT::default();
            unsafe { GetWindowRect(HWND(self.hwnd as _), &mut r)? };
            if let Some(first) = snapshot.nodes.first() {
                r = first.rect;
                for n in &snapshot.nodes {
                    r.left = r.left.min(n.rect.left);
                    r.top = r.top.min(n.rect.top);
                    r.right = r.right.max(n.rect.right);
                    r.bottom = r.bottom.max(n.rect.bottom);
                }
            }
            r
        };
        Ok(UiaRect {
            left: r.left as f64,
            top: r.top as f64,
            width: (r.right - r.left) as f64,
            height: (r.bottom - r.top) as f64,
        })
    }
    fn GetEmbeddedFragmentRoots(&self) -> Result<*mut SAFEARRAY> {
        Ok(std::ptr::null_mut())
    }
    fn SetFocus(&self) -> Result<()> {
        unsafe {
            PostMessageW(
                Some(HWND(self.hwnd as _)),
                WM_FOCUS,
                WPARAM(self.key.unwrap_or(0) as usize),
                LPARAM(0),
            )
        }
    }
    fn FragmentRoot(&self) -> Result<IRawElementProviderFragmentRoot> {
        self.fragment(None).cast()
    }
}
impl IRawElementProviderFragmentRoot_Impl for Provider_Impl {
    fn ElementProviderFromPoint(&self, x: f64, y: f64) -> Result<IRawElementProviderFragment> {
        let key = self.tree.0.read().ok().and_then(|s| {
            s.nodes
                .iter()
                .find(|n| {
                    x >= n.rect.left as f64
                        && x < n.rect.right as f64
                        && y >= n.rect.top as f64
                        && y < n.rect.bottom as f64
                })
                .map(|n| n.key)
        });
        Ok(self.fragment(key))
    }
    fn GetFocus(&self) -> Result<IRawElementProviderFragment> {
        let key = self.tree.0.read().ok().and_then(|s| s.focus);
        Ok(self.fragment(key))
    }
}
impl IInvokeProvider_Impl for Provider_Impl {
    fn Invoke(&self) -> Result<()> {
        let node = self.node()?;
        if !node.enabled {
            return Err(HRESULT(UIA_E_ELEMENTNOTENABLED as i32).into());
        }
        unsafe {
            PostMessageW(
                Some(HWND(self.hwnd as _)),
                WM_INVOKE,
                WPARAM(node.key as usize),
                LPARAM(0),
            )
        }
    }
}

impl ISelectionItemProvider_Impl for Provider_Impl {
    fn Select(&self) -> Result<()> {
        if !self.node()?.tab {
            return Err(E_INVALIDARG.into());
        }
        IInvokeProvider_Impl::Invoke(self)
    }
    fn AddToSelection(&self) -> Result<()> {
        self.Select()
    }
    fn RemoveFromSelection(&self) -> Result<()> {
        if self.node()?.selected {
            Err(E_INVALIDARG.into())
        } else {
            Ok(())
        }
    }
    fn IsSelected(&self) -> Result<BOOL> {
        Ok(self.node()?.selected.into())
    }
    fn SelectionContainer(&self) -> Result<IRawElementProviderSimple> {
        self.fragment(None).cast()
    }
}
impl ISelectionProvider_Impl for Provider_Impl {
    fn GetSelection(&self) -> Result<*mut SAFEARRAY> {
        let keys: Vec<u32> = self
            .tree
            .0
            .read()
            .map_err(|_| Error::from_hresult(E_FAIL))?
            .nodes
            .iter()
            .filter(|n| n.tab && n.selected)
            .map(|n| n.key)
            .collect();
        unsafe {
            let array = SafeArrayCreateVector(
                windows::Win32::System::Variant::VT_UNKNOWN,
                0,
                keys.len() as u32,
            );
            if array.is_null() {
                return Err(E_OUTOFMEMORY.into());
            }
            for (i, key) in keys.into_iter().enumerate() {
                let element: IRawElementProviderSimple = self.fragment(Some(key)).cast()?;
                SafeArrayPutElement(array, &(i as i32), element.as_raw())?;
            }
            Ok(array)
        }
    }
    fn CanSelectMultiple(&self) -> Result<BOOL> {
        Ok(false.into())
    }
    fn IsSelectionRequired(&self) -> Result<BOOL> {
        Ok(self
            .tree
            .0
            .read()
            .is_ok_and(|s| s.nodes.iter().any(|n| n.tab))
            .into())
    }
}
