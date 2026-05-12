fn main() {
    #[cfg(target_os = "macos")]
    {
        cc::Build::new()
            .file("src/renderers/mac_colorsync_saturation_filter.c")
            .compile("mac_colorsync_saturation_filter");

        println!("cargo:rustc-link-lib=framework=ApplicationServices");
        println!("cargo:rustc-link-lib=framework=ColorSync");
    }

    tauri_build::build()
}
