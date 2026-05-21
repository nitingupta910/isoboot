use anyhow::{anyhow, bail, Context, Result};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Deserialize)]
struct LsblkOutput {
    blockdevices: Vec<BlockDevice>,
}

#[derive(Debug, Deserialize)]
pub struct BlockDevice {
    pub name: String,
    pub size: Option<String>,
    pub model: Option<String>,
    pub vendor: Option<String>,
    pub serial: Option<String>,
    pub tran: Option<String>,
    pub rm: Option<bool>,
    #[serde(rename = "type")]
    pub kind: Option<String>,
}

pub fn enumerate() -> Result<Vec<BlockDevice>> {
    let out = Command::new("lsblk")
        .args([
            "-d",
            "-J",
            "-o",
            "NAME,SIZE,MODEL,VENDOR,SERIAL,TRAN,RM,TYPE",
        ])
        .output()
        .context("running lsblk")?;
    if !out.status.success() {
        bail!("lsblk failed: {}", String::from_utf8_lossy(&out.stderr));
    }
    let parsed: LsblkOutput = serde_json::from_slice(&out.stdout).context("parsing lsblk JSON")?;
    Ok(parsed.blockdevices)
}

pub fn list_candidates() -> Result<()> {
    let devs = enumerate()?;
    println!(
        "{:<14} {:<8} {:<4} {:<6} {:<22} SERIAL",
        "DEVICE", "SIZE", "RM", "TRAN", "MODEL"
    );
    for d in devs.iter().filter(|d| d.kind.as_deref() == Some("disk")) {
        println!(
            "{:<14} {:<8} {:<4} {:<6} {:<22} {}",
            format!("/dev/{}", d.name),
            d.size.as_deref().unwrap_or("?"),
            d.rm.map(|b| if b { "yes" } else { "no" }).unwrap_or("?"),
            d.tran.as_deref().unwrap_or("?"),
            d.model.as_deref().unwrap_or("").trim(),
            d.serial.as_deref().unwrap_or("").trim(),
        );
    }
    println!("\nRM=yes means the device is reported as removable by the kernel.");
    println!("isoboot install will refuse RM=no devices unless --allow-internal is given.");
    Ok(())
}

pub fn lookup<'a>(devs: &'a [BlockDevice], path: &Path) -> Result<&'a BlockDevice> {
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow!("invalid device path: {}", path.display()))?;
    devs.iter()
        .find(|d| d.name == name)
        .ok_or_else(|| anyhow!("device {} not found in lsblk output", path.display()))
}

/// Build the partition device path for a whole-disk device.
/// `/dev/sdb` + 1 → `/dev/sdb1`; `/dev/nvme0n1` + 1 → `/dev/nvme0n1p1`.
pub fn partition_path(device: &Path, n: u32) -> PathBuf {
    let s = device.to_string_lossy().into_owned();
    let needs_p = s
        .chars()
        .last()
        .map(|c| c.is_ascii_digit())
        .unwrap_or(false);
    if needs_p {
        PathBuf::from(format!("{}p{}", s, n))
    } else {
        PathBuf::from(format!("{}{}", s, n))
    }
}

#[cfg(test)]
mod tests {
    use super::partition_path;
    use std::path::Path;

    #[test]
    fn sata_style() {
        assert_eq!(
            partition_path(Path::new("/dev/sdb"), 2).to_str(),
            Some("/dev/sdb2")
        );
    }

    #[test]
    fn nvme_style() {
        assert_eq!(
            partition_path(Path::new("/dev/nvme0n1"), 3).to_str(),
            Some("/dev/nvme0n1p3")
        );
    }

    #[test]
    fn mmc_style() {
        assert_eq!(
            partition_path(Path::new("/dev/mmcblk0"), 1).to_str(),
            Some("/dev/mmcblk0p1")
        );
    }
}
