extern crate libzfs_sys as sys;

use std::ffi::{CStr};
use std::fmt;
use std::mem::transmute;

use super::LibZfs;

pub type Result<T> = ::std::result::Result<T, Error>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZfsError {
    pub code: sys::zfs_error,
    pub msg: String,
}

impl ZfsError {
    pub fn last_error(lib: &LibZfs) -> Self {
        let code: sys::zfs_error = unsafe { transmute(sys::libzfs_errno(lib.handle)) };
        let msg_cstr = unsafe { CStr::from_ptr(sys::libzfs_error_description(lib.handle)) };
        let msg = msg_cstr.to_string_lossy().into_owned();
        ZfsError { code, msg }
    }
}

impl ::std::error::Error for ZfsError {
    fn description(&self) -> &str {
        "ZFS error"
    }
}

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
    fn description(&self) -> &str {
        match *self {
            Error::Sys(ref e) => e.description(),
            Error::Zfs(ref e) => e.description(),
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
