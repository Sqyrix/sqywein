//! Shared generation/barrier prevents another window resurrecting cleared cookies.
use crate::{state,storage};
use serde::{Serialize,Deserialize};
use std::{io,path::PathBuf};
#[derive(Default,Serialize,Deserialize)]
pub struct Control{pub epoch:u64,clearing_until:u64}
fn path(profile:&str)->PathBuf{state::root().join(format!("cookie-control-{profile}.bin"))}
fn now()->u64{std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs()}
fn read(profile:&str)->io::Result<Control>{match storage::read_json(&path(profile)){Ok(c)=>Ok(c),Err(e)if e.kind()==io::ErrorKind::NotFound=>Ok(Control::default()),Err(e)=>Err(e)}}
pub fn generation(profile:&str)->Option<u64>{read(profile).ok().filter(|c|c.clearing_until<=now()).map(|c|c.epoch)}
pub fn begin(profile:&str)->io::Result<u64>{
    let _guard=storage::RecordLock::acquire(&path(profile))?;let mut c=read(profile)?;
    c.epoch=c.epoch.saturating_add(1);c.clearing_until=now()+120;storage::write_json(&path(profile),&c)?;
    storage::remove(&state::root().join(format!("session-{profile}.bin")));Ok(c.epoch)
}
pub fn finish(profile:&str,epoch:u64){
    if let Ok(_guard)=storage::RecordLock::acquire(&path(profile)){if let Ok(mut c)=read(profile){if c.epoch==epoch{c.clearing_until=0;let _=storage::write_json(&path(profile),&c);}}}
}
pub fn lock_current(profile:&str,epoch:u64)->Option<storage::RecordLock>{let guard=storage::RecordLock::acquire(&path(profile)).ok()?;if generation(profile)==Some(epoch){Some(guard)}else{None}}
