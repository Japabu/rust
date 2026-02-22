pub use super::common::Env;
use crate::collections::HashMap;
use crate::ffi::{OsStr, OsString};
use crate::io;
use crate::sync::Mutex;

static ENV: Mutex<Option<HashMap<OsString, OsString>>> = Mutex::new(None);

pub fn init() {
    *ENV.lock().unwrap() = Some(HashMap::new());
}

pub fn env() -> Env {
    let guard = ENV.lock().unwrap();
    let map = guard.as_ref().unwrap();
    let result = map.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    Env::new(result)
}

pub fn getenv(k: &OsStr) -> Option<OsString> {
    ENV.lock().unwrap().as_ref().unwrap().get(k).cloned()
}

pub unsafe fn setenv(k: &OsStr, v: &OsStr) -> io::Result<()> {
    ENV.lock().unwrap().as_mut().unwrap().insert(k.to_owned(), v.to_owned());
    Ok(())
}

pub unsafe fn unsetenv(k: &OsStr) -> io::Result<()> {
    ENV.lock().unwrap().as_mut().unwrap().remove(k);
    Ok(())
}
