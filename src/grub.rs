use crate::runner::Runner;
use anyhow::{Context, Result};
use std::path::Path;

const GRUB_CFG: &str = include_str!("../assets/grub.cfg");

const MARKER_BODY: &str =
    "This file marks the isoboot data partition.\n\
     Drop Linux distro *.iso files into this directory and reboot.\n";

pub fn install_grub_efi(runner: &Runner, efi_mp: &Path) -> Result<()> {
    let efi_arg = format!("--efi-directory={}", efi_mp.display());
    let boot_arg = format!("--boot-directory={}", efi_mp.join("boot").display());
    runner.run(
        "grub-install",
        &[
            "--target=x86_64-efi",
            "--removable",
            "--no-nvram",
            "--recheck",
            &efi_arg,
            &boot_arg,
        ],
    )?;
    Ok(())
}

pub fn install_grub_bios(runner: &Runner, efi_mp: &Path, device: &Path) -> Result<()> {
    let boot_arg = format!("--boot-directory={}", efi_mp.join("boot").display());
    let dev_str = device.to_string_lossy().into_owned();
    runner.run(
        "grub-install",
        &[
            "--target=i386-pc",
            "--recheck",
            &boot_arg,
            &dev_str,
        ],
    )?;
    Ok(())
}

pub fn write_grub_cfg(runner: &Runner, efi_mp: &Path) -> Result<()> {
    let grub_dir = efi_mp.join("boot/grub");
    let cfg_path = grub_dir.join("grub.cfg");
    if runner.dry_run {
        eprintln!("$ write {} ({} bytes)", cfg_path.display(), GRUB_CFG.len());
        return Ok(());
    }
    std::fs::create_dir_all(&grub_dir)
        .with_context(|| format!("creating {}", grub_dir.display()))?;
    std::fs::write(&cfg_path, GRUB_CFG)
        .with_context(|| format!("writing {}", cfg_path.display()))?;
    Ok(())
}

pub fn write_data_marker(runner: &Runner, data_mp: &Path) -> Result<()> {
    let isos_dir = data_mp.join("isos");
    let marker = isos_dir.join(".isoboot");
    if runner.dry_run {
        eprintln!("$ mkdir -p {}", isos_dir.display());
        eprintln!("$ write {}", marker.display());
        return Ok(());
    }
    std::fs::create_dir_all(&isos_dir)
        .with_context(|| format!("creating {}", isos_dir.display()))?;
    std::fs::write(&marker, MARKER_BODY)
        .with_context(|| format!("writing {}", marker.display()))?;
    Ok(())
}
