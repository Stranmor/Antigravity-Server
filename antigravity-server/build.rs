//! Build script for antigravity-server.
//!
//! Handles git version extraction and Leptos frontend compilation via trunk.
//!
//! This script:
//! - Extracts git version from tags for runtime version display
//! - Compiles the Leptos WASM frontend using trunk
//! - Implements smart caching to skip rebuilds when sources unchanged

// Build scripts are allowed to panic on errors and use ? operator
#![allow(
    clippy::panic,
    clippy::question_mark_used,
    clippy::expect_used,
    reason = "Build scripts use panic/expect for fatal errors"
)]

use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::SystemTime;

/// Main entry point for the build script.
fn main() {
    println!("cargo:rerun-if-changed=../.git/HEAD");
    println!("cargo:rerun-if-changed=../.git/refs/tags");

    let version = Command::new("git")
        .args(["describe", "--tags", "--always", "--dirty"])
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|text| text.trim().to_owned())
        .filter(|text| !text.is_empty())
        .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_owned());

    println!("cargo:rustc-env=GIT_VERSION={version}");

    let build_time = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    println!("cargo:rustc-env=BUILD_TIME={build_time}");

    println!("cargo:rerun-if-changed=../src-leptos/src");
    println!("cargo:rerun-if-changed=../src-leptos/styles");
    println!("cargo:rerun-if-changed=../src-leptos/index.html");
    println!("cargo:rerun-if-changed=../src-leptos/Cargo.toml");
    println!("cargo:rerun-if-changed=../src-leptos/Trunk.toml");

    // Only skip on DOCS_RS (docs.rs build) or explicit SKIP_TRUNK_BUILD
    // NOTE: We intentionally do NOT skip on CI=true because that's often set
    // by shell scripts to disable prompts, not to skip frontend builds
    if env::var("DOCS_RS").is_ok() || env::var("SKIP_TRUNK_BUILD").is_ok() {
        println!("cargo:warning=Skipping trunk build (DOCS_RS/SKIP_TRUNK_BUILD set)");
        return;
    }

    let manifest_dir =
        env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR must be set by cargo");
    let manifest_path = Path::new(&manifest_dir);
    let leptos_dir = manifest_path.join("../src-leptos").canonicalize().unwrap_or_else(|_| {
        panic!("src-leptos directory not found relative to {manifest_dir}");
    });
    let dist_dir = leptos_dir.join("dist");

    if should_skip_trunk_build(&leptos_dir, &dist_dir) {
        println!("cargo:warning=Frontend up-to-date, skipping trunk build");
        return;
    }

    let trunk_check = Command::new("trunk").arg("--version").output();
    if trunk_check.is_err() {
        println!("cargo:warning=trunk not found, skipping frontend build");
        println!("cargo:warning=Install with: cargo install trunk");
        return;
    }

    println!("cargo:warning=Building Leptos frontend with trunk...");

    let profile = env::var("PROFILE").unwrap_or_else(|_| "debug".to_owned());
    let trunk_target_dir = leptos_dir.join("target");
    let mut cmd = Command::new("trunk");
    let _: &mut Command = cmd
        .arg("build")
        .current_dir(&leptos_dir)
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .env_remove("RUSTFLAGS")
        .env("CARGO_TARGET_DIR", &trunk_target_dir)
        .env("CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_LINKER", "rust-lld");

    if profile == "release" {
        let _: &mut Command = cmd.arg("--release");
    }

    let status = cmd.status();
    match status {
        Ok(exit_status) if exit_status.success() => {
            println!("cargo:warning=Frontend build complete");
        },
        Ok(exit_status) => {
            panic!("trunk build failed with exit code: {:?}", exit_status.code());
        },
        Err(err) => {
            panic!("Failed to run trunk: {err}");
        },
    }
}

/// Checks if trunk build can be skipped based on file timestamps.
///
/// Returns `true` if the compiled WASM is newer than all source files.
fn should_skip_trunk_build(leptos_dir: &Path, dist_dir: &Path) -> bool {
    let Some(wasm_mtime) = find_newest_wasm(dist_dir) else {
        return false;
    };

    let sources = [
        leptos_dir.join("src"),
        leptos_dir.join("styles"),
        leptos_dir.join("index.html"),
        leptos_dir.join("Cargo.toml"),
        leptos_dir.join("Trunk.toml"),
    ];

    for source in &sources {
        if let Some(source_mtime) = get_newest_mtime(source) {
            if source_mtime > wasm_mtime {
                return false;
            }
        }
    }

    true
}

/// Finds the newest WASM file in the dist directory.
///
/// Returns the modification time of the most recently modified `.wasm` file.
fn find_newest_wasm(dist_dir: &Path) -> Option<SystemTime> {
    let entries = fs::read_dir(dist_dir).ok()?;
    let mut newest: Option<SystemTime> = None;

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "wasm") {
            if let Ok(mtime) = fs::metadata(&path).and_then(|meta| meta.modified()) {
                newest = Some(match newest {
                    Some(current) if mtime > current => mtime,
                    Some(current) => current,
                    None => mtime,
                });
            }
        }
    }

    newest
}

/// Gets the newest modification time for a path (file or directory).
///
/// For files, returns the file's mtime. For directories, recursively finds
/// the newest mtime among all contained files.
fn get_newest_mtime(path: &Path) -> Option<SystemTime> {
    if path.is_file() {
        return fs::metadata(path).and_then(|meta| meta.modified()).ok();
    }

    if path.is_dir() {
        let mut newest: Option<SystemTime> = None;
        if let Ok(entries) = fs::read_dir(path) {
            for entry in entries.flatten() {
                if let Some(mtime) = get_newest_mtime(&entry.path()) {
                    newest = Some(match newest {
                        Some(current) if mtime > current => mtime,
                        Some(current) => current,
                        None => mtime,
                    });
                }
            }
        }
        return newest;
    }

    None
}
