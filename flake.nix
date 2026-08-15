{
  description = "pulse-ring-nix-Neo — Wayland music-reactive wallpaper with folia lyric visualization";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { inherit system overlays; };

        # Native libraries required by the Rust crates (gstreamer, wgpu/vulkan, audio, wayland).
        nativeBuildInputs = with pkgs; [
          pkg-config
          clang
          rustPlatform.bindgenHook
        ];

        buildInputs = with pkgs; [
          # GStreamer media pipeline (gstreamer crate → system-deps pkg-config)
          gst_all_1.gstreamer
          gst_all_1.gst-plugins-base
          gst_all_1.gst-plugins-good
          gst_all_1.gst-plugins-bad
          gst_all_1.gst-plugins-ugly
          gst_all_1.gst-libav
          # GLib / GObject (gstreamer + smithay deps)
          glib
          # Graphics: wgpu → vulkan; image rendering → cairo/pango
          vulkan-loader
          vulkan-headers
          libglvnd
          mesa
          cairo
          pango
          # Audio: cpal → alsa on Linux
          alsa-lib
          # Wayland compositor client
          wayland
          wayland-protocols
          wayland-scanner
          libxkbcommon
          # Font rasterization deps (ab_glyph/rusttype build)
          freetype
          fontconfig
        ];

        # Runtime LD_LIBRARY_PATH so the binary finds gstreamer/vulkan plugins at runtime.
        runtimeLibs = with pkgs; [
          gst_all_1.gstreamer
          gst_all_1.gst-plugins-base
          gst_all_1.gst-plugins-good
          gst_all_1.gst-plugins-bad
          gst_all_1.gst-plugins-ugly
          gst_all_1.gst-libav
          vulkan-loader
          libglvnd
          mesa
          wayland
          libxkbcommon
          fontconfig
          freetype
        ];

        # Rust toolchain pinned via overlay (stable, matches Cargo edition 2024).
        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" "rust-analyzer" ];
        };
      in
      {
        devShells.default = pkgs.mkShell {
          nativeBuildInputs = nativeBuildInputs ++ [ rustToolchain ];
          buildInputs = buildInputs
            ++ (with pkgs; [ nodejs electron glew ]);

          # Expose a unified PKG_CONFIG_PATH so cargo can locate every system lib.
          shellHook = ''
            # pkg-config .pc files live in each package's `dev` output under lib/pkgconfig.
            export PKG_CONFIG_PATH="${pkgs.lib.makeSearchPathOutput "dev" "lib/pkgconfig" buildInputs}:$PKG_CONFIG_PATH"
            # Let the Electron offscreen renderer find its libraries at runtime.
            export LD_LIBRARY_PATH="${pkgs.lib.makeLibraryPath runtimeLibs}:$LD_LIBRARY_PATH"
            # GStreamer plugin path for video/audio wallpaper decoding at runtime.
            export GST_PLUGIN_PATH="${pkgs.lib.makeSearchPathOutput "lib" "lib/gstreamer-1.0" runtimeLibs}:$GST_PLUGIN_PATH"
            # Vulkan ICD / wgpu device discovery.
            export VK_LAYER_PATH="${pkgs.vulkan-loader}/etc/vulkan/icd.d:$VK_LAYER_PATH"
            # Electron sandbox helper for offscreen rendering.
            export ELECTRON_DISABLE_SANDBOX=1
            # Use the Nix-provided Electron when running via `cargo run`, so the
            # dev loop needs no `npm install` in electron-wallpaper.
            export PULSE_RING_ELECTRON="${pkgs.electron}/bin/electron"
          '';
        };

        packages.pulse-ring = pkgs.rustPlatform.buildRustPackage {
          pname = "pulse-ring";
          version = "0.1.0";
          src = ./.;

          cargoLock = {
            lockFile = ./Cargo.lock;
          };

          # makeWrapper is needed by postInstall to wrap the binary with runtime
          # LD_LIBRARY_PATH / GST_PLUGIN_PATH; native libraries stay as in buildInputs.
          # Without `rec`, these RHS names resolve to the outer let-bound variables.
          nativeBuildInputs = nativeBuildInputs ++ [ pkgs.makeWrapper ];
          buildInputs = buildInputs;

          # The folia wallpaper JS bundle is shipped prebuilt in the repo
          # (folia-wallpaper/dist/), so no Node/npm/network is needed at build
          # time. A pure Nix sandbox can't `npm install` anyway (no network),
          # and the committed dist is the source of truth ("开箱即用无需 npm build").

          # Skipped checkPhase: the crate's unit tests rasterize glyphs and
          # need a usable system font + a Vulkan/Wayland surface, which a pure
          # build sandbox cannot provide. Tests are run manually via the devShell
          # (see `cargo test --bin pulse-ring folia_bridge` for the bridge suite).
          doCheck = false;

          postInstall = ''
            # Ship Electron helper + folia wallpaper assets next to the binary.
            mkdir -p $out/share/pulse-ring
            cp -r electron-wallpaper $out/share/pulse-ring/
            cp -r assets $out/share/pulse-ring/
            # folia-wallpaper/dist is referenced by the folia-lyrics pack's
            # project.json via a relative path, so the bundle must sit in the
            # same tree as `assets/` for the relative path to resolve at runtime.
            cp -r folia-wallpaper $out/share/pulse-ring/
            # Wrap the binary so it finds gstreamer/vulkan plugins at runtime
            # without the user having to set LD_LIBRARY_PATH manually, and point
            # the offscreen Electron helper at the Nix-provided Electron (the
            # CARGO_MANIFEST_DIR path baked in at compile time is stale for an
            # installed binary, so PULSE_RING_ELECTRON is the real resolver).
            wrapProgram $out/bin/pulse-ring \
              --prefix LD_LIBRARY_PATH : "${pkgs.lib.makeLibraryPath runtimeLibs}" \
              --prefix GST_PLUGIN_PATH : "${pkgs.lib.makeSearchPathOutput "lib" "lib/gstreamer-1.0" runtimeLibs}" \
              --set PULSE_RING_ELECTRON "${pkgs.electron}/bin/electron"
          '';

          meta = with pkgs.lib; {
            description = "Wayland wallpaper music-reactive visualizer (wgpu/Vulkan) + folia lyric effects";
            license = licenses.agpl3Only;
            mainProgram = "pulse-ring";
            platforms = platforms.linux;
          };
        };

        packages.default = self.packages.${system}.pulse-ring;

        apps.pulse-ring = {
          type = "app";
          program = "${self.packages.${system}.pulse-ring}/bin/pulse-ring";
        };
        apps.default = self.apps.${system}.pulse-ring;
      });
}
