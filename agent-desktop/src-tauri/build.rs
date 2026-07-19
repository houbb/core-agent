fn main() {
    let icon = std::path::PathBuf::from(
        std::env::var_os("CARGO_MANIFEST_DIR").expect("manifest directory"),
    )
    .join("icons/icon.ico");
    let windows = tauri_build::WindowsAttributes::new().window_icon_path(icon);
    tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(windows))
        .expect("run Tauri build script");
}
