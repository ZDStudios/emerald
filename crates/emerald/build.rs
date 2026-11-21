fn main() {
    // Rebuild when UI assets change (they are embedded with include_dir!).
    println!("cargo:rerun-if-changed=../../ui");
    println!("cargo:rerun-if-changed=../../themes");
    #[cfg(target_os = "windows")]
    {
        let mut res = winres::WindowsResource::new();
        if std::path::Path::new("../../packaging/windows/emerald.ico").exists() {
            res.set_icon("../../packaging/windows/emerald.ico");
        }
        res.set("ProductName", "Emerald").set("FileDescription", "Emerald Browser");
        let _ = res.compile();
    }
}
