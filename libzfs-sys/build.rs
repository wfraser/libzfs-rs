extern crate bindgen;
extern crate pkg_config;

use std::env;
use std::path::PathBuf;

fn main() {
    let pkg = pkg_config::Config::new().probe("libzfs").expect("pkg-config for libzfs failed");

    let bindings = bindgen::Builder::default()
        .header("wrapper.h")
        .clang_args(pkg.include_paths.iter().map(|path|
            format!("-I{}", path.to_str().expect("non-Unicode include path"))))
        .constified_enum_module("pool_state")
        .constified_enum_module("zfs_type_t")
        .constified_enum_module("lzc_send_flags")
        //.constified_enum_module(".*_t")
        .rustified_enum("zfs_error")
        .opaque_type("libzfs_handle")
        .opaque_type("zfs_handle")
        .opaque_type("zpool_handle")
        .generate()
        .expect("failed to generate libzfs bindings");

    bindings.write_to_file(PathBuf::from(env::var("OUT_DIR").unwrap()).join("bindings.rs"))
        .expect("failed to write libzfs bindings");
}
