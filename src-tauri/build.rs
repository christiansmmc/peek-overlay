fn main() {
    let manifest = if std::env::var("PROFILE").as_deref() == Ok("release") {
        "manifests/release.manifest"
    } else {
        "manifests/debug.manifest"
    };
    println!("cargo:rerun-if-changed=manifests");
    let manifest_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(manifest);
    let xml = std::fs::read_to_string(&manifest_path).expect("failed to read manifest");
    let attrs = tauri_build::WindowsAttributes::new().app_manifest(xml);
    tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(attrs))
        .expect("failed to run tauri-build");
}
