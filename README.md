# isoboot

A small Rust CLI that turns a USB drive into a Linux-only multi-boot device.
Drop ISO files into one folder, reboot, pick a distro from the GRUB menu.

Think of it as a deliberately narrow [Ventoy](https://www.ventoy.net/)
alternative that:

- ships **zero pre-built binaries** — uses the host distro's `grub-install`,
  `parted`, `mkfs.*`, `lsblk`
- ships **no custom kernel modules** — uses upstream GRUB's `loopback`
  module plus documented kernel boot parameters (`findiso=`,
  `iso-scan/filename=`, `img_dev=/img_loop=`, `isofrom_device=`, Alpine's
  `modloop=`)
- supports **Linux only** as both the host (the machine you run `isoboot`
  on) and the booted guests
- is **~700 lines of Rust + a 100-line `grub.cfg`** — auditable in one
  sitting

## Install

```sh
cargo install isoboot
```

You also need these system tools available on the host:
`lsblk parted wipefs blkid udevadm mount umount mkfs.vfat grub-install`
plus `mkfs.ext4` (default) or `mkfs.exfat` for the data partition.
Debian/Ubuntu: `apt install grub2-common grub-pc-bin grub-efi-amd64-bin
parted dosfstools e2fsprogs util-linux`.

## Quick start

```sh
# 1. See what removable devices are attached.
isoboot list

# 2. Wipe a USB and install isoboot onto it. ALL DATA ON THE DEVICE IS DESTROYED.
sudo isoboot install --device /dev/sdX

# 3. Mount the data partition (any Linux file manager will auto-mount it as
#    "ISOBOOT") and drop ISO files into /isos/.

cp ~/Downloads/ubuntu-26.04-live-server-amd64.iso /run/media/$USER/ISOBOOT/isos/
cp ~/Downloads/Fedora-Server-netinst-x86_64-43-1.6.iso /run/media/$USER/ISOBOOT/isos/

# 4. Eject, reboot, pick from the GRUB menu.
```

No re-formatting between distros. Add or remove ISOs anytime — the GRUB
menu rebuilds itself dynamically at every boot by enumerating `/isos/*.iso`.

## Do I need to format the USB first?

No. `isoboot install --device /dev/sdX` does everything in one shot: wipes
the existing partition table, lays down GPT, formats the partitions, and
installs GRUB for both BIOS and UEFI. You point it at a raw block device,
not a formatted volume.

## USB layout after install

A single GPT-partitioned disk:

| # | Size       | Type / FS                              | Purpose                     |
| - | ---------- | -------------------------------------- | --------------------------- |
| 1 | 2 MiB      | BIOS boot partition (`bios_grub`)      | GRUB core image for legacy BIOS |
| 2 | 128 MiB    | ESP, FAT32, label `ISOBOOT-EFI`        | GRUB EFI binary + `grub.cfg` |
| 3 | rest       | ext4 (default) or exFAT, label `ISOBOOT` | `/isos/` for your ISO files |

Boots on legacy BIOS *and* UEFI from the same USB.

## How an ISO actually boots

At boot, the menu config (`assets/grub.cfg`) iterates `/isos/*.iso`. For
each ISO it probe-mounts the file via GRUB's `loopback` module and looks
for `boot/grub/loopback.cfg` inside. If present (Ubuntu, Debian, Mint,
Manjaro, Kali, MX, …), one menu entry is emitted that just `configfile`s
the upstream loopback.cfg — the distro handles its own boot args.

For ISOs without a `loopback.cfg`, isoboot emits a small set of
per-family fallback entries using documented kernel params:

- **Debian-live**: `findiso=…`
- **Ubuntu casper**: `iso-scan/filename=…`
- **Fedora / dracut**: `iso-scan/filename=…`
- **openSUSE**: `isofrom_device=…/isofrom_system=…`
- **Arch**: `img_dev=UUID=…/img_loop=…`
- **Alpine virt**: `modloop=…/alpine_dev=loop`

## Why not just use Ventoy?

Ventoy is great and battle-tested. The reason this project exists is its
trust model: Ventoy ships ~100 pre-built binaries in-tree (Windows EXEs,
EFI binaries, kernel modules like `geom_ventoy.ko` and `dm-mod.ko`,
third-party blobs like `imdisk.sys` and `7za.exe`), documented in
`BLOB_List.md`, with build scripts but no routine reproducibility
verification. That's a real supply-chain surface.

isoboot punts on all of that by being Linux-only and refusing to bundle
anything binary. Everything that ends up on your USB comes from your own
distro's signed packages (`grub-install`, `parted`, `mkfs.*`).

Tradeoff: isoboot won't help you install Windows from a USB. If that
matters to you, use Ventoy.

## Safety

- Refuses non-removable devices unless `--allow-internal` is passed
- Refuses partition paths (`/dev/sdb1`) — requires whole-disk path
- Shows model / serial / size / transport before doing anything
- Requires retyping the device path verbatim to confirm (or `--yes`)
- `grub-install --no-nvram --removable` so installing on a UEFI machine
  doesn't pollute that machine's firmware boot entries

## Build from source

```sh
git clone https://github.com/nitingupta910/isoboot
cd isoboot
cargo build --release
./target/release/isoboot --help
```

## End-to-end test (QEMU)

`tests/e2e/run.sh` builds a raw image, installs isoboot onto it via a
loop device, boots the image under QEMU in both BIOS and UEFI modes, and
verifies a sentinel ISO actually boots.

```sh
# Smoke test — hermetic, ~30s, no internet.
sudo tests/e2e/run.sh

# Multi-distro test — downloads Alpine + Ubuntu 26.04 Server +
# Fedora 43 Server, boots each one through UEFI, verifies each boots.
sudo tests/e2e/run.sh --multi
```

A `tests/e2e/install-sudo-helper.sh` script optionally drops a narrow
NOPASSWD sudoers entry so CI / automation can run the test
non-interactively.

## License

MIT.
