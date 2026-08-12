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

            buildInputs = [
              pkgs.alsa-lib
              pkgs.wayland
              pkgs.libxkbcommon
            ];

            postPatch = ''
              substituteInPlace src/main.rs \
                --replace-fail "/usr/share/fonts/TTF/JetBrains-Maple-Mono-NF-XX-XX/JetBrainsMapleMono-Regular.ttf" "${fontPaths.mapleMono}" \
                --replace-fail "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc" "${fontPaths.notoCJK}" \
                --replace-fail "/usr/share/fonts/noto/NotoSans-Regular.ttf" "${fontPaths.notoSans}" \
                --replace-fail "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf" "${fontPaths.dejavu}"
            '';

            # wgpu loads the Vulkan loader + ICDs via dlopen at runtime.
            postInstall = ''
              install -Dm644 config/pulse-ring.qml "$out/share/pulse-ring/pulse-ring.qml"
              install -Dm644 config/pulse-ring.lua "$out/share/pulse-ring/pulse-ring.lua"
              install -Dm644 LICENSE "$out/share/licenses/pulse-ring/LICENSE"
              wrapProgram "$out/bin/pulse-ring" \
                --prefix LD_LIBRARY_PATH : "${pkgs.vulkan-loader}/lib:${pkgs.mesa}/lib:${pkgs.libGL}/lib" \
                --prefix VK_ICD_FILENAMES : "${
                  pkgs.lib.concatStringsSep ":"
                  (map
                    (name: "${pkgs.mesa}/share/vulkan/icd.d/${name}")
                    (builtins.attrNames (builtins.readDir "${pkgs.mesa}/share/vulkan/icd.d")))
                }"
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
            ];
          };
        });
    };
}
