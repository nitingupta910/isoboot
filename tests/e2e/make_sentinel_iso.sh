#!/usr/bin/env bash
# Build a tiny "sentinel" ISO whose loopback.cfg, when sourced by GRUB on
# the isoboot USB, prints a known string to serial and halts QEMU.
#
# This proves the entire boot chain works without needing to actually boot
# a real Linux distro:
#   GRUB on USB loads -> grub.cfg finds /isos/*.iso -> user enters submenu
#   -> loopback module mounts the ISO -> sources (loop)/boot/grub/loopback.cfg
#   -> our test script runs.

set -euo pipefail

OUT="${1:?usage: make_sentinel_iso.sh OUT.iso [SENTINEL_STRING]}"
SENTINEL="${2:-ISOBOOT_E2E_OK}"

WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT

mkdir -p "$WORK/boot/grub"
cat > "$WORK/boot/grub/loopback.cfg" <<EOF
# Sentinel loopback.cfg for isoboot end-to-end testing.
set timeout=0
set default=0
menuentry "isoboot E2E sentinel" {
    echo ""
    echo "=========================================="
    echo "  ${SENTINEL}"
    echo "=========================================="
    echo ""
    sleep --verbose 2
    halt
}
EOF

# Build a minimal ISO9660 image. -joliet for Windows-style names is
# unnecessary here but cheap.
xorriso -as mkisofs \
    -V ISOBOOT_E2E \
    -J -joliet-long -r \
    -o "$OUT" \
    "$WORK" >/dev/null
echo "wrote sentinel ISO: $OUT ($(stat -c%s "$OUT") bytes)"
