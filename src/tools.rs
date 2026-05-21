use anyhow::{bail, Result};
use std::path::PathBuf;

const REQUIRED: &[&str] = &[
    "lsblk",
    "parted",
    "wipefs",
    "blkid",
    "udevadm",
    "mount",
    "umount",
    "mkfs.vfat",
    "grub-install",
];

pub fn check_required(fs_mkfs: &str) -> Result<()> {
    let mut missing = Vec::new();
    for tool in REQUIRED.iter().chain(std::iter::once(&fs_mkfs)) {
        if which(tool).is_none() {
            missing.push((*tool).to_string());
        }
    }
    if !missing.is_empty() {
        bail!(
            "missing required system tools: {}\n\
             install them via your distro's package manager \
             (typically: grub2, parted, dosfstools, e2fsprogs or exfatprogs, util-linux)",
            missing.join(", ")
        );
    }
    Ok(())
}

fn which(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}
