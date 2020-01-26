// silence style lints and things that don't actually matter
#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(clippy::redundant_static_lifetimes)]
#![allow(clippy::trivially_copy_pass_by_ref)]
#![allow(clippy::unreadable_literal)]
#![allow(clippy::useless_transmute)]

// this should actually be fixed, but it's not clear how: some functions have u128 return types
#![allow(improper_ctypes)]

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
