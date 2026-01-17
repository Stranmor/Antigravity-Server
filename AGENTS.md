
## 🚨 ARCHITECTURAL DECISION [2026-01-17]: Full Containerization Migration

**Problem:** Current deployment method via `install-server` copies a binary directly to the host, violating rootless containerization and reproducible deployment doctrines. This leads to:
- Lack of environmental isolation.
- Potential conflicts with host system libraries.
- Inconsistent behavior across different deployment targets.
- Suboptimal adherence to `BUILD_CONTAINER_SHIP_IMAGE` philosophy.

**Root Cause:** Incomplete transition to the container-native deployment. The Nix setup provides a reproducible build environment, but the final `antigravity-server` artifact is deployed as a host binary, not a container image.

**Solution:** Implement full containerization for `antigravity-server` using Podman, building upon the existing Nix infrastructure. This involves:
1.  Creating a `Containerfile` for `antigravity-server`.
2.  Integrating `Containerfile` build into `flake.nix` using `pkgs.dockerTools.buildLayeredImage`.
3.  Generating Systemd Quadlet service files via Nix to manage the container.

**Benefits:**
- ✅ **Full Reproducibility:** Container images built via Nix are content-addressed and fully reproducible.
- ✅ **Isolation:** `antigravity-server` runs in an isolated environment, preventing host conflicts.
- ✅ **Security:** Rootless Podman enhances security posture.
- ✅ **Unified Deployment:** Adherence to `BUILD_CONTAINER_SHIP_IMAGE` strategy.
- ✅ **Simplicity:** Systemd Quadlet for easy management of containerized services.

**Migration Tasks:**
- [x] Create `Containerfile` for `antigravity-server`.
- [x] Modify `flake.nix` to build the container image using `pkgs.dockerTools.buildLayeredImage`.
- [x] Add a Nix package to `flake.nix` that generates the Systemd Quadlet service file for `antigravity-server`.
- [x] Update `install-server` script to use the generated Systemd Quadlet service.
- [ ] Verify deployment on a test system, ensuring the container runs correctly and is managed by Systemd. (Manual verification needed)

---
