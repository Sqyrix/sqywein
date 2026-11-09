//! Secondary native windows share the engine profile, not one another's tab list.
use crate::{state::{self,AppState,SavedTab},storage};
use serde::{Serialize,Deserialize};
use std::{io,path::PathBuf,sync::OnceLock};

pub fn secondary()->bool{std::env::args().any(|a|a=="--new-window"||a=="--private-window")}
pub fn private()->bool{std::env::args().any(|a|a=="--private-window")}
fn id()->&'static str{
    static ID:OnceLock<String>=OnceLock::new();ID.get_or_init(||format!("{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos()))
}
fn path()->PathBuf{state::root().join("windows").join(format!("{}.bin",id()))}
#[derive(Serialize,Deserialize)]
#[serde(rename_all="PascalCase")]
struct Window{tabs:Vec<SavedTab>,active_index:i32,window:Option<[i32;5]>,profile:String}
pub fn restore(state:&mut AppState){
    if !secondary(){return}
    state.tabs.clear();state.active_index=0;state.window=None;
    if let Some(profile)=std::env::var("SQYWEIN_WINDOW_PROFILE").ok().filter(|id|state.profiles.iter().any(|p|&p.id==id)){state.current_profile=profile;}
}
pub fn checkpoint(state:&AppState)->io::Result<()>{
    if !secondary()||private(){return Ok(())}
    storage::write_json(&path(),&Window{tabs:state.tabs.clone(),active_index:state.active_index,window:state.window,profile:state.current_profile.clone()})
}
pub fn close(){if secondary(){storage::remove(&path())}}
pub fn spawn(private:bool,profile:&str)->io::Result<()>{
    std::process::Command::new(std::env::current_exe()?).arg(if private{"--private-window"}else{"--new-window"}).env("SQYWEIN_WINDOW_PROFILE",profile).spawn()?;Ok(())
}
