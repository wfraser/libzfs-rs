//! Idiomatic Rust bindings for libzfs.
//! Copyright 2018 by William R. Fraser <wfraser@codewise.org>

extern crate libzfs_sys as sys;

use std::ffi::CStr;
use std::os::raw::c_void;

mod string;
mod error;

pub use string::SafeString;
pub use error::*;

#[derive(Debug)]
pub struct LibZfs {
    handle: *mut sys::libzfs_handle_t,
}

impl LibZfs {
    pub fn new() -> Result<Self> {
        let handle = unsafe { sys::libzfs_init() };
        if handle.is_null() {
            Err(Error::Sys(std::io::Error::last_os_error()))
        } else {
            Ok(LibZfs { handle })
        }
    }

    pub fn pool_by_name(&self, name: &SafeString) -> Result<ZPool> {
        let handle = unsafe { sys::zpool_open(self.handle, name.as_ptr()) };
        self.ptr_or_err(handle).map(|handle| ZPool { handle })
    }

    pub fn dataset_by_name(&self, name: &SafeString, types: DatasetTypeMask) -> Result<Dataset> {
        let handle = unsafe { sys::zfs_open(self.handle, name.as_ptr(), types.0 as i32) };
        self.ptr_or_err(handle).map(|handle| Dataset { handle })
    }

    fn ptr_or_err<T>(&self, ptr: *mut T) -> Result<*mut T> {
        if ptr.is_null() {
            let zfs_err = ZfsError::last_error(self);
            // TODO: is this valid? should we do this on EZFS_SUCCESS instead / in addition?
            if zfs_err.code != sys::zfs_error::EZFS_UNKNOWN {
                Err(Error::Zfs(zfs_err))
            } else {
                Err(Error::Sys(std::io::Error::last_os_error()))
            }
        } else {
            Ok(ptr)
        }
    }
}

impl Drop for LibZfs {
    fn drop(&mut self) {
        unsafe {
            sys::libzfs_fini(self.handle);
        }
    }
}

#[derive(Debug)]
pub struct ZPool {
    handle: *mut sys::zpool_handle_t,
}

impl ZPool {
    pub fn get_state(&self) -> ZPoolState {
        // this is defined as returning an int, though it really returns a pool_state_t.
        let raw: i32 = unsafe { sys::zpool_get_state(self.handle) };
        ZPoolState::from(raw as sys::pool_state_t)
    }

    pub fn get_name(&self) -> SafeString {
        let cstr = unsafe { CStr::from_ptr(sys::zpool_get_name(self.handle)) };
        let utf8_verified = cstr.to_str().expect("invalid UTF8 in pool name");
        SafeString::from(utf8_verified.to_owned())
    }
}

impl Drop for ZPool {
    fn drop(&mut self) {
        unsafe {
            sys::zpool_close(self.handle);
        }
    }
}

#[derive(Debug)]
pub struct Dataset {
    handle: *mut sys::zfs_handle_t,
}

impl Dataset {
    /// Get the type of this dataset.
    pub fn get_type(&self) -> DatasetType {
        DatasetType::from(unsafe { sys::zfs_get_type(self.handle) })
    }

    /// Get the name of this dataset.
    pub fn get_name(&self) -> SafeString {
        let cstr = unsafe { CStr::from_ptr(sys::zfs_get_name(self.handle)) };
        let utf8_verified = cstr.to_str().expect("invalid UTF8 in dataset name");
        SafeString::from(utf8_verified.to_owned())
    }

    /// Get the pool this dataset belongs to.
    pub fn get_pool(&self) -> ZPool {
        let handle = unsafe { sys::zfs_get_pool_handle(self.handle) };
        ZPool { handle }
    }

    /// Get the name of the pool this dataset belongs to.
    pub fn get_pool_name(&self) -> SafeString {
        let cstr = unsafe { CStr::from_ptr(sys::zfs_get_pool_name(self.handle)) };
        let utf8_verified = cstr.to_str().expect("invalid UTF8 in pool name");
        SafeString::from(utf8_verified.to_owned())
    }

    /// Get all snapshots of this dataset.
    pub fn get_snapshots(&self) -> Vec<Dataset> {
        let mut snapshots = Vec::<Dataset>::new();
        let vec_p = &mut snapshots as *mut _ as *mut c_void;
        unsafe {
            sys::zfs_iter_snapshots(
                self.handle,
                0, // "simple"
                Some(zfs_iter_collect),
                vec_p,
                0, // min_txg: none
                0, // max_txg: none
                );
        }
        snapshots
    }

    /// Get all snapshots of this dataset, ordered by creation time (oldest first).
    pub fn get_snapshots_ordered(&self) -> Vec<Dataset> {
        let mut snapshots = Vec::<Dataset>::new();
        let vec_p = &mut snapshots as *mut _ as *mut c_void;
        unsafe {
            sys::zfs_iter_snapshots_sorted(
                self.handle,
                Some(zfs_iter_collect),
                vec_p,
                0, // min_txg: none
                0, // max_txg: none
                );
        }
        snapshots
    }

    /// Execute a callback function for each snapshot of this dataset.
    pub fn foreach_snapshot(&self, callback: Box<dyn FnMut(Dataset)>) {
        let mut context = ZfsIterContext { callback };
        unsafe {
            sys::zfs_iter_snapshots(
                self.handle,
                0,
                Some(zfs_iter_handler),
                &mut context as *mut _ as *mut c_void,
                0,
                0,
                );
        }
    }

    /// Execute a callback function for each snapshot of this dataset, ordered by creation time
    /// (oldest first).
    pub fn foreach_snapshot_ordered(&self, callback: Box<dyn FnMut(Dataset)>) {
        let mut context = ZfsIterContext { callback };
        unsafe {
            sys::zfs_iter_snapshots_sorted(
                self.handle,
                Some(zfs_iter_handler),
                &mut context as *mut _ as *mut c_void,
                0,
                0,
                );
        }
    }

    /// Get all filesystems under (not including) this one.
    pub fn get_child_filesystems(&self) -> Vec<Dataset> {
        let mut filesystems = Vec::<Dataset>::new();
        let vec_p = &mut filesystems as *mut _ as *mut c_void;
        unsafe {
            sys::zfs_iter_filesystems(
                self.handle,
                Some(zfs_iter_collect),
                vec_p);
        }
        filesystems
    }

    /// Get all child datasets of this one, of all types (snapshot, filesystem, etc.).
    pub fn get_all_children(&self) -> Vec<Dataset> {
        let mut datasets = Vec::<Dataset>::new();
        let vec_p = &mut datasets as *mut _ as *mut c_void;
        unsafe {
            sys::zfs_iter_children(
                self.handle,
                Some(zfs_iter_collect),
                vec_p);
        }
        datasets
    }
}

extern "C" fn zfs_iter_collect(handle: *mut sys::zfs_handle_t, context: *mut c_void) -> i32 {
    let collected = unsafe { &mut *(context as *mut Vec<Dataset>) };
    collected.push(Dataset { handle });
    0
}

struct ZfsIterContext {
    callback: Box<dyn FnMut(Dataset)>,
}

extern "C" fn zfs_iter_handler(handle: *mut sys::zfs_handle_t, context: *mut c_void) -> i32 {
    let ctx = unsafe { &mut *(context as *mut ZfsIterContext) };
    (ctx.callback)(Dataset { handle });
    0
}

impl Clone for Dataset {
    fn clone(&self) -> Self {
        let handle = unsafe { sys::zfs_handle_dup(self.handle) };
        Dataset { handle }
    }
}

impl Drop for Dataset {
    fn drop(&mut self) {
        unsafe {
            sys::zfs_close(self.handle);
        }
    }
}

// this is meant to be used with the bindgen option 'constified_enum_module'
macro_rules! translate_enum {
    (
        new_name: $new_name:ident,
        sys_name: $sys_name:path,
        repr: $repr:ident,
        variants: {
            $(
                $sys:ident => $new:ident,
            )*
        }
    ) => {
        // This is needed to access variants of $sys_name. As far as I can tell, it's impossible
        // to join a path and an identifier with '::' in a macro. :(
        use $sys_name::*;

        #[derive(Debug, Copy, Clone, PartialEq, Eq)]
        #[repr($repr)]
        pub enum $new_name {
            $($new = ($sys as $repr),)*
        }

        impl From<$repr> for $new_name {
            fn from(raw: $repr) -> $new_name {
                use $new_name::*;
                match raw {
                    $(
                        $sys => $new
                    ),*,
                    _ => panic!("unknown {} variant: {}", stringify!($sys_name), raw)
                }
            }
        }

        impl Into<$repr> for $new_name {
            fn into(self) -> $repr {
                unsafe { std::mem::transmute(self) }
            }
        }
    }
}

translate_enum! {
    new_name: ZPoolState,
    sys_name: sys::pool_state,
    repr: u32,
    variants: {
        POOL_STATE_ACTIVE => Active,
        POOL_STATE_EXPORTED => Exported,
        POOL_STATE_DESTROYED => Destroyed,
        POOL_STATE_SPARE => Spare,
        POOL_STATE_L2CACHE => L2Cache,
        POOL_STATE_UNINITIALIZED => Uninitialized,
        POOL_STATE_UNAVAIL => Unavailable,
        POOL_STATE_POTENTIALLY_ACTIVE => PotentiallyActive,
    }
}

translate_enum! {
    new_name: DatasetType,
    sys_name: sys::zfs_type_t,
    repr: u32,
    variants: {
        ZFS_TYPE_FILESYSTEM => Filesystem,
        ZFS_TYPE_SNAPSHOT => Snapshot,
        ZFS_TYPE_VOLUME => Volume,
        ZFS_TYPE_POOL => Pool,
        ZFS_TYPE_BOOKMARK => Bookmark,
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct DatasetTypeMask(u32);

impl DatasetTypeMask {
    pub fn all() -> Self {
        DatasetTypeMask(std::u32::MAX)
    }
}

impl From<DatasetType> for DatasetTypeMask {
    fn from(t: DatasetType) -> DatasetTypeMask {
        DatasetTypeMask(t.into())
    }
}

impl std::ops::BitOr for DatasetType {
    type Output = DatasetTypeMask;
    fn bitor(self, rhs: DatasetType) -> Self::Output {
        DatasetTypeMask(Into::<u32>::into(self) | Into::<u32>::into(rhs))
    }
}

impl std::ops::BitOr<DatasetType> for DatasetTypeMask {
    type Output = DatasetTypeMask;
    fn bitor(self, rhs: DatasetType) -> Self::Output {
        DatasetTypeMask(self.0 | Into::<u32>::into(rhs))
    }
}
