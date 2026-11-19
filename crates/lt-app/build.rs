fn main() {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os != "windows" {
        return;
    }

    let icon = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/icon.ico");
    let icon = icon.canonicalize().unwrap_or(icon);
    println!("cargo:rerun-if-changed={}", icon.display());

    let mut res = winresource::WindowsResource::new();
    res.set_icon(icon.to_str().expect("chemin icon.ico UTF-8"));
    // GPUI charge l'icône via LoadImageW(module, MAKEINTRESOURCE(1), IMAGE_ICON, …).
    res.set("ProductName", "Load Testing");
    res.set("FileDescription", "Load Testing");
    if let Err(err) = res.compile() {
        panic!("échec d'embedding de l'icône Windows : {err}");
    }
}
