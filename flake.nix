{
  description = "Antigravity Manager - Headless Daemon & WebUI";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, rust-overlay, flake-utils, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };

        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" "rust-analyzer" ];
          targets = [ "wasm32-unknown-unknown" ];
        };

        # Helper to create scripts
        mkScript = name: text: pkgs.writeShellScriptBin name text;

        # Scripts migrated from Justfile
        scripts = {
          build-server = mkScript "build-server" ''
            echo "📦 Building Antigravity Server..."
            cd src-leptos && trunk build --release
            cd ..
            cargo build --release -p antigravity-server
            echo "✅ Build complete: target/release/antigravity-server"
          '';

          install-server = mkScript "install-server" ''
            # Call the build script
            build-server
            
            echo "🚀 Installing Antigravity Server..."
            pkill -9 -f antigravity-server || true
            systemctl --user stop antigravity-manager || true
            
            mkdir -p ~/.local/bin
            cp target/release/antigravity-server ~/.local/bin/
            chmod +x ~/.local/bin/antigravity-server
            
            systemctl --user daemon-reload
            systemctl --user restart antigravity-manager
            echo "✅ Installed and Service Started"
            echo "🌐 WebUI available at: http://localhost:8045/"
          '';

          run-server = mkScript "run-server" ''
            echo "🚀 Starting Antigravity Server..."
            cd src-leptos && trunk build --release
            cd ..
            ANTIGRAVITY_STATIC_DIR=./src-leptos/dist cargo run --release -p antigravity-server
          '';

          clean = mkScript "clean" ''
            echo "🧹 Cleaning everything..."
            cargo clean
            rm -rf src-leptos/dist
            rm -rf src-leptos/target
            echo "✨ Sparkle clean"
          '';

          build-frontend = mkScript "build-frontend" ''
            echo "📦 Building Leptos Frontend..."
            cd src-leptos && trunk build --release
            echo "✅ Frontend built: src-leptos/dist/"
          '';

          sync-upstream = mkScript "sync-upstream" ''
            echo "🔄 Syncing upstream proxy code..."
            git fetch upstream
            ./scripts/sync-upstream.sh
            cargo check -p antigravity-core
            echo "✅ Sync complete. Review changes and commit."
          '';

          lint = mkScript "lint" ''
            cargo clippy --workspace -- -D warnings
          '';

          test-suite = mkScript "test-suite" ''
            cargo test --workspace
          '';

          check = mkScript "check" ''
            cargo check --workspace
          '';
          
          help = mkScript "help" ''
            echo "🦀 Antigravity Manager Nix Commands:"
            echo "  build-server    - Build server & frontend (Release)"
            echo "  install-server  - Install to ~/.local/bin & restart systemd"
            echo "  run-server      - Run locally (Debug/Release mix)"
            echo "  build-frontend  - Build WASM frontend only"
            echo "  sync-upstream   - Sync proxy logic from vendor"
            echo "  lint            - Run clippy"
            echo "  test-suite      - Run tests"
            echo "  clean           - Clean artifacts"
            echo "  check           - Check compilation"
          '';
        };

      in
      {
        packages = scripts // {
          default = scripts.build-server;
        };

        devShells.default = pkgs.mkShell {
          buildInputs = [
            rustToolchain
            pkgs.trunk
            pkgs.pkg-config
            pkgs.openssl
            pkgs.binaryen # for wasm-opt
            pkgs.sass
          ] ++ builtins.attrValues scripts;

          shellHook = ''
            echo "🌌 Welcome to Antigravity Manager DevShell"
            echo "Type 'help' to see available commands."
          '';
        };
      }
    );
}
