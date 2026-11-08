fn main() {
    // Simge ve manifest (DPI, segment heap, comctl32 v6) exe'ye gömülür.
    let version = std::env::var("CARGO_PKG_VERSION").unwrap();
    let numbers = ["MAJOR","MINOR","PATCH"].map(|part|std::env::var(format!("CARGO_PKG_VERSION_{part}")).unwrap()).join(",");
    embed_resource::compile("sqywein.rc", &[format!("SQYWEIN_FILE_VERSION={numbers},0"),format!("SQYWEIN_VERSION=\"{version}\"")])
        .manifest_required()
        .unwrap();
    println!("cargo:rerun-if-changed=sqywein.rc");
    println!("cargo:rerun-if-changed=sqywein.manifest");
}
