//! Draw runtime context commands using the application's native menu.
use super::*;
use menu::{Category,Item};
fn find(entries:&[engine::ContextEntry],command:i32)->Option<&engine::ContextEntry>{for e in entries{if e.command==command{return Some(e);}
if let Some(found)=find(&e.children,command){return Some(found);}}None}
fn items(entries:&[engine::ContextEntry])->Vec<Item>{entries.iter().map(|e|{
    if e.separator{return Item::Sep;}
    let cmd=if e.children.is_empty(){MenuCmd::Context(e.command)}else{MenuCmd::Category(Category::Context(e.command))};
    Item::Action{label:if e.checked{format!("✓ {}",e.label)}else{e.label.clone()},hint:String::new(),cmd,enabled:e.enabled,side:None,icon:None}
}).collect()}
impl App{
    pub(super) fn web_context_items(&self,command:i32)->Vec<Item>{self.web_context.as_ref().and_then(|(_,_,r)|find(&r.entries,command)).map_or_else(Vec::new,|e|items(&e.children))}
    pub(super) fn open_web_context(&mut self,id:u32,request:Box<engine::ContextRequest>){
        if self.tabs.get(self.active).is_none_or(|t|t.id!=id){return;}
        self.close_menu();self.close_popup();self.close_suggest();
        let content=self.layout().content;let scale=self.scale();let mut origin=POINT::default();unsafe{let _=ClientToScreen(self.hwnd,&mut origin);}
        let point=POINT{x:origin.x+((content.x+request.location.x as f32)*scale).round()as i32+(320.0*scale).round()as i32,y:origin.y+((content.y+request.location.y as f32)*scale).round()as i32};
        if let Ok(menu)=Menu::open(self.hwnd,MenuKind::WebContext,point,self.dpi,items(&request.entries),None,self.gfx.hardware){let hwnd=menu.hwnd;self.web_context=Some((id,self.tabs[self.active].epoch,request));self.menu=Some(menu);defer(move||unsafe{let _=ShowWindow(hwnd,SW_SHOW);});}
    }
}
