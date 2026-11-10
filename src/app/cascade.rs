//! Native cascading menus share one focus family and close as a unit.
use super::*;
use menu::Category;
pub(super) const TIMER:usize=11;
impl App{
    pub(super) fn close_submenus(&mut self,from:usize){
        self.pending_submenu=None;unsafe{let _=KillTimer(Some(self.hwnd),TIMER);}
        while self.submenus.len()>from{let mut m=self.submenus.pop().unwrap();if m.start_exit(){self.retiring_menus.push(m)}else{let hwnd=m.hwnd;defer(move||unsafe{let _=DestroyWindow(hwnd);});}}
    }
    pub(super) fn open_submenu(&mut self,category:Category,keyboard:bool){
        let Some(source)=self.menu_source.or_else(||self.menu.as_ref().map(|m|m.hwnd)) else{return};
        let (parent,depth)=if self.menu.as_ref().is_some_and(|m|m.hwnd==source){(self.menu.as_ref().unwrap(),0)}else if let Some(i)=self.submenus.iter().position(|m|m.hwnd==source){(&self.submenus[i],i+1)}else{return};
        if self.submenus.get(depth).is_some_and(|m|m.kind==MenuKind::Category(category)){return}
        let Some(row)=parent.category_rect(category) else{return};
        let mut bounds=RECT::default();unsafe{let _=GetWindowRect(parent.hwnd,&mut bounds);}
        let width=(320.0*self.scale()).round()as i32;
        let mut info=MONITORINFO{cbSize:std::mem::size_of::<MONITORINFO>()as u32,..Default::default()};unsafe{let _=GetMonitorInfoW(MonitorFromWindow(self.hwnd,MONITOR_DEFAULTTONEAREST),&mut info);}
        let edge=if bounds.right+width<=info.rcWork.right{bounds.right+width-3}else{bounds.left+3};
        self.close_submenus(depth);let items=self.category_items(category);
        if let Ok(menu)=Menu::open(self.hwnd,MenuKind::Category(category),POINT{x:edge,y:row.top-6},self.dpi,items,None,self.gfx.hardware){let hwnd=menu.hwnd;self.submenus.push(menu);if unsafe{IsWindowVisible(self.hwnd)}.as_bool(){defer(move||unsafe{let _=ShowWindow(hwnd,if keyboard{SW_SHOW}else{SW_SHOWNOACTIVATE});});}}
    }
    pub(super) fn hover_submenu(&mut self,source:HWND,category:Option<Category>){
        let depth=if self.menu.as_ref().is_some_and(|m|m.hwnd==source){0}else{self.submenus.iter().position(|m|m.hwnd==source).map_or(self.submenus.len(),|i|i+1)};
        if let Some(category)=category{
            if self.submenus.get(depth).is_some_and(|m|m.kind==MenuKind::Category(category)){return}
            if self.pending_submenu==Some((source,category)){return}
            self.pending_submenu=Some((source,category));unsafe{SetTimer(Some(self.hwnd),TIMER,180,None);}
        }else{self.close_submenus(depth);}
    }
    pub(super) fn back_submenu(&mut self,hwnd:HWND){
        if let Some(i)=self.submenus.iter().position(|m|m.hwnd==hwnd){let parent=if i==0{self.menu.as_ref().map(|m|m.hwnd)}else{Some(self.submenus[i-1].hwnd)};self.close_submenus(i);if let Some(parent)=parent{unsafe{let _=SetFocus(Some(parent));}}}
    }
}
