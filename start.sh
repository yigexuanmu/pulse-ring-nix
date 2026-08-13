#!/usr/bin/env bash
# pulse-ring 启动脚本 — NixOS 专用
# 解决 wgpu surface 创建失败问题（需要完整 nix store 库路径）
#
# 用法:
#   ./start.sh              # 默认启动 (debug preview off)
#   ./start.sh --debug      # 打开 PULSE_RING_DEBUG_PREVIEW=1
#   ./start.sh --kill       # 杀掉现有进程
#   ./start.sh --status     # 查看运行状态
#   ./start.sh --build      # 重新 build release

set -e

# === 路径配置 ===
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

# === 库路径 (NixOS 需要显式指定) ===
NVIDIA_LIB="/run/opengl-driver/lib"
VULKAN_PROFILE_LIB="/nix/store/g2nljbvj82q2b7c56csial73i4f7qag6-profile/lib"
LIBGLVND_LIB="/nix/store/bcm3mwyfja8rd2y995vcnzq06b341nnz-libglvnd-1.7.0/lib"
WAYLAND_LIB="/nix/store/lay2nvvimnv4snsmf3bk2bdnqdcdrg8d-wayland-1.26.0/lib"
ALSA_LIB="/nix/store/71ng8shgss15xpxmjs935cjdxsq2jwxd-alsa-lib-1.2.16.1/lib"
XKBCOMMON_LIB="/nix/store/f7dllvig9i72z13kzxczwq7wy8a1jpgg-libxkbcommon-1.13.2/lib"

# === 环境变量 ===
export WAYLAND_DISPLAY="${WAYLAND_DISPLAY:-wayland-1}"
export VK_ICD_FILENAMES="${VK_ICD_FILENAMES:-/run/opengl-driver/share/vulkan/icd.d/nvidia_icd.json}"
export LD_LIBRARY_PATH="$VULKAN_PROFILE_LIB:$LIBGLVND_LIB:$WAYLAND_LIB:$ALSA_LIB:$XKBCOMMON_LIB:$NVIDIA_LIB:${LD_LIBRARY_PATH:-}"
# nvidia driver 需要
export __GL_SHADER_DISK_CACHE=1
export __GL_SHADER_DISK_CACHE_PATH="${XDG_CACHE_HOME:-$HOME/.cache}/nvidia_shader_cache"

# === 命令分发 ===
case "${1:-run}" in
    --kill|-k|kill)
        echo "[start.sh] killing existing pulse-ring..."
        pkill -9 -f "target/release/pulse-ring" || true
        sleep 1
        pgrep -af "target/release/pulse-ring" || echo "[start.sh] no pulse-ring running"
        exit 0
        ;;
    --status|-s|status)
        echo "[start.sh] pulse-ring status:"
        pgrep -af "target/release/pulse-ring" || echo "  (not running)"
        echo
        echo "[start.sh] log tail:"
        tail -5 /tmp/pulse-ring.log 2>/dev/null || echo "  (no log)"
        exit 0
        ;;
    --build|-b|build)
        echo "[start.sh] building release..."
        nix develop .#default --command cargo build --release
        echo "[start.sh] build done"
        exit 0
        ;;
    --debug|-d|debug)
        export PULSE_RING_DEBUG_PREVIEW=1
        shift
        ;&
    run|"")
        # 检查 binary
        if [ ! -x "./target/release/pulse-ring" ]; then
            echo "[start.sh] ERROR: target/release/pulse-ring not found, run --build first"
            exit 1
        fi
        # 杀掉已有实例
        pkill -9 -f "target/release/pulse-ring" 2>/dev/null || true
        sleep 1
        # 启动 (前台运行, 方便看 log)
        echo "[start.sh] starting pulse-ring..."
        echo "[start.sh] WAYLAND_DISPLAY=$WAYLAND_DISPLAY"
        echo "[start.sh] VK_ICD_FILENAMES=$VK_ICD_FILENAMES"
        echo "[start.sh] PULSE_RING_DEBUG_PREVIEW=${PULSE_RING_DEBUG_PREVIEW:-0}"
        exec ./target/release/pulse-ring
        ;;
    --help|-h|help)
        sed -n '2,20p' "$0"
        exit 0
        ;;
    *)
        echo "Unknown arg: $1 (try --help)"
        exit 1
        ;;
esac
