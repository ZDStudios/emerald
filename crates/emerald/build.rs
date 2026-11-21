fn main() {
    // Rebuild when UI assets change (they are embedded with include_dir!).
    println!("cargo:rerun-if-changed=../../ui");
    println!("cargo:rerun-if-changed=../../themes");
    #[cfg(target_os = "windows")]
    {
        let mut res = winres::WindowsResource::new();
        res.set_icon("../../packaging/windows/emerald.ico");
        res.set_manifest_file("../../packaging/windows/emerald.exe.manifest");
        res.set("ProductName", "Emerald")
            .set("FileDescription", "Emerald Browser")
            .set("CompanyName", "Emerald")
            .set("LegalCopyright", "MIT OR Apache-2.0");
        res.compile().expect("compiling Windows resources (icon, manifest)");
    }
}
