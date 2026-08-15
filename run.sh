#!/usr/bin/env bash
# pulse-ring launcher — classic worktree edition.
#
# Goal: run the classic lyric engine OUT OF THE BOX on this (classic) worktree, without
# touching the user's ~/.config/pulse-ring/pulse-ring.qml — which may say `style: "sonnet"`
# left over from the connet branch and would otherwise shadow this worktree's defaults.
#
# Mechanism: redirect the config loader to an ISOLATED runtime copy whose `style:` line we
# rewrite to the chosen flavour. The program honours PULSE_RING_CONFIG first (see
# src/config.rs::config_path), and ensure_defaults() only ever writes ~/.config when those
# files are *missing*, so the user's global config is never modified or clobbered here.
#
# Menu (lyric style):  1) classic (default)   2) sonnet   3) off
#   - Interactive: prompts on a tty.
#   - Non-interactive: reads one line of stdin (`echo 1 | ./run.sh`) or defaults to classic
#     when stdin is closed (`./run.sh </dev/null`).
# Override: pre-export PULSE_RING_CONFIG=<path> to an existing file and this script honours
#   it as-is (the menu is skipped, your file is used verbatim).

set -e
cd "$(dirname "$0")"
WT="$PWD"

# ---- pick lyric style ------------------------------------------------------
pick_style() {
    local dflt=1
    if [ -t 0 ]; then
        cat >&2 <<'EOF'
pulse-ring (classic worktree) — choose lyric style:
  1) classic  (default)
  2) sonnet
  3) off
EOF
        read -r -p "choice [1]: " choice || choice=$dflt
    else
        read -r choice || choice=$dflt
    fi
    case "$choice" in
        2|sonnet) echo sonnet ;;
        3|off)    echo off ;;
        *)        echo classic ;;
    esac
}

# ---- isolated runtime config ----------------------------------------------
# If the user already pointed PULSE_RING_CONFIG at an existing file, honour it verbatim.
if [ -z "${PULSE_RING_CONFIG:-}" ] || [ ! -f "${PULSE_RING_CONFIG:-}" ]; then
    STYLE="$(pick_style)"
    RT="${XDG_RUNTIME_DIR:-/tmp}/pulse-ring-classic-$$"
    mkdir -p "$RT"
    export PULSE_RING_CONFIG="$RT/pulse-ring.qml"
    # Base = this worktree's curated bundled default (config/pulse-ring.qml, which has a
    # `style:` line). Rewrite only that line to the chosen flavour; every other value is the
    # shipped branch default. The classic branch's Default impl is Classic, so even a base
    # without a style line would still resolve to classic for the default menu entry.
    sed -E "s/^[[:space:]]*style:[[:space:]]*\"[^\"]*\"/    style: \"$STYLE\"/" \
        "$WT/config/pulse-ring.qml" > "$PULSE_RING_CONFIG"
else
    echo "run.sh: honouring pre-set PULSE_RING_CONFIG=$PULSE_RING_CONFIG" >&2
fi

# ---- runtime env (Wayland / Vulkan / nix libs) -----------------------------
export WAYLAND_DISPLAY="${WAYLAND_DISPLAY:-wayland-1}"
export VK_ICD_FILENAMES="/run/opengl-driver/share/vulkan/icd.d/nvidia_icd.json"
export LD_LIBRARY_PATH="/nix/store/g2nljbvj82q2b7c56csial73i4f7qag6-profile/lib"\
":/nix/store/bcm3mwyfja8rd2y995vcnzq06b341nnz-libglvnd-1.7.0/lib"\
":/nix/store/lay2nvvimnv4snsmf3bk2bdnqdcdrg8d-wayland-1.26.0/lib"\
":/nix/store/71ng8shgss15xpxmjs935cjdxsq2jwxd-alsa-lib-1.2.16.1/lib"\
":/nix/store/f7dllvig9i72z13kzxczwq7wy8a1jpgg-libxkbcommon-1.13.2/lib"\
":/run/opengl-driver/lib"\
":${LD_LIBRARY_PATH:-}"

exec "$WT/target/release/pulse-ring" "$@"
