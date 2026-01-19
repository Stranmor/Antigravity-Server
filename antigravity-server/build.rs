use std::env;
use std::path::Path;
use std::process::Command;

fn main() {
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
    let workspace_root = Path::new(&manifest_dir).parent().unwrap();
    let leptos_dir = workspace_root.join("src-leptos");

    let trunk_check = Command::new("trunk").arg("--version").output();
    if trunk_check.is_err() {
        println!("cargo:warning=trunk not found, skipping frontend build");
        println!("cargo:warning=Install with: cargo install trunk");
        return;
    }

    println!("cargo:warning=Building Leptos frontend with trunk...");

    let profile = env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());
    let mut cmd = Command::new("trunk");
    cmd.arg("build").current_dir(&leptos_dir);

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
