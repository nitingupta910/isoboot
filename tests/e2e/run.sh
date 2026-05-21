#!/usr/bin/env bash
# End-to-end test for isoboot.
#
# Builds a raw USB image, installs isoboot onto it via a loopback device,
# places either a hermetic "sentinel" ISO or a real Linux ISO in /isos/,
# then boots the image under QEMU (BIOS and/or UEFI) and verifies that
# GRUB renders the menu and the chosen ISO actually executes.
#
# Requires root for losetup/mount. Run with sudo.
#
# Usage:
#   sudo tests/e2e/run.sh                     # smoke test (sentinel ISO)
#   sudo tests/e2e/run.sh --iso PATH.iso      # boot a real local ISO
#   sudo tests/e2e/run.sh --download-alpine   # grab Alpine virt ISO (~60 MB)
#   sudo tests/e2e/run.sh --firmware uefi     # uefi only (default: both)
#   sudo tests/e2e/run.sh --keep              # don't tear down image on exit
#   sudo tests/e2e/run.sh --timeout 60        # QEMU run-time cap, seconds

set -euo pipefail

# ----- argument parsing -----
MODE="smoke"
ISO_PATH=""
FIRMWARE="both"
TIMEOUT=45
KEEP=0
IMAGE_MB=512
SENTINEL="ISOBOOT_E2E_OK"
CACHE_DIR="/var/cache/isoboot-test"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --iso) MODE="iso"; ISO_PATH="$2"; shift 2;;
        --download-alpine) MODE="iso"; ISO_PATH="__DOWNLOAD_ALPINE__"; shift;;
        --multi) MODE="multi"; IMAGE_MB=8192; TIMEOUT=180; shift;;
        --cache-dir) CACHE_DIR="$2"; shift 2;;
        --firmware) FIRMWARE="$2"; shift 2;;
        --timeout) TIMEOUT="$2"; shift 2;;
        --keep) KEEP=1; shift;;
        --image-mb) IMAGE_MB="$2"; shift 2;;
        -h|--help) sed -n '2,20p' "$0"; exit 0;;
        *) echo "unknown arg: $1" >&2; exit 2;;
    esac
done

if [[ $EUID -ne 0 ]]; then
    echo "error: must run as root (try sudo)" >&2
    exit 1
fi

REPO=$(cd "$(dirname "$0")/../.." && pwd)
BIN="$REPO/target/release/isoboot"
if [[ ! -x "$BIN" ]]; then
    echo ">>> building isoboot (release)…"
    (cd "$REPO" && cargo build --release)
fi

WORK=$(mktemp -d /tmp/isoboot-e2e.XXXXXX)
# Make workdir world-readable so an unprivileged user can tail the
# serial logs in real time. The image, OVMF vars, and mount points
# all live here; none contain secrets, just test artifacts.
chmod 0755 "$WORK"
umask 0022
IMG="$WORK/usb.img"
LOOP=""

cleanup() {
    set +e
    if [[ -n "$LOOP" ]]; then
        umount -R "$WORK"/mnt-* 2>/dev/null
        losetup -d "$LOOP" 2>/dev/null
    fi
    if [[ $KEEP -eq 1 ]]; then
        echo "(--keep) workdir preserved: $WORK"
    else
        rm -rf "$WORK"
    fi
}
trap cleanup EXIT

# ----- create image and attach as loop device -----
echo ">>> creating $IMAGE_MB MB raw image: $IMG"
truncate -s "${IMAGE_MB}M" "$IMG"
LOOP=$(losetup --partscan --find --show "$IMG")
echo ">>> attached as $LOOP"

# ----- run isoboot install -----
echo ">>> running isoboot install --device $LOOP"
"$BIN" install --device "$LOOP" --yes --verbose

# Re-trigger partition scan after parted operations on a loop device.
partprobe "$LOOP" 2>/dev/null || true
udevadm settle

DATA_PART="${LOOP}p3"
ESP_PART="${LOOP}p2"

# ----- stage an ISO into /isos/ -----
MNT_DATA="$WORK/mnt-data"
mkdir -p "$MNT_DATA"
mount "$DATA_PART" "$MNT_DATA"

case "$MODE" in
    smoke)
        echo ">>> building hermetic sentinel ISO"
        bash "$REPO/tests/e2e/make_sentinel_iso.sh" "$WORK/sentinel.iso" "$SENTINEL"
        cp "$WORK/sentinel.iso" "$MNT_DATA/isos/test-sentinel.iso"
        EXPECT="$SENTINEL"
        # Shrink the outer menu timeout so the test progresses fast.
        # We mount the ESP separately just for this surgery.
        MNT_ESP="$WORK/mnt-esp"
        mkdir -p "$MNT_ESP"
        mount "$ESP_PART" "$MNT_ESP"
        sed -i 's/^set timeout=15$/set timeout=2/' "$MNT_ESP/boot/grub/grub.cfg"
        sync
        umount "$MNT_ESP"
        ;;
    iso)
        if [[ "$ISO_PATH" == "__DOWNLOAD_ALPINE__" ]]; then
            ISO_URL="https://dl-cdn.alpinelinux.org/alpine/latest-stable/releases/x86_64/alpine-virt-3.23.0-x86_64.iso"
            ISO_PATH="$WORK/alpine.iso"
            echo ">>> downloading $ISO_URL"
            curl -fsSL -o "$ISO_PATH" "$ISO_URL"
        fi
        if [[ ! -f "$ISO_PATH" ]]; then
            echo "error: ISO not found: $ISO_PATH" >&2
            exit 1
        fi
        echo ">>> copying $(basename "$ISO_PATH") into /isos/"
        cp "$ISO_PATH" "$MNT_DATA/isos/"
        EXPECT="Linux version"   # generic kernel banner; works for ~any distro
        ;;
    multi)
        mkdir -p "$CACHE_DIR"

        URL_alpine="https://dl-cdn.alpinelinux.org/alpine/latest-stable/releases/x86_64/alpine-virt-3.23.0-x86_64.iso"
        URL_ubuntu="https://releases.ubuntu.com/26.04/ubuntu-26.04-live-server-amd64.iso"
        URL_fedora="https://download.fedoraproject.org/pub/fedora/linux/releases/43/Server/x86_64/iso/Fedora-Server-netinst-x86_64-43-1.6.iso"

        download_one() {
            local name="$1"
            local url="$2"
            local target="$CACHE_DIR/$name.iso"
            if [[ -s "$target" ]]; then
                echo "  [cache] $name: $(numfmt --to=iec --suffix=B $(stat -c%s "$target"))"
                return 0
            fi
            echo "  [fetch] $name: $url"
            curl --fail --location --silent --show-error --retry 3 \
                 -o "$target.part" "$url" && mv "$target.part" "$target"
        }

        echo ">>> downloading 3 ISOs in parallel (cache: $CACHE_DIR)"
        download_one alpine "$URL_alpine" &
        PID_a=$!
        download_one ubuntu "$URL_ubuntu" &
        PID_u=$!
        download_one fedora "$URL_fedora" &
        PID_f=$!

        wait $PID_a || { echo "alpine download failed" >&2; exit 1; }
        wait $PID_u || { echo "ubuntu download failed" >&2; exit 1; }
        wait $PID_f || { echo "fedora download failed" >&2; exit 1; }

        echo ">>> copying ISOs onto data partition"
        cp "$CACHE_DIR/alpine.iso" "$MNT_DATA/isos/alpine.iso"
        cp "$CACHE_DIR/ubuntu.iso" "$MNT_DATA/isos/ubuntu.iso"
        cp "$CACHE_DIR/fedora.iso" "$MNT_DATA/isos/fedora.iso"

        # Swap in the test-tuned grub.cfg (per-distro entries with
        # console=ttyS0 baked into kernel args).
        MNT_ESP="$WORK/mnt-esp"
        mkdir -p "$MNT_ESP"
        mount "$ESP_PART" "$MNT_ESP"
        cp "$REPO/tests/e2e/multi-test-grub.cfg" "$MNT_ESP/boot/grub/grub.cfg"
        sync
        umount "$MNT_ESP"
        ;;
esac

ls -la "$MNT_DATA/isos/"
sync
umount "$MNT_DATA"

# Detach so QEMU has exclusive access to the image file.
losetup -d "$LOOP"
LOOP=""

# ----- boot under QEMU -----
qemu_run() {
    local mode="$1" extra=()
    case "$mode" in
        bios)
            ;;
        uefi)
            local ovmf_code="/usr/share/OVMF/OVMF_CODE_4M.fd"
            local ovmf_vars="/usr/share/OVMF/OVMF_VARS_4M.fd"
            if [[ ! -f "$ovmf_code" ]]; then
                ovmf_code="/usr/share/OVMF/OVMF_CODE.fd"
                ovmf_vars="/usr/share/OVMF/OVMF_VARS.fd"
            fi
            if [[ ! -f "$ovmf_code" || ! -f "$ovmf_vars" ]]; then
                echo "  SKIP uefi: OVMF firmware not found"
                return 0
            fi
            cp "$ovmf_vars" "$WORK/OVMF_VARS.fd"
            extra=(
                -drive "if=pflash,format=raw,readonly=on,file=$ovmf_code"
                -drive "if=pflash,format=raw,file=$WORK/OVMF_VARS.fd"
            )
            ;;
    esac

    local log="$WORK/serial-$mode.log"
    echo ">>> booting QEMU ($mode, timeout ${TIMEOUT}s) — log: $log"
    set +e
    timeout --foreground "$TIMEOUT" qemu-system-x86_64 \
        -m 1024 \
        -nodefaults \
        -no-reboot \
        -display none \
        -serial "file:$log" \
        -drive "file=$IMG,format=raw,if=virtio,cache=none" \
        "${extra[@]}" \
        >/dev/null 2>&1
    local rc=$?
    set -e

    echo "--- serial tail ($mode) ---"
    tail -20 "$log" 2>/dev/null || echo "(no serial output captured)"
    echo "---"

    if grep -q "$EXPECT" "$log" 2>/dev/null; then
        echo "  PASS [$mode]: matched '$EXPECT' in serial output"
        return 0
    else
        echo "  FAIL [$mode]: did not see '$EXPECT' within ${TIMEOUT}s (qemu exit=$rc)"
        return 1
    fi
}

fail=0

if [[ "$MODE" == "multi" ]]; then
    # For each distro, patch grub.cfg's default= line, boot UEFI, and assert
    # we see early-kernel output specific to that distro on the serial log.
    # Re-attach the loop device long enough to surgically edit grub.cfg.
    # Per-distro "we definitely booted this" signatures. We require at least
    # one to appear in the serial log. Multiple alternatives guard against
    # transient differences in distro init output.
    declare -A SIGS
    SIGS[alpine]="Linux version|Alpine Init|Welcome to Alpine"
    SIGS[ubuntu]="Linux version|Ubuntu .* LTS|casper|systemd .* running"
    SIGS[fedora]="Linux version|Fedora|anaconda|dracut-initqueue"

    pass_count=0
    for distro in alpine ubuntu fedora; do
        echo ""
        echo "============================================================"
        echo ">>> boot test: $distro"
        echo "============================================================"

        LOOP=$(losetup --partscan --find --show "$IMG")
        mkdir -p "$WORK/mnt-esp"
        mount "${LOOP}p2" "$WORK/mnt-esp"
        sed -i "s/^set default=.*/set default=$distro/" "$WORK/mnt-esp/boot/grub/grub.cfg"
        sync
        umount "$WORK/mnt-esp"
        losetup -d "$LOOP"
        LOOP=""

        EXPECT="${SIGS[$distro]}"
        log="$WORK/serial-multi-$distro.log"

        ovmf_code="/usr/share/OVMF/OVMF_CODE_4M.fd"
        ovmf_vars="/usr/share/OVMF/OVMF_VARS_4M.fd"
        cp "$ovmf_vars" "$WORK/OVMF_VARS-$distro.fd"

        echo ">>> launching QEMU UEFI (timeout ${TIMEOUT}s) — log: $log"
        set +e
        timeout --foreground "$TIMEOUT" qemu-system-x86_64 \
            -m 2048 \
            -nodefaults \
            -no-reboot \
            -display none \
            -serial "file:$log" \
            -drive "if=pflash,format=raw,readonly=on,file=$ovmf_code" \
            -drive "if=pflash,format=raw,file=$WORK/OVMF_VARS-$distro.fd" \
            -drive "file=$IMG,format=raw,if=virtio,cache=none" \
            >/dev/null 2>&1
        rc=$?
        set -e

        echo "--- serial tail [$distro] ---"
        tail -25 "$log" 2>/dev/null | tr -d '\r' | sed 's/\x1b\[[0-9;]*[a-zA-Z]//g' | grep -aE '.{3,}' | head -25 \
            || echo "(no serial output captured)"
        echo "---"

        if grep -qE "$EXPECT" "$log" 2>/dev/null; then
            matched=$(grep -oE "$EXPECT" "$log" | head -1)
            echo "  PASS [$distro]: matched '$matched' (qemu exit=$rc)"
            pass_count=$((pass_count + 1))
        else
            echo "  FAIL [$distro]: none of /$EXPECT/ within ${TIMEOUT}s (qemu exit=$rc)"
            fail=1
        fi
    done

    echo ""
    echo "============================================================"
    echo "Multi-ISO results: $pass_count / 3 distros booted"
    echo "============================================================"
else
    case "$FIRMWARE" in
        bios) qemu_run bios || fail=1;;
        uefi) qemu_run uefi || fail=1;;
        both)
            qemu_run bios || fail=1
            qemu_run uefi || fail=1
            ;;
        *) echo "unknown --firmware: $FIRMWARE" >&2; exit 2;;
    esac
fi

if [[ $fail -ne 0 ]]; then
    echo ""
    echo "  *** E2E TEST FAILED ***"
    exit 1
fi

echo ""
echo "  E2E test passed ($MODE)"
