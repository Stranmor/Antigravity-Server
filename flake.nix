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
          config.allowUnfree = true; # Needed for some dockerTools dependencies
        };

        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" "rust-analyzer" ];
          targets = [ "wasm32-unknown-unknown" ];
        };

        # Build the frontend as a separate derivation for caching and reuse
        frontendDist = pkgs.runCommand "leptos-frontend-dist" {
          nativeBuildInputs = [ pkgs.trunk ];
        } ''
          mkdir -p $out
          cp -R src-leptos/dist/* $out/
        '';

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
            echo "📦 Building Antigravity Container Image..."
            # Build the image using Nix
            local_image_path=$(nix build .#antigravity-server-image --json | jq -r '.[0].outputs.out')
            echo "✅ Image built at: $local_image_path"

            echo "🚀 Loading Antigravity Container Image into Podman..."
            # Load the image into Podman
            podman load -i "$local_image_path"

            echo "📄 Generating Systemd Quadlet file..."
            # Generate the Quadlet file
            local_quadlet_path=$(nix build .#antigravity-manager-quadlet --json --no-link | jq -r '.[0].outputs.out')
            mkdir -p ~/.config/containers/systemd/
            cp "$local_quadlet_path" ~/.config/containers/systemd/antigravity-manager.container
            echo "✅ Quadlet file generated: ~/.config/containers/systemd/antigravity-manager.container"
            
            echo "♻️ Reloading Systemd and starting service..."
            # Reload systemd daemon and start/restart the service
            systemctl --user daemon-reload
            systemctl --user enable --now antigravity-manager.service
            systemctl --user restart antigravity-manager.service
            echo "✅ Antigravity Manager Container Service Started"
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
            echo "  build-image     - Build the Podman container image (nix build .#antigravity-server-image)"
            echo "  generate-quadlet - Generate systemd quadlet file (nix build .#antigravity-manager-quadlet)"
          '';
        };

        antigravity-server-bin = pkgs.rustPlatform.buildRustPackage {
          pname = "antigravity-server";
          version = self.version; # Use the version from flake.nix metadata
          src = ./.; # The entire repository is the source

          cargoLock = {
            lockFile = ./Cargo.lock;
          };

          # Explicitly build the antigravity-server package
          cargoBuildFlags = [ "-p" "antigravity-server" ];
          cargoInstallFlags = [ "-p" "antigravity-server" ];

          # Frontend must be built before the server if it serves static files
          preBuild = ''
            cd src-leptos && trunk build --release
            cd $OLDPWD
          '';
        };

        antigravity-server-image = pkgs.dockerTools.buildLayeredImage {
          name = "antigravity-manager";
          tag = "latest";
          created = "2026-01-17T00:00:00Z"; # Fixed date for reproducibility

          from = "debian:bullseye-slim"; # Base image for runtime

          # The Containerfile from the current directory
          dockerfile = ./Containerfile;

          # Context for the Dockerfile build, includes everything
          # This should ideally be a filtered source to reduce context size
          # For now, we use the entire repo as context to simplify
          buildContext = ./.;

          # These inputs represent the final binaries and static assets
          # that the Dockerfile expects to find in the build context
          # The Dockerfile build will COPY them from the builder stage
          # and also from the build context directly (e.g. src-leptos/dist)
          # So we need to ensure they are built and available.
          # For buildLayeredImage, this ensures the binaries are part of the image
          # and also satisfies the Dockerfile's expectations for COPYing from context.
          contents = [
            antigravity-server-bin
            frontendDist
          ];

          # We need to ensure the Rust toolchain is available for the builder stage in Dockerfile
          # This is implicitly handled by the Dockerfile's `FROM rust:1.92-slim-bullseye`
          # but we might need to specify nativeBuildInputs if Nix was building the image
          # directly without Dockerfile. Given we use a Dockerfile, it manages its own build env.
        };

        antigravity-manager-quadlet = pkgs.writeText "antigravity-manager.container" ''
          [Container]
          Image=antigravity-manager:latest
          # Run antigravity-server from /usr/local/bin within the container
          # The CMD in Containerfile already handles this, but explicit Exec ensures it.
          # Exec=/usr/local/bin/antigravity-server

          # Map container port 8045 to host port 8045
          Port=8045:8045

          # Mount host's ~/.antigravity directory into the container
          # This is where the server stores its data (db, logs etc.)
          Volume=%h/.antigravity:/app/.antigravity

          # Environment variables for the container
          # RUST_LOG is used for logging verbosity
          # ANTIGRAVITY_PORT should match the Port mapping
          # ANTIGRAVITY_STATIC_DIR points to the static files within the container
          Environment=RUST_LOG=info
          Environment=ANTIGRAVITY_PORT=8045
          Environment=ANTIGRAVITY_STATIC_DIR=/app/src-leptos/dist

          # Restart the container always if it exits
          Restart=always

          [Install]
          # Enable the service when the user logs in
          WantedBy=default.target
          # Enable this service by default
          # This makes `systemctl --user enable antigravity-manager.service` work after install
          # Or `podman generate systemd --new --files antigravity-manager`
        '';

      in
      {
        packages = scripts // {
          default = scripts.build-server;
          antigravity-server-image = antigravity-server-image; # Add the image to packages
          antigravity-manager-quadlet = antigravity-manager-quadlet; # Add the quadlet file to packages
        };

        devShells.default = pkgs.mkShell {
          buildInputs = [
            rustToolchain
            pkgs.trunk
            pkgs.pkg-config
            pkgs.openssl
            pkgs.binaryen # for wasm-opt
            pkgs.sass
            # Add podman for local container management
            pkgs.podman
          ] ++ builtins.attrValues scripts;

          shellHook = ''
            echo "🌌 Welcome to Antigravity Manager DevShell"
            echo "Type 'help' to see available commands."
            echo "To build the container image: nix build .#antigravity-server-image"
            echo "To load the image into Podman: podman load -i $(nix build .#antigravity-server-image --json | jq -r '.[0].outputs.out')"
            echo "To generate the systemd quadlet file: nix build .#antigravity-manager-quadlet"
            echo "To install the quadlet file: cat $(nix build .#antigravity-manager-quadlet --no-link --json | jq -r '.[0].outputs.out') > ~/.config/containers/systemd/antigravity-manager.container"
          '';
        };
      }
    );
}
