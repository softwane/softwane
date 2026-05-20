fn main() {
    println!("cargo:rerun-if-changed=icons");

    #[cfg(target_os = "macos")]
    {
        println!("cargo:rustc-link-lib=framework=ApplicationServices");
    }

    tauri_build::build()
}
