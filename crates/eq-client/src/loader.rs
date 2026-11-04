//! Finding eqclient.dll for a plugin: it's next to the plugin's own DLL.

use std::ffi::c_void;
use std::fmt;
use std::path::{Path, PathBuf};

use crate::abi::{API_VERSION, Api, DLL_NAME, EXPORT};
use crate::win::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadError {
    /// It isn't there, or Windows wouldn't load it.
    NotFound(PathBuf),
    /// It's there but isn't eqclient.dll: no [`EXPORT`].
    NoApi(PathBuf),
    /// The loaded one is older than this eq-client needs. Another plugin
    /// loaded it first, and it stays until the game restarts.
    TooOld { found: u32, needed: u32 },
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadError::NotFound(path) => write!(f, "{} isn't there or wouldn't load", path.display()),
            LoadError::NoApi(path) => write!(f, "{} isn't eqclient.dll (it doesn't export {EXPORT:?})", path.display()),
            LoadError::TooOld { found, needed } => write!(f, "the loaded {DLL_NAME} speaks version {found}, older than {needed}: restart the game to load the new one"),
        }
    }
}

impl std::error::Error for LoadError {}

/// Loads eqclient.dll from the folder of the module containing `address`
/// (pass any function or static of the calling plugin).
pub fn load_beside(address: *const c_void) -> Result<&'static Api, LoadError> {
    load(&path_beside(address))
}

/// Where [`load_beside`] looks: eqclient.dll in the folder of the module
/// containing `address`.
pub fn path_beside(address: *const c_void) -> PathBuf {
    let mut module = std::ptr::null_mut();
    let found = unsafe { GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT, address.cast(), &mut module) };
    let mut name = [0u16; 1024];
    let len = if found != 0 { (unsafe { GetModuleFileNameW(module, name.as_mut_ptr(), name.len() as u32) }) as usize } else { 0 };
    let own = PathBuf::from(String::from_utf16_lossy(&name[..len]));
    own.parent().unwrap_or(Path::new("")).join(DLL_NAME)
}

/// Loads the DLL at `path` and checks its version.
pub fn load(path: &Path) -> Result<&'static Api, LoadError> {
    let module = unsafe { LoadLibraryW(wide(path.as_os_str()).as_ptr()) };
    if module.is_null() {
        return Err(LoadError::NotFound(path.to_owned()));
    }
    let export = unsafe { GetProcAddress(module, EXPORT.as_ptr()) };
    if export.is_null() {
        return Err(LoadError::NoApi(path.to_owned()));
    }
    let get: extern "C" fn() -> *const Api = unsafe { std::mem::transmute(export) };
    let api = unsafe { get().as_ref() }.ok_or_else(|| LoadError::NoApi(path.to_owned()))?;
    if api.version < API_VERSION {
        return Err(LoadError::TooOld { found: api.version, needed: API_VERSION });
    }
    Ok(api)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_dll_and_one_without_the_api_are_refused() {
        let missing = Path::new(r"C:\nowhere\eqclient.dll");
        assert_eq!(load(missing).err(), Some(LoadError::NotFound(missing.to_owned())));
        let kernel32 = Path::new("kernel32.dll");
        assert_eq!(load(kernel32).err(), Some(LoadError::NoApi(kernel32.to_owned())));
    }

    #[test]
    fn it_looks_beside_the_calling_module() {
        let test_exe = std::env::current_exe().unwrap();
        assert_eq!(path_beside(path_beside as *const c_void), test_exe.parent().unwrap().join(DLL_NAME));
    }
}
