fn main() {
    // Simge ve manifest (DPI, segment heap, comctl32 v6) exe'ye gömülür.
    embed_resource::compile("sqywein.rc", embed_resource::NONE)
        .manifest_required()
        .unwrap();
    println!("cargo:rerun-if-changed=sqywein.rc");
    println!("cargo:rerun-if-changed=sqywein.manifest");
}
