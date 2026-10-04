use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SidecarManifest {
    schema_version: u32,
    target: String,
    goos: String,
    goarch: String,
    package_version: String,
    file_name: String,
    sha256: String,
    size: u64,
}

fn sha256_hex(path: &Path) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|error| format!("read {}: {error}", path.display()))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn expected_file_name(target: &str) -> String {
    let extension = if target.contains("-windows-") {
        ".exe"
    } else {
        ""
    };
    format!("agenthub-adapterd-{target}{extension}")
}

fn emit_sidecar_identity() -> Result<(), String> {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").map_err(|e| e.to_string())?);
    let target = env::var("TARGET").map_err(|e| e.to_string())?;
    let file_name = expected_file_name(&target);
    let binary = manifest_dir.join("binaries").join(&file_name);
    let manifest = manifest_dir
        .join("binaries")
        .join(format!("{file_name}.json"));
    println!("cargo:rerun-if-changed={}", binary.display());
    println!("cargo:rerun-if-changed={}", manifest.display());

    match (binary.is_file(), manifest.is_file()) {
        (false, false) => {
            // Ordinary cargo check does not require generated bundle inputs.
            // Tauri bundling still fails closed because externalBin requires
            // the target-specific binary before producing an installer.
            println!("cargo:rustc-env=AGENTHUB_ADAPTERD_BUNDLED_SHA256=");
            println!("cargo:rustc-env=AGENTHUB_ADAPTERD_BUNDLED_VERSION=");
            Ok(())
        }
        (true, true) => {
            let raw = fs::read(&manifest)
                .map_err(|error| format!("read {}: {error}", manifest.display()))?;
            let parsed: SidecarManifest = serde_json::from_slice(&raw)
                .map_err(|error| format!("parse {}: {error}", manifest.display()))?;
            let package_version = env::var("CARGO_PKG_VERSION").map_err(|e| e.to_string())?;
            let size = fs::metadata(&binary)
                .map_err(|error| format!("stat {}: {error}", binary.display()))?
                .len();
            let digest = sha256_hex(&binary)?;
            if parsed.schema_version != 1
                || parsed.target != target
                || parsed.package_version != package_version
                || parsed.file_name != file_name
                || parsed.size != size
                || parsed.sha256 != digest
                || parsed.sha256.len() != 64
                || parsed.goos.is_empty()
                || parsed.goarch.is_empty()
            {
                return Err(format!(
                    "generated Go sidecar manifest does not match {}",
                    binary.display()
                ));
            }
            println!(
                "cargo:rustc-env=AGENTHUB_ADAPTERD_BUNDLED_SHA256={}",
                parsed.sha256
            );
            println!(
                "cargo:rustc-env=AGENTHUB_ADAPTERD_BUNDLED_VERSION={}",
                parsed.package_version
            );
            Ok(())
        }
        _ => Err(format!(
            "generated Go sidecar binary and manifest must both exist: {}, {}",
            binary.display(),
            manifest.display()
        )),
    }
}

fn main() {
    if let Err(error) = emit_sidecar_identity() {
        panic!("{error}");
    }
    tauri_build::build()
}
