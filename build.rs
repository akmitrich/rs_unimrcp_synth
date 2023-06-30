fn main() {
    println!("cargo:rerun-if-changed=includes/mrcp.h");

    println!("cargo:rustc-link-lib=apr-1");
    println!("cargo:rustc-link-lib=unimrcpserver");
    println!("cargo:rustc-link-search=/opt/unimrcp/lib");

    let mut builder = bindgen::Builder::default();
    builder = builder
        .header("includes/mrcp.h")
        .clang_arg("-I/opt/unimrcp/include")
        .clang_arg("-I/opt/unimrcp/include/apr-1");
    let bindings = builder
        .constified_enum_module("*")
        .prepend_enum_name(false)
        .blocklist_item("FALSE")
        .derive_eq(true)
        .generate()
        .expect("Unable to generate bindings.");
    let out_path = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Unable to write bindings.");
}
