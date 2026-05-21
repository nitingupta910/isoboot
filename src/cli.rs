use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "isoboot", version, about = "Multi-boot USB for Linux ISOs")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// List candidate USB devices on this system.
    List,
    /// Wipe a USB and install isoboot. ALL DATA ON THE DEVICE IS DESTROYED.
    Install(InstallArgs),
    /// Refresh the GRUB menu on an existing isoboot USB without reformatting.
    UpdateMenu(DeviceArgs),
    /// Sanity-check an existing isoboot USB.
    Verify(DeviceArgs),
}

#[derive(Args)]
pub struct InstallArgs {
    /// Target block device (e.g. /dev/sdb). Pass the whole disk, not a partition.
    #[arg(long)]
    pub device: PathBuf,

    /// Filesystem for the data partition where ISOs are dropped.
    #[arg(long, value_enum, default_value_t = Fs::Ext4)]
    pub filesystem: Fs,

    /// Volume label for the data partition.
    #[arg(long, default_value = "ISOBOOT")]
    pub label: String,

    /// Size of the ESP in MiB. 128 MiB fits GRUB modules with plenty of room.
    #[arg(long, default_value_t = 128)]
    pub esp_mib: u64,

    /// Allow installing to a non-removable disk (DANGEROUS).
    #[arg(long)]
    pub allow_internal: bool,

    /// Skip the interactive "type the device path" confirmation prompt.
    #[arg(long, short = 'y')]
    pub yes: bool,

    /// Print every shell command that would be executed without running it.
    #[arg(long)]
    pub dry_run: bool,

    /// Echo every shell command before running.
    #[arg(long, short = 'v')]
    pub verbose: bool,
}

#[derive(Args)]
pub struct DeviceArgs {
    /// Target block device (e.g. /dev/sdb).
    #[arg(long)]
    pub device: PathBuf,

    #[arg(long, short = 'v')]
    pub verbose: bool,
}

#[derive(Copy, Clone, Debug, ValueEnum)]
pub enum Fs {
    Ext4,
    Exfat,
}

impl Fs {
    pub fn mkfs(&self) -> &'static str {
        match self {
            Fs::Ext4 => "mkfs.ext4",
            Fs::Exfat => "mkfs.exfat",
        }
    }

    pub fn mkfs_args<'a>(&self, label: &'a str) -> Vec<&'a str> {
        match self {
            Fs::Ext4 => vec!["-F", "-L", label],
            Fs::Exfat => vec!["-L", label],
        }
    }
}
