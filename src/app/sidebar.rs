//! Native library sidebar; no extra web renderer.
use super::*;
impl App {
    pub(super) fn sidebar_width(&self)->f32 {if self.sidebar_open && !self.chrome_hidden() {if self.width as f32/self.scale()<760.0 {196.0}else{244.0}}else{0.0}}
    fn sidebar_rows(&self)->Vec<(Rect,Hit,String)> {
        let width=self.sidebar_width();if width==0.0{return vec![];}
        let content=self.layout().content;let mut rows=vec![];let mut y=content.y+54.0;
        for (hit,label) in [(Hit::SidebarBookmarks,"Yer işaretleri"),(Hit::SidebarReading,"Okuma Listesi"),(Hit::Overview,"Sekme görünümü")] {rows.push((Rect::new(12.0,y,width-24.0,38.0),hit,label.into()));y+=42.0;}
        y=content.y+262.0;let private=self.tabs[self.active].private;
        for (i,t) in self.tabs.iter().enumerate().filter(|(_,t)|t.private==private) {if y+38.0>content.bottom()-16.0{break;}rows.push((Rect::new(12.0,y,width-24.0,38.0),Hit::SidebarTab(i),t.label()));y+=42.0;}
        rows
    }
    pub(super) fn sidebar_hit(&self,x:f32,y:f32)->Option<Hit>{self.sidebar_rows().into_iter().find(|(r,_,_)|r.contains(x,y)).map(|(_,h,_)|h)}
    pub(super) fn sidebar_rect(&self,hit:Hit)->Option<Rect>{self.sidebar_rows().into_iter().find(|(_,h,_)|*h==hit).map(|(r,_,_)|r)}
    pub(super) fn paint_sidebar(&self,l:&Layout){
        let width=self.sidebar_width();if width==0.0{return;}let g=&self.gfx;
        g.fill(Rect::new(0.0,l.content.y,width,l.content.h),base());
        g.fill_alpha(Rect::new(width-1.0,l.content.y,1.0,l.content.h),line(),0.55);
        g.text("Kütüphane",Rect::new(24.0,l.content.y+14.0,width-48.0,28.0),Font::UiBold,ink(),Align::Left);
        let section=l.content.y+230.0;
        g.fill_alpha(Rect::new(24.0,section-26.0,width-48.0,1.0),line(),0.65);
        g.text(if self.tabs[self.active].private {"Gizli sekmeler"}else{"Sekmeler"},Rect::new(24.0,section,width-48.0,24.0),Font::UiBold,muted(),Align::Left);
        for (r,h,label) in self.sidebar_rows(){
            let selected=matches!(h,Hit::SidebarTab(i) if i==self.active);
            if selected{g.round_alpha(r,9.0,system_blue(),if is_dark(){0.20}else{0.10});g.round_outline_alpha(r,9.0,system_blue(),0.7,0.13);}else if self.hover==h{g.round(r,9.0,hover());}
            let icon=Rect::new(r.x+12.0,r.cy()-8.0,16.0,16.0);
            let glyph=match h{Hit::SidebarBookmarks=>ICON_STAR,Hit::SidebarReading=>"\u{E736}",Hit::Overview=>"\u{E8F1}",_=>ICON_GLOBE};
            if h==Hit::Overview {
                g.round_outline(Rect::new(icon.x+3.0,icon.y,13.0,12.0),2.0,system_blue(),1.1);
                let front=Rect::new(icon.x,icon.y+3.0,13.0,12.0);g.round(front,2.0,base());g.round_outline(front,2.0,system_blue(),1.1);
            }else if let Hit::SidebarTab(i)=h{if let Some(img)=self.tabs[i].icon.as_ref(){g.image(img,icon,false);}else{g.text(glyph,icon,Font::Icon,if selected{system_blue()}else{muted()},Align::Center);}}else{g.text(glyph,icon,Font::Icon,system_blue(),Align::Center);}
            let tx=icon.right()+10.0;
            g.text(&label,Rect::new(tx,r.y,r.right()-tx-12.0,r.h),if selected{Font::UiBold}else{Font::Ui},ink(),Align::Left);
        }
    }
}
