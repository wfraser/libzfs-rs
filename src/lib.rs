//! Idiomatic Rust bindings for libzfs.
//! Copyright 2018 by William R. Fraser <wfraser@codewise.org>

use libzfs_sys as sys;

use std::collections::VecDeque;
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
        self.ptr_or_err(handle).map(|handle| ZPool { libzfs: self.handle, handle })
    }

    pub fn dataset_by_name(&self, name: &SafeString, types: DatasetTypeMask) -> Result<Dataset> {
        let handle = unsafe { sys::zfs_open(self.handle, name.as_ptr(), types.0 as i32) };
        self.ptr_or_err(handle).map(|handle| Dataset { libzfs: self.handle, handle })
    }

    pub fn create_snapshots<I, T>(&self, names: I) -> Result<()>
        where I: Iterator<Item = T>,
              T: AsRef<str>,
    {
        let nvl = self.build_nvlist(names)?;

        // Need to check if empty, otherwise it segfaults.
        let ret = match unsafe { sys::nvlist_empty(nvl) } {
            0 => if 0 != unsafe { sys::zfs_snapshot_nvl(self.handle, nvl, std::ptr::null_mut()) } {
                self.get_last_error()
            } else {
                Ok(())
            },
            _ => Ok(()),
        };

        unsafe { sys::nvlist_free(nvl) };

        ret
    }

    pub fn destroy_snapshots<I, T>(&self, names: I) -> Result<()>
        where I: Iterator<Item = T>,
              T: AsRef<str>,
    {
        let nvl = self.build_nvlist(names)?;

        // Need to check if empty, otherwise it segfaults.
        let ret = match unsafe { sys::nvlist_empty(nvl) } {
            0 => match unsafe { sys::zfs_destroy_snaps_nvl(self.handle, nvl, 0) } {
                0 => Ok(()),
                _ => self.get_last_error(),
            },
            _ => Ok(()),
        };

        unsafe { sys::nvlist_free(nvl) };

        ret
    }

    fn build_nvlist<I, T>(&self, names: I) -> Result<*mut sys::nvlist_t>
        where I: Iterator<Item = T>,
              T: AsRef<str>,
    {
        let mut nvl = std::ptr::null_mut();

        if 0 != unsafe { sys::nvlist_alloc(&mut nvl as *mut _, sys::NV_UNIQUE_NAME, 0) } {
            return self.get_last_error();
        }

        for name in names {
            let mut cstr = name.as_ref().to_owned();
            cstr.push('\0');
            unsafe { sys::fnvlist_add_boolean(nvl, cstr.as_ptr() as *const _) };
        }

        Ok(nvl)
    }

    pub fn get_zpools(&self) -> Result<Vec<ZPool>> {
        //let mut pools = vec![];
        struct Context {
            libzfs: *mut sys::libzfs_handle_t,
            pools: Vec<ZPool>,
        }

        extern "C" fn zpool_iter_collect(handle: *mut sys::zpool_handle_t, context: *mut c_void) -> i32 {
            let ctx = unsafe { &mut *(context as *mut Context) };
            ctx.pools.push(ZPool { libzfs: ctx.libzfs, handle });
            0
        }

        let mut ctx = Context {
            libzfs: self.handle,
            pools: vec![],
        };
        let result = unsafe {
            sys::zpool_iter(
                self.handle,
                Some(zpool_iter_collect),
                &mut ctx as *mut _ as *mut c_void,
            )
        };

        if result == 0 {
            Ok(ctx.pools)
        } else {
            Err(ZfsError::last_error(self.handle).into())
        }
    }

    fn ptr_or_err<T>(&self, ptr: *mut T) -> Result<*mut T> {
        if ptr.is_null() {
            self.get_last_error()
        } else {
            Ok(ptr)
        }
    }

    fn get_last_error<T>(&self) -> Result<T> {
        let zfs_err = ZfsError::last_error(self.handle);
        // TODO: is this valid? should we do this on EZFS_SUCCESS instead / in addition?
        if zfs_err.code != sys::zfs_error::EZFS_UNKNOWN {
            Err(Error::Zfs(zfs_err))
        } else {
            Err(Error::Sys(std::io::Error::last_os_error()))
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
    libzfs: *mut sys::libzfs_handle_t,
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

    pub fn get_datasets(&self) -> Result<Vec<Dataset>> {
        let pool_name = self.get_name();

        let root_handle = unsafe {
            sys::zfs_open(self.libzfs, pool_name.as_ptr(), sys::zfs_type_t::ZFS_TYPE_FILESYSTEM as i32)
        };
        if root_handle.is_null() {
            return Err(ZfsError::last_error(self.libzfs).into());
        }

        let mut ctx = ZfsIterCollectContext {
            libzfs: self.libzfs,
            vec: vec![Dataset { libzfs: self.libzfs, handle: root_handle }],
        };

        let result = unsafe {
            sys::zfs_iter_dependents(
                root_handle,
                1, // allow recursion
                Some(zfs_iter_collect),
                &mut ctx as *mut _ as *mut c_void,
            )
        };

        ctx.vec.retain(|ds| {
            let typ = ds.get_type();
            if typ == DatasetType::Bookmark || typ == DatasetType::Snapshot {
                false
            } else {
                true
            }
        });

        if result == 0 {
            Ok(ctx.vec)
        } else {
            Err(ZfsError::last_error(self.libzfs).into())
        }
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
    libzfs: *mut sys::libzfs_handle_t,
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
        ZPool { libzfs: self.libzfs, handle }
    }

    /// Get the name of the pool this dataset belongs to.
    pub fn get_pool_name(&self) -> SafeString {
        let cstr = unsafe { CStr::from_ptr(sys::zfs_get_pool_name(self.handle)) };
        let utf8_verified = cstr.to_str().expect("invalid UTF8 in pool name");
        SafeString::from(utf8_verified.to_owned())
    }

    /// Get all snapshots of this dataset.
    pub fn get_snapshots(&self) -> Result<Vec<Dataset>> {
        let mut ctx = ZfsIterCollectContext {
            libzfs: self.libzfs,
            vec: vec![],
        };
        let result = unsafe {
            sys::zfs_iter_snapshots(
                self.handle,
                0, // "simple"
                Some(zfs_iter_collect),
                &mut ctx as *mut _ as *mut c_void,
                0, // min_txg: none
                0, // max_txg: none
            )
        };
        if result == 0 {
            Ok(ctx.vec)
        } else {
            Err(ZfsError::last_error(self.libzfs).into())
        }
    }

    /// Get all snapshots of this dataset, ordered by creation time (oldest first).
    pub fn get_snapshots_ordered(&self) -> Result<Vec<Dataset>> {
        let mut ctx = ZfsIterCollectContext {
            libzfs: self.libzfs,
            vec: vec![],
        };
        let result = unsafe {
            sys::zfs_iter_snapshots_sorted(
                self.handle,
                Some(zfs_iter_collect),
                &mut ctx as *mut _ as *mut c_void,
                0, // min_txg: none
                0, // max_txg: none
            )
        };
        if result == 0 {
            Ok(ctx.vec)
        } else {
            Err(ZfsError::last_error(self.libzfs).into())
        }
    }

    /// Execute a callback function for each snapshot of this dataset.
    pub fn foreach_snapshot(&self, callback: Box<dyn FnMut(Dataset)>) -> Result<()> {
        let mut ctx = ZfsIterCallbackContext {
            libzfs: self.libzfs,
            callback,
        };
        let result = unsafe {
            sys::zfs_iter_snapshots(
                self.handle,
                0,
                Some(zfs_iter_callback),
                &mut ctx as *mut _ as *mut c_void,
                0,
                0,
            )
        };
        if result == 0 {
            Ok(())
        } else {
            Err(ZfsError::last_error(self.libzfs).into())
        }
    }

    /// Execute a callback function for each snapshot of this dataset, ordered by creation time
    /// (oldest first).
    pub fn foreach_snapshot_ordered(&self, callback: Box<dyn FnMut(Dataset)>) -> Result<()> {
        let mut ctx = ZfsIterCallbackContext {
            libzfs: self.libzfs,
            callback,
        };
        let result = unsafe {
            sys::zfs_iter_snapshots_sorted(
                self.handle,
                Some(zfs_iter_callback),
                &mut ctx as *mut _ as *mut c_void,
                0,
                0,
            )
        };
        if result == 0 {
            Ok(())
        } else {
            Err(ZfsError::last_error(self.libzfs).into())
        }
    }

    /// Get all direct descendent filesystems under this one.
    pub fn get_child_filesystems(&self) -> Result<Vec<Dataset>> {
        let mut ctx = ZfsIterCollectContext {
            libzfs: self.libzfs,
            vec: vec![],
        };
        let result = unsafe {
            sys::zfs_iter_filesystems(
                self.handle,
                Some(zfs_iter_collect),
                &mut ctx as *mut _ as *mut c_void,
            )
        };
        if result == 0 {
            Ok(ctx.vec)
        } else {
            Err(ZfsError::last_error(self.libzfs).into())
        }
    }

    /// Get all child datasets of this one, recursively, of all types (snapshot, filesystem, etc.).
    pub fn get_all_dependents(&self) -> Result<Vec<Dataset>> {
        let mut ctx = ZfsIterCollectContext {
            libzfs: self.libzfs,
            vec: vec![],
        };
        let result = unsafe {
            sys::zfs_iter_dependents(
                self.handle,
                1, // allow recursion
                Some(zfs_iter_collect),
                &mut ctx as *mut _ as *mut c_void,
            )
        };
        if result == 0 {
            Ok(ctx.vec)
        } else {
            Err(ZfsError::last_error(self.libzfs).into())
        }
    }
}

struct ZfsIterCollectContext {
    libzfs: *mut sys::libzfs_handle_t,
    vec: Vec<Dataset>,
}

extern "C" fn zfs_iter_collect(handle: *mut sys::zfs_handle_t, context: *mut c_void) -> i32 {
    let ctx = unsafe { &mut *(context as *mut ZfsIterCollectContext) };
    ctx.vec.push(Dataset { libzfs: ctx.libzfs, handle });
    0
}

struct ZfsIterCallbackContext {
    libzfs: *mut sys::libzfs_handle_t,
    callback: Box<dyn FnMut(Dataset)>,
}

extern "C" fn zfs_iter_callback(handle: *mut sys::zfs_handle_t, context: *mut c_void) -> i32 {
    let ctx = unsafe { &mut *(context as *mut ZfsIterCallbackContext) };
    (ctx.callback)(Dataset { libzfs: ctx.libzfs, handle });
    0
}

impl Clone for Dataset {
    fn clone(&self) -> Self {
        let handle = unsafe { sys::zfs_handle_dup(self.handle) };
        Dataset { libzfs: self.libzfs, handle }
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

        impl From<$new_name> for $repr {
            fn from(val: $new_name) -> $repr {
                unsafe { std::mem::transmute(val) }
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
