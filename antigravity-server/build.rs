use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::SystemTime;

fn main() {
    println!("cargo:rerun-if-changed=../.git/HEAD");
    println!("cargo:rerun-if-changed=../.git/refs/tags");

    let version = Command::new("git")
        .args(["describe", "--tags", "--always", "--dirty"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string());

    println!("cargo:rustc-env=GIT_VERSION={}", version);

    let build_time = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    println!("cargo:rustc-env=BUILD_TIME={}", build_time);

    println!("cargo:rerun-if-changed=../src-leptos/src");
    println!("cargo:rerun-if-changed=../src-leptos/styles");
    println!("cargo:rerun-if-changed=../src-leptos/index.html");
    println!("cargo:rerun-if-changed=../src-leptos/Cargo.toml");
    println!("cargo:rerun-if-changed=../src-leptos/Trunk.toml");

    if env::var("CI").is_ok() || env::var("DOCS_RS").is_ok() || env::var("SKIP_TRUNK_BUILD").is_ok()
    {
        println!("cargo:warning=Skipping trunk build (CI/DOCS_RS/SKIP_TRUNK_BUILD set)");
        return;
    }

    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let manifest_path = Path::new(&manifest_dir);
    let leptos_dir = manifest_path
        .join("../src-leptos")
        .canonicalize()
        .unwrap_or_else(|_| {
            panic!(
                "src-leptos directory not found relative to {}",
                manifest_dir
            );
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

    let profile = env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());
    let trunk_target_dir = leptos_dir.join("target");
    let mut cmd = Command::new("trunk");
    cmd.arg("build")
        .current_dir(&leptos_dir)
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .env_remove("RUSTFLAGS")
        .env("CARGO_TARGET_DIR", &trunk_target_dir)
        .env("CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_LINKER", "rust-lld");

    if profile == "release" {
        cmd.arg("--release");
    }

    let status = cmd.status();
    match status {
        Ok(s) if s.success() => {
            println!("cargo:warning=Frontend build complete");
        }
        Ok(s) => {
            panic!("trunk build failed with exit code: {:?}", s.code());
        }
        Err(e) => {
            panic!("Failed to run trunk: {}", e);
        }
    }
}

fn should_skip_trunk_build(leptos_dir: &Path, dist_dir: &Path) -> bool {
    let wasm_mtime = match find_newest_wasm(dist_dir) {
        Some(t) => t,
        None => return false,
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

fn find_newest_wasm(dist_dir: &Path) -> Option<SystemTime> {
    let entries = fs::read_dir(dist_dir).ok()?;
    let mut newest: Option<SystemTime> = None;

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "wasm") {
            if let Ok(mtime) = fs::metadata(&path).and_then(|m| m.modified()) {
                newest = Some(match newest {
                    Some(n) if mtime > n => mtime,
                    Some(n) => n,
                    None => mtime,
                });
            }
        }
    }

    newest
}

fn get_newest_mtime(path: &Path) -> Option<SystemTime> {
    if path.is_file() {
        return fs::metadata(path).and_then(|m| m.modified()).ok();
    }

    if path.is_dir() {
        let mut newest: Option<SystemTime> = None;
        if let Ok(entries) = fs::read_dir(path) {
            for entry in entries.flatten() {
                if let Some(mtime) = get_newest_mtime(&entry.path()) {
                    newest = Some(match newest {
                        Some(n) if mtime > n => mtime,
                        Some(n) => n,
                        None => mtime,
                    });
                }
            }
        }
        return newest;
    }

    None
}
