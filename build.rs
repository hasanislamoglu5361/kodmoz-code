fn main() {
    println!("cargo:rerun-if-changed=macos/open_files.m");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        cc::Build::new()
            .file("macos/open_files.m")
            .flag("-fobjc-arc")
            .flag("-fblocks")
            .compile("kodmoz_macos");
        println!("cargo:rustc-link-lib=framework=AppKit");
        println!("cargo:rustc-link-lib=framework=CoreServices");
    }
}
