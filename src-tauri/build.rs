fn main() {
    // Tauri 2.0 handles all Windows PE resources (icon, version, metadata)
    // from tauri.conf.json — winresource is not needed and causes
    // "duplicate resource type:VERSION" linker errors (CVT1100 / LNK1123).
    tauri_build::build();
}
