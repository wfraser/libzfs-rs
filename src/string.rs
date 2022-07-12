use std::ffi::{CStr, CString};
use std::fmt;

/// A FFI-friendly string: null-terminated, no internal nulls, well-formed UTF-8. Lets us skip
/// checks and reallocations when passing around between functions.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SafeString {
    // TODO(wfraser): explore changing this to Vec<u8> (including null terminator) and just
    // implementing AsRef<CStr> and AsRef<str> and selected String functions on top of that.
    inner: CString,
}

impl SafeString {
    pub fn as_ptr(&self) -> *const ::std::os::raw::c_char {
        self.inner.as_ptr()
    }
}

impl From<SafeString> for String {
    fn from(s: SafeString) -> String {
        let bytes = s.inner.into_bytes();
        // Safety: we already checked its UTF-8'ness
        unsafe { String::from_utf8_unchecked(bytes) }
    }
}

impl From<String> for SafeString {
    fn from(s: String) -> SafeString {
        SafeString {
            inner: CString::new(s).unwrap()
        }
    }
}

impl<'a> From<&'a str> for SafeString {
    fn from(s: &'a str) -> SafeString {
        SafeString::from(s.to_owned())
    }
}

impl AsRef<str> for SafeString {
    fn as_ref(&self) -> &str {
        let bytes = self.inner.to_bytes();
        // Safety: we already checked its UTF-8'ness
        unsafe { std::str::from_utf8_unchecked(bytes) }
    }
}

impl AsRef<CStr> for SafeString {
    fn as_ref(&self) -> &CStr {
        &self.inner
    }
}

impl fmt::Debug for SafeString {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        fmt::Debug::fmt(&self.inner, f)
    }
}

impl fmt::Display for SafeString {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        fmt::Display::fmt(AsRef::<str>::as_ref(self), f)
    }
}
