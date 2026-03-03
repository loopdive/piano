fn main() {
    // The WebGPU compatibility probe in src/lib.rs (check_webgpu_support)
    // works around wgpu 0.19 sending the deprecated `maxInterStageShaderComponents`
    // limit.  Newer wgpu versions use `maxInterStageShaderVariables` instead,
    // so the probe should be simplified or removed after upgrading.
    let cargo_toml = std::fs::read_to_string("Cargo.toml").expect("Failed to read Cargo.toml");
    let has_wgpu_019 = cargo_toml.lines().any(|line| line.contains("wgpu") && line.contains("\"0.19"));
    if !has_wgpu_019 {
        println!(
            "cargo:warning=wgpu is no longer 0.19 — review the WebGPU probe \
             (check_webgpu_support in src/lib.rs). The maxInterStageShaderComponents \
             workaround is likely unnecessary now."
        );
    }
}
