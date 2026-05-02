use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn bridge_binary_name() -> &'static str {
    #[cfg(windows)]
    {
        "yorling-bridge.exe"
    }

    #[cfg(not(windows))]
    {
        "yorling-bridge"
    }
}

fn bridge_sidecar_filename(target: &str) -> String {
    #[cfg(windows)]
    {
        format!("yorling-bridge-{target}.exe")
    }

    #[cfg(not(windows))]
    {
        format!("yorling-bridge-{target}")
    }
}

fn build_bridge_sidecar(manifest_dir: &Path, target: &str, profile: &str) {
    let workspace_root = manifest_dir
        .parent()
        .expect("src-tauri should live under the workspace root");
    let bridge_manifest = workspace_root.join("crates/yorling-island-bridge/Cargo.toml");
    let bridge_src_dir = workspace_root.join("crates/yorling-island-bridge/src");
    let bridge_target_dir = workspace_root.join("target/yorling-bridge-sidecar");

    println!("cargo:rerun-if-changed={}", bridge_manifest.display());
    println!("cargo:rerun-if-changed={}", bridge_src_dir.display());
    println!("cargo:rerun-if-env-changed=TARGET");
    println!("cargo:rerun-if-env-changed=PROFILE");

    let cargo_bin = env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let mut command = Command::new(cargo_bin);
    command
        .arg("build")
        .arg("--manifest-path")
        .arg(&bridge_manifest)
        .arg("--bin")
        .arg("yorling-bridge")
        .arg("--target")
        .arg(target)
        .env("CARGO_TARGET_DIR", &bridge_target_dir);

    if profile == "release" {
        command.arg("--release");
    }

    let status = command
        .status()
        .expect("failed to launch cargo build for yorling-bridge");
    if !status.success() {
        panic!("failed to build yorling-bridge sidecar");
    }

    let source_binary = bridge_target_dir
        .join(target)
        .join(profile)
        .join(bridge_binary_name());
    let sidecar_dir = manifest_dir.join("binaries");
    let sidecar_binary = sidecar_dir.join(bridge_sidecar_filename(target));

    fs::create_dir_all(&sidecar_dir).expect("failed to create src-tauri/binaries");
    fs::copy(&source_binary, &sidecar_binary).unwrap_or_else(|error| {
        panic!(
            "failed to copy yorling-bridge sidecar from {} to {}: {error}",
            source_binary.display(),
            sidecar_binary.display(),
        )
    });

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mut permissions = fs::metadata(&sidecar_binary)
            .expect("failed to read sidecar metadata")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&sidecar_binary, permissions)
            .expect("failed to mark yorling-bridge sidecar executable");
    }
}

fn main() {
    #[cfg(target_os = "macos")]
    println!("cargo:rustc-link-search=framework=/System/Library/PrivateFrameworks");

    let manifest_dir =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("missing CARGO_MANIFEST_DIR"));
    let target = env::var("TARGET").expect("missing TARGET");
    let profile = env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());

    println!("cargo:rustc-env=YORLING_TARGET_TRIPLE={target}");
    build_bridge_sidecar(&manifest_dir, &target, &profile);

    tauri_build::build();
}
