use std::path::PathBuf;

fn main() {
    compile_fusion();
}

fn compile_fusion() {
    let manifest_dir =
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("missing manifest dir"));
    let workspace_dir = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root");
    let fusion_dir = workspace_dir.join("contrib").join("fusion");

    println!("cargo:rerun-if-changed={}", fusion_dir.display());

    cc::Build::new()
        .include(&fusion_dir)
        .file(fusion_dir.join("FusionAhrs.c"))
        .file(fusion_dir.join("FusionCompass.c"))
        .file(fusion_dir.join("FusionOffset.c"))
        .compile("fusion");
}
