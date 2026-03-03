fn main() {
    // Verify wgpu version matches what the codebase expects.
    let cargo_toml = std::fs::read_to_string("Cargo.toml").expect("Failed to read Cargo.toml");
    let has_wgpu_28 = cargo_toml.lines().any(|line| line.contains("wgpu") && line.contains("\"28"));
    if !has_wgpu_28 {
        println!(
            "cargo:warning=wgpu version changed — review API usage in src/lib.rs \
             and renderer modules for compatibility."
        );
    }
}
