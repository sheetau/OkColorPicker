fn main() {
    println!("cargo:rerun-if-changed=app.manifest");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc")
    {
        println!("cargo:rerun-if-changed=assets/windows.rc");
        println!("cargo:rerun-if-changed=assets/okcolorpicker.ico");
        let sdk = find_msvc_tools::find_windows_sdk(std::env::consts::ARCH)
            .expect("Windows SDK is required to compile the application icon");
        let compiler = sdk
            .path()
            .map(|dir| dir.join("rc.exe"))
            .find(|p| p.is_file())
            .expect("Windows SDK resource compiler");
        let res = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("icon.res");
        let status = std::process::Command::new(compiler)
            .args(["/nologo", "/fo"])
            .arg(&res)
            .arg("assets/windows.rc")
            .status()
            .expect("compile application icon");
        assert!(status.success(), "icon resource compilation failed");
        println!("cargo:rustc-link-arg={}", res.display());
        let manifest = std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap())
            .join("app.manifest");
        // Common Controls v6 exports the native subclassing APIs by name.
        // Apply the same activation context to the application and native tests.
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
    }
}
