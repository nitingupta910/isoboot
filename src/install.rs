use crate::cli::{DeviceArgs, InstallArgs};
use crate::disk;
use crate::grub;
use crate::runner::Runner;
use crate::tools;
use anyhow::{bail, Context, Result};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

extern "C" {
    fn geteuid() -> u32;
}

fn is_root() -> bool {
    unsafe { geteuid() == 0 }
}

fn require_root() -> Result<()> {
    if !is_root() {
        bail!("isoboot needs root privileges (try `sudo isoboot ...`)");
    }
    Ok(())
}

pub fn install(args: &InstallArgs) -> Result<()> {
    tools::check_required(args.filesystem.mkfs())?;
    if !args.dry_run {
        require_root()?;
    }

    let devs = disk::enumerate()?;
    let dev = disk::lookup(&devs, &args.device)?;

    let is_loop = dev.kind.as_deref() == Some("loop");
    match dev.kind.as_deref() {
        Some("disk") => {}
        Some("loop") => {
            eprintln!("note: {} is a loop device (test/QEMU image install)", args.device.display());
        }
        other => bail!("{} has unexpected lsblk type {:?}", args.device.display(), other),
    }
    let removable = dev.rm.unwrap_or(false) || is_loop;
    if !removable && !args.allow_internal {
        bail!(
            "{} is not removable — refusing.\n\
             Pass --allow-internal only if you're absolutely certain.",
            args.device.display()
        );
    }

    print_target_summary(dev, args);

    if !args.yes && !args.dry_run {
        confirm(&args.device)?;
    }

    let part_esp = disk::partition_path(&args.device, 2);
    let part_data = disk::partition_path(&args.device, 3);

    let runner = Runner::new(args.verbose, args.dry_run);

    // Best-effort: unmount anything currently mounted from this disk.
    let _ = unmount_existing(&args.device, &runner);

    let dev_str = args.device.to_string_lossy().into_owned();
    let esp_end = format!("{}MiB", 3 + args.esp_mib);

    runner.run("wipefs", &["-a", &dev_str])?;
    runner.run("parted", &["-s", &dev_str, "mklabel", "gpt"])?;
    runner.run("parted", &["-s", &dev_str, "mkpart", "bios_boot", "1MiB", "3MiB"])?;
    runner.run("parted", &["-s", &dev_str, "set", "1", "bios_grub", "on"])?;
    runner.run(
        "parted",
        &["-s", &dev_str, "mkpart", "ESP", "fat32", "3MiB", &esp_end],
    )?;
    runner.run("parted", &["-s", &dev_str, "set", "2", "esp", "on"])?;
    runner.run(
        "parted",
        &["-s", &dev_str, "mkpart", "DATA", &esp_end, "100%"],
    )?;
    runner.run("udevadm", &["settle"])?;

    runner.run(
        "mkfs.vfat",
        &["-F", "32", "-n", "ISOBOOT-EFI", &part_esp.to_string_lossy()],
    )?;
    let part_data_str = part_data.to_string_lossy().into_owned();
    let mut mkfs_args = args.filesystem.mkfs_args(&args.label);
    mkfs_args.push(&part_data_str);
    runner.run(args.filesystem.mkfs(), &mkfs_args)?;

    runner.run("udevadm", &["settle"])?;

    if args.dry_run {
        eprintln!("(dry-run) would mount, install grub, write grub.cfg, then unmount");
        return Ok(());
    }

    let mounts = TempMounts::new(&runner, &part_esp, &part_data)?;
    grub::install_grub_efi(&runner, &mounts.efi)?;
    grub::install_grub_bios(&runner, &mounts.efi, &args.device)?;
    grub::write_grub_cfg(&runner, &mounts.efi)?;
    grub::write_data_marker(&runner, &mounts.data)?;
    drop(mounts);

    eprintln!(
        "\n  isoboot installed on {}\n  Copy *.iso files into /isos/ on the data partition and boot.\n",
        args.device.display()
    );
    Ok(())
}

pub fn update_menu(args: &DeviceArgs) -> Result<()> {
    tools::check_required("mkfs.ext4")?;
    require_root()?;
    let part_esp = disk::partition_path(&args.device, 2);
    let runner = Runner::new(args.verbose, false);

    let mp = mktemp("isoboot-esp")?;
    runner.run("mount", &[&part_esp.to_string_lossy(), &mp.to_string_lossy()])?;
    let result = grub::write_grub_cfg(&runner, &mp);
    let _ = runner.run("umount", &[&mp.to_string_lossy()]);
    let _ = std::fs::remove_dir(&mp);
    result?;
    eprintln!("  GRUB menu refreshed on {}", part_esp.display());
    Ok(())
}

pub fn verify(args: &DeviceArgs) -> Result<()> {
    require_root()?;
    let part_esp = disk::partition_path(&args.device, 2);
    let part_data = disk::partition_path(&args.device, 3);
    let runner = Runner::new(args.verbose, false);

    let mp_esp = mktemp("isoboot-verify-esp")?;
    let mp_data = mktemp("isoboot-verify-data")?;
    runner.run(
        "mount",
        &["-o", "ro", &part_esp.to_string_lossy(), &mp_esp.to_string_lossy()],
    )?;
    runner.run(
        "mount",
        &["-o", "ro", &part_data.to_string_lossy(), &mp_data.to_string_lossy()],
    )?;

    let mut ok = true;
    let cfg = mp_esp.join("boot/grub/grub.cfg");
    if cfg.is_file() {
        println!("  grub.cfg present at {}", cfg.display());
    } else {
        println!("  MISSING {}", cfg.display());
        ok = false;
    }
    let marker = mp_data.join("isos/.isoboot");
    if marker.is_file() {
        println!("  data marker present at {}", marker.display());
    } else {
        println!("  MISSING data marker {}", marker.display());
        ok = false;
    }
    let isos_dir = mp_data.join("isos");
    if isos_dir.is_dir() {
        let count = std::fs::read_dir(&isos_dir)
            .map(|rd| {
                rd.filter_map(|e| e.ok())
                    .filter(|e| {
                        e.path()
                            .extension()
                            .and_then(|s| s.to_str())
                            .map(|s| s.eq_ignore_ascii_case("iso"))
                            .unwrap_or(false)
                    })
                    .count()
            })
            .unwrap_or(0);
        println!("  {} ISO file(s) in /isos/", count);
    }

    let _ = runner.run("umount", &[&mp_esp.to_string_lossy()]);
    let _ = runner.run("umount", &[&mp_data.to_string_lossy()]);
    let _ = std::fs::remove_dir(&mp_esp);
    let _ = std::fs::remove_dir(&mp_data);

    if !ok {
        bail!("verification failed");
    }
    Ok(())
}

fn confirm(device: &Path) -> Result<()> {
    eprint!(
        "\nTo confirm wiping {}, retype the full device path: ",
        device.display()
    );
    io::stderr().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    if input.trim() != device.to_string_lossy() {
        bail!("confirmation mismatch; aborting");
    }
    Ok(())
}

fn print_target_summary(dev: &disk::BlockDevice, args: &InstallArgs) {
    eprintln!("\nTarget: /dev/{}", dev.name);
    eprintln!("  size:       {}", dev.size.as_deref().unwrap_or("?"));
    eprintln!("  model:      {}", dev.model.as_deref().unwrap_or("?").trim());
    eprintln!("  vendor:     {}", dev.vendor.as_deref().unwrap_or("?").trim());
    eprintln!("  serial:     {}", dev.serial.as_deref().unwrap_or("?").trim());
    eprintln!(
        "  removable:  {}",
        dev.rm.map(|b| if b { "yes" } else { "NO" }).unwrap_or("?")
    );
    eprintln!("  transport:  {}", dev.tran.as_deref().unwrap_or("?"));
    eprintln!("  filesystem: {:?}", args.filesystem);
    eprintln!("  esp size:   {} MiB", args.esp_mib);
    eprintln!("  data label: {}", args.label);
    eprintln!("\n  *** ALL EXISTING DATA ON THIS DEVICE WILL BE DESTROYED ***");
}

fn unmount_existing(device: &Path, runner: &Runner) -> Result<()> {
    let out = runner.run_capture(
        "lsblk",
        &["-rno", "MOUNTPOINT", &device.to_string_lossy()],
    )?;
    for line in out.lines() {
        let mp = line.trim();
        if !mp.is_empty() {
            let _ = runner.run("umount", &[mp]);
        }
    }
    Ok(())
}

fn mktemp(prefix: &str) -> Result<PathBuf> {
    let path = std::env::temp_dir().join(format!("{}-{}", prefix, std::process::id()));
    std::fs::create_dir_all(&path)
        .with_context(|| format!("creating {}", path.display()))?;
    Ok(path)
}

struct TempMounts<'a> {
    runner: &'a Runner,
    root: PathBuf,
    efi: PathBuf,
    data: PathBuf,
}

impl<'a> TempMounts<'a> {
    fn new(runner: &'a Runner, esp: &Path, data: &Path) -> Result<Self> {
        let root = mktemp("isoboot-mnt")?;
        let efi = root.join("efi");
        let data_mp = root.join("data");
        std::fs::create_dir_all(&efi)?;
        std::fs::create_dir_all(&data_mp)?;
        runner.run(
            "mount",
            &[&esp.to_string_lossy(), &efi.to_string_lossy()],
        )?;
        runner.run(
            "mount",
            &[&data.to_string_lossy(), &data_mp.to_string_lossy()],
        )?;
        Ok(Self {
            runner,
            root,
            efi,
            data: data_mp,
        })
    }
}

impl<'a> Drop for TempMounts<'a> {
    fn drop(&mut self) {
        let _ = self.runner.run("umount", &[&self.efi.to_string_lossy()]);
        let _ = self.runner.run("umount", &[&self.data.to_string_lossy()]);
        let _ = std::fs::remove_dir(&self.efi);
        let _ = std::fs::remove_dir(&self.data);
        let _ = std::fs::remove_dir(&self.root);
    }
}
