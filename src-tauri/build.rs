fn main() {
    if let Ok(root) = std::env::var("GSTREAMER_1_0_ROOT_MSVC_X86_64") {
        let libpath = format!("{}\\lib", root.trim_end_matches('\\'));
        println!("cargo:rustc-link-search=native={}", libpath);
        println!("cargo:rerun-if-env-changed=GSTREAMER_1_0_ROOT_MSVC_X86_64");
    }

    tauri_build::build()
}
