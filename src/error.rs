extern crate libzfs_sys as sys;

use std::ffi::{CStr};
use std::fmt;
use std::mem::transmute;

pub type Result<T> = ::std::result::Result<T, Error>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZfsError {
    pub code: sys::zfs_error,
    pub msg: String,
}

impl ZfsError {
    //pub fn last_error(lib: &LibZfs) -> Self {
    pub fn last_error(handle: *mut sys::libzfs_handle_t) -> Self {
        let code: sys::zfs_error = unsafe { transmute(sys::libzfs_errno(handle)) };
        let msg_cstr = unsafe { CStr::from_ptr(sys::libzfs_error_description(handle)) };
        let msg = msg_cstr.to_string_lossy().into_owned();
        ZfsError { code, msg }
    }
}

impl ::std::error::Error for ZfsError {}

impl fmt::Display for ZfsError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "ZFS error {:?}: {}", self.code, self.msg)
    }
}

#[derive(Debug)]
pub enum Error {
    Sys(::std::io::Error),
    Zfs(ZfsError),
}

impl ::std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Sys(e) => Some(e),
            Error::Zfs(e) => Some(e),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Error::Sys(ref e) => e.fmt(f),
            Error::Zfs(ref e) => e.fmt(f),
        }
    }
}

impl From<ZfsError> for Error {
    fn from(z: ZfsError) -> Error {
        Error::Zfs(z)
    }
}
