{
  description = "pulse-ring-nix-Neo — Wayland music-reactive wallpaper with folia lyric visualization";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
    # wl-proxy: provide `wl-paper`, a Wayland proxy that wraps arbitrary
    # Wayland clients (Electron) into wlr-layer-shell surfaces. We use it to
    # run the folia Electron renderer as a native layer-shell overlay
    # (transparent, compositor-direct, no stdout framebuffer pipe) sitting
    # ABOVE pulse-ring's own Layer::Background surface. Pure-Rust crate
    # (pre-generated protocols, no system-deps / wayland-scanner at build
    # time), so builds cleanly in a pure Nix sandbox.
    # NOTE: wl-proxy upstream has no flake.nix (plain cargo workspace), so we
    # treat it as a tarball source (`flake = false`) and build `wl-paper` via
    # buildRustPackage in the let block below.
    wl-proxy = {
      url = "github:mahkoh/wl-proxy";
      flake = false;
    };
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay, wl-proxy }:
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
          # Font rasterization deps (ab_glyph/rusttype build).
          # NOTE: no `fontconfig` here — fonts are baked into the binary at build
          # time via `postPatch` substituteInPlace (Nix store paths), so load_font()
          # reads them directly with no runtime fc-match lookup.
          freetype
          # GTK4 / libadwaita: 配置 GUI (bin: pulse-ring-config) 所需的 native libs.
          # gtk4-rs / libadwaita-rs 是纯 Rust crate, 但通过 system-deps 在构建期
          # 用 pkg-config 查这些系统库; 运行时也需在 LD_LIBRARY_PATH.
          gtk4
          libadwaita
          graphene
          harfbuzz
          gdk-pixbuf
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
          freetype
          # GTK4 / libadwaita runtime libs for pulse-ring-config GUI.
          gtk4
          libadwaita
          graphene
          harfbuzz
          gdk-pixbuf
        ];

        # Rust toolchain pinned via overlay (stable, matches Cargo edition 2024).
        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" "rust-analyzer" ];
        };

        # `wl-paper` binary (from wl-proxy input) — wraps Electron as a
        # wlr-layer-shell surface. Pure-Rust crate (edition 2024, rust 1.89+);
        # protocols are pre-generated so no wayland-scanner/system-deps at
        # build time. We build only the `apps/wl-paper` subcrate via
        # buildAndTestSubdir (the workspace root has no [package], only
        # [workspace]). Consumed in web_wallpaper.rs via PULSE_RING_WL_PAPER.
        wlPaper = pkgs.rustPlatform.buildRustPackage {
          pname = "wl-paper";
          version = "0.1.0";
          src = wl-proxy;
          cargoLock.lockFile = wl-proxy + "/Cargo.lock";
          buildAndTestSubdir = "apps/wl-paper";
          doCheck = false;
        };

        # Fonts baked into the binary at build time (postPatch substituteInPlace
        # replaces the Arch/FHS hard-coded paths in src/main.rs with these Nix
        # store paths). This mirrors the master-branch packaging and frees the
        # runtime of any fontconfig/fc-match dependency.
        fonts = with pkgs; {
          mapleMono = maple-mono.NF-CN;
          notoCJK = noto-fonts-cjk-sans;
          notoSans = noto-fonts;
          dejavu = dejavu_fonts;
        };
        fontPaths = {
          mapleMono = "${fonts.mapleMono}/share/fonts/truetype/MapleMono-NF-CN-Regular.ttf";
          notoCJK = "${fonts.notoCJK}/share/fonts/opentype/noto-cjk/NotoSansCJK-VF.otf.ttc";
          notoSans = "${fonts.notoSans}/share/fonts/noto/NotoSans.ttf";
          dejavu = "${fonts.dejavu}/share/fonts/truetype/DejaVuSans.ttf";
        };

        # Runtime PATH additions (wrapProgram --prefix PATH): `pactl`
        # (pulseaudio) is what cpal needs to discover the PipeWire default sink
        # + monitor source. Without it `audio: capture on None` fails and
        # falls back to a silent fake sine — the spectrum bars won't track music.
        runtimeBins = [ pkgs.pulseaudio ];
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
            # `pactl` (pulseaudio) is needed at runtime to discover the default
            # sink + monitor source so cpal can capture the PipeWire monitor.
            # DevShell `cargo run` needs it on PATH the same way the installed
            # wrapper does (wrapProgram --prefix PATH).
            export PATH="${pkgs.lib.makeBinPath runtimeBins}:$PATH"
            # Bundled wallpaper library: lets `cargo run` resolve packaged
            # wallpaper packs (e.g. folia-lyrics) the same way the installed
            # wrapper does, without copying packs into ~/.config/.
            export PULSE_RING_WALLPAPER_LIB="$PWD/assets/wallpapers"
            # wl-paper binary: wraps Electron as a layer-shell surface (used by
            # web_wallpaper.rs to composite the folia renderer natively on top
            # of pulse-ring's Layer::Background surface).
            export PULSE_RING_WL_PAPER="${wlPaper}/bin/wl-paper"
          '';
        };

        packages.pulse-ring = pkgs.rustPlatform.buildRustPackage {
          pname = "pulse-ring";
          version = "0.1.0";
          src = ./.;

          cargoLock = {
            lockFile = ./Cargo.lock;
          };

          # Bake Nix store font paths into the binary at build time (load_font()'s
          # Arch/FHS hard-coded candidates don't exist on NixOS). This removes the
          # runtime fontconfig/fc-match lookup entirely.
          postPatch = ''
            substituteInPlace src/main.rs \
              --replace-fail "/usr/share/fonts/TTF/JetBrains-Maple-Mono-NF-XX-XX/JetBrainsMapleMono-Regular.ttf" "${fontPaths.mapleMono}" \
              --replace-fail "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc" "${fontPaths.notoCJK}" \
              --replace-fail "/usr/share/fonts/noto/NotoSans-Regular.ttf" "${fontPaths.notoSans}" \
              --replace-fail "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf" "${fontPaths.dejavu}"
          '';

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
              --prefix PATH : "${pkgs.lib.makeBinPath runtimeBins}" \
              --set PULSE_RING_ELECTRON "${pkgs.electron}/bin/electron" \
              --set PULSE_RING_HELPER "$out/share/pulse-ring/electron-wallpaper/main.js" \
              --set PULSE_RING_WALLPAPER_LIB "$out/share/pulse-ring/assets/wallpapers" \
              --set PULSE_RING_WL_PAPER "${wlPaper}/bin/wl-paper"
          '';

          meta = with pkgs.lib; {
            description = "Wayland wallpaper music-reactive visualizer (wgpu/Vulkan) + folia lyric effects";
            license = licenses.agpl3Only;
            mainProgram = "pulse-ring";
            platforms = platforms.linux;
          };
        };

        packages.default = self.packages.${system}.pulse-ring;

        # Standalone `wl-paper` binary ( Wayland proxy for arbitrary clients).
        # Also consumed internally by the pulse-ring package (wrapProgram env).
        packages.wl-paper = wlPaper;

        apps.pulse-ring = {
          type = "app";
          program = "${self.packages.${system}.pulse-ring}/bin/pulse-ring";
        };
        apps.default = self.apps.${system}.pulse-ring;
      });
}
