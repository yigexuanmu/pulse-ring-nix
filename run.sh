#!/usr/bin/env bash
# pulse-ring launcher for the nix store environment.
# Adds the right LD_LIBRARY_PATH so cargo-built binary can find libwayland,
# libvulkan, libEGL, libxkbcommon, and the system NVIDIA driver at /run/opengl-driver.

set -e
cd /tmp/opencode/pulse-ring-nix

export WAYLAND_DISPLAY="${WAYLAND_DISPLAY:-wayland-1}"
export VK_ICD_FILENAMES="/run/opengl-driver/share/vulkan/icd.d/nvidia_icd.json"

# Library paths gathered from `find /nix/store -name 'lib*.so*'`.
export LD_LIBRARY_PATH="/nix/store/g2nljbvj82q2b7c56csial73i4f7qag6-profile/lib"\
":/nix/store/bcm3mwyfja8rd2y995vcnzq06b341nnz-libglvnd-1.7.0/lib"\
":/nix/store/lay2nvvimnv4snsmf3bk2bdnqdcdrg8d-wayland-1.26.0/lib"\
":/nix/store/71ng8shgss15xpxmjs935cjdxsq2jwxd-alsa-lib-1.2.16.1/lib"\
":/nix/store/f7dllvig9i72z13kzxczwq7wy8a1jpgg-libxkbcommon-1.13.2/lib"\
":/run/opengl-driver/lib"\
":${LD_LIBRARY_PATH:-}"

exec ./target/release/pulse-ring "$@"
