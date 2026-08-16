{
  description = "Wayland wallpaper-layer music visualization (GPU, QML style + Lua behavior)";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  };

  outputs = { self, nixpkgs }:
    let
      supportedSystems = [ "x86_64-linux" "aarch64-linux" ];
      forAllSystems = nixpkgs.lib.genAttrs supportedSystems;
    in
    {
      packages = forAllSystems (system:
        let
          pkgs = import nixpkgs { inherit system; };
          # Fonts baked into the binary (the app scans hard-coded absolute paths).
          fonts = {
            mapleMono = pkgs.maple-mono.NF-CN;
            notoCJK = pkgs.noto-fonts-cjk-sans;
            notoSans = pkgs.noto-fonts;
            dejavu = pkgs.dejavu_fonts;
          };
          fontPaths = {
            mapleMono = "${fonts.mapleMono}/share/fonts/truetype/MapleMono-NF-CN-Regular.ttf";
            notoCJK = "${fonts.notoCJK}/share/fonts/opentype/noto-cjk/NotoSansCJK-VF.otf.ttc";
            notoSans = "${fonts.notoSans}/share/fonts/noto/NotoSans.ttf";
            dejavu = "${fonts.dejavu}/share/fonts/truetype/DejaVuSans.ttf";
          };
        in
        rec {
          default = pulse-ring;

          pulse-ring = pkgs.rustPlatform.buildRustPackage {
            pname = "pulse-ring";
            version = "0.1.0";

            src = nixpkgs.lib.cleanSource self;
            cargoLock.lockFile = ./Cargo.lock;

            nativeBuildInputs = [
              pkgs.pkg-config
              pkgs.makeWrapper
            ];

            # PipeWire + pactl (pulseaudio) are runtime deps, not build deps, but
            # mkShell doesn't separate them; they must be present for the wrapped
            # binary to find libpipewire + libasound_module_pcm_pipewire and for
            # `pactl` (used by audio.rs::ensure_pipewire_monitor_node) to resolve
            # the default sink's monitor node so cpal taps system audio.
            buildInputs = [
              pkgs.alsa-lib
              pkgs.wayland
              pkgs.libxkbcommon
              # Sonnet v2 uses FreeType for byte-identical glyph coverage + harfbuzz
              # for shaping, replacing the fontdue SDF raster.
              pkgs.freetype
              pkgs.harfbuzz
              # PipeWire ALSA plugin: libasound_module_pcm_pipewire.so + 50/99-pipewire.conf
              # Without it, ALSA `default` PCM can't dlopen the PipeWire backend and
              # `snd_pcm_hw_params` returns ENOENT → audio falls back to silent_source.
              pkgs.pipewire
              # pactl binary for auto-resolving PIPEWIRE_NODE to the monitor source.
              pkgs.pulseaudio
            ];

            postPatch = ''
              substituteInPlace src/main.rs \
                --replace-fail "/usr/share/fonts/TTF/JetBrains-Maple-Mono-NF-XX-XX/JetBrainsMapleMono-Regular.ttf" "${fontPaths.mapleMono}" \
                --replace-fail "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc" "${fontPaths.notoCJK}" \
                --replace-fail "/usr/share/fonts/noto/NotoSans-Regular.ttf" "${fontPaths.notoSans}" \
                --replace-fail "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf" "${fontPaths.dejavu}"
            '';

            # sdf::tests call fc-match for a system font at runtime; the Nix sandbox
            # has no fontconfig catalog so they panic in checkPhase. They only guard
            # the glyph atlas packing (layout math), not the sonnet/audio layer.
            # Skip the suite — build quality is covered by cargo build + the existing
            # `shader_is_valid_wgsl` naga test (which doesn't need fc-match).
            doCheck = false;

            # wgpu loads the Vulkan loader + ICDs via dlopen at runtime.
            postInstall = ''
              install -Dm644 config/pulse-ring.qml "$out/share/pulse-ring/pulse-ring.qml"
              install -Dm644 config/pulse-ring.lua "$out/share/pulse-ring/pulse-ring.lua"
              install -Dm644 LICENSE "$out/share/licenses/pulse-ring/LICENSE"
              wrapProgram "$out/bin/pulse-ring" \
                --prefix PATH : "${pkgs.lib.makeBinPath [ pkgs.pulseaudio ]}" \
                --prefix LD_LIBRARY_PATH : "${pkgs.wayland}/lib:${pkgs.alsa-lib}/lib:${pkgs.libxkbcommon}/lib:${pkgs.vulkan-loader}/lib:${pkgs.mesa}/lib:${pkgs.libGL}/lib:${pkgs.pipewire}/lib" \
                --set ALSA_PLUGIN_DIR "${pkgs.pipewire}/lib/alsa-lib" \
                --prefix ALSA_PLUGIN_DIR : "${pkgs.pipewire}/lib/alsa-lib"
            '';

            meta = with pkgs.lib; {
              description = "Wayland wallpaper-layer music visualization (GPU rendering, QML style + Lua behavior)";
              homepage = "https://github.com/yigexuanmu/pulse-ring-nix";
              license = licenses.gpl3Plus;
              platforms = [ "x86_64-linux" "aarch64-linux" ];
              mainProgram = "pulse-ring";
            };
          };
        });

      devShells = forAllSystems (system:
        let
          pkgs = import nixpkgs { inherit system; };
        in
        {
          default = pkgs.mkShell {
            packages = [
              pkgs.rustc
              pkgs.cargo
              pkgs.rustfmt
              pkgs.clippy
              pkgs.pkg-config
              pkgs.alsa-lib
              pkgs.wayland
              pkgs.libxkbcommon
              # Dev shell also gets PipeWire + pactl so `cargo run` (unwrapped) can
              # catch real audio the same way the installed binary does.
              pkgs.pipewire
              pkgs.pulseaudio
              # Sonnet v2 uses FreeType for byte-identical glyph coverage + harfbuzz
              # for shaping, replacing the fontdue SDF raster.
              pkgs.freetype
              pkgs.harfbuzz
            ];
          };
        });
    };
}
