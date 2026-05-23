fn main() {
    generate_bmi270_config();
}

fn generate_bmi270_config() {
    use std::fs;
    use std::path::PathBuf;

    let manifest_dir =
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("missing manifest dir"));
    let workspace_dir = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root");
    let source_path = workspace_dir
        .join("contrib")
        .join("bmi270")
        .join("bmi270_upstream.c");

    println!("cargo:rerun-if-changed={}", source_path.display());

    let source = fs::read_to_string(&source_path)
        .expect("missing contrib/bmi270/bmi270_upstream.c; this file is required to build the BMI270 configuration image");

    let marker = "const uint8_t bmi270_config_file[] = {";
    let start = source
        .find(marker)
        .expect("failed to find bmi270_config_file in contrib/bmi270/bmi270_upstream.c")
        + marker.len();
    let end = source[start..]
        .find("};")
        .map(|offset| start + offset)
        .expect("failed to find the end of bmi270_config_file in contrib/bmi270/bmi270_upstream.c");

    let bytes = source[start..end]
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join(", ");

    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR is not set"));
    let output = out_dir.join("bmi270_config.rs");

    fs::write(
        output,
        format!("pub const BMI270_CONFIG: &[u8] = &[{bytes}];\n"),
    )
    .expect("failed to write bmi270_config.rs");
}
