use anyhow::{bail, Context, Result};
use std::process::Command;

pub struct Runner {
    pub verbose: bool,
    pub dry_run: bool,
}

impl Runner {
    pub fn new(verbose: bool, dry_run: bool) -> Self {
        Self { verbose, dry_run }
    }

    pub fn run(&self, cmd: &str, args: &[&str]) -> Result<()> {
        if self.verbose || self.dry_run {
            eprintln!("$ {} {}", cmd, args.join(" "));
        }
        if self.dry_run {
            return Ok(());
        }
        let status = Command::new(cmd)
            .args(args)
            .status()
            .with_context(|| format!("spawning `{}`", cmd))?;
        if !status.success() {
            bail!("`{}` failed (exit {:?})", cmd, status.code());
        }
        Ok(())
    }

    pub fn run_capture(&self, cmd: &str, args: &[&str]) -> Result<String> {
        if self.verbose {
            eprintln!("$ {} {}", cmd, args.join(" "));
        }
        let out = Command::new(cmd)
            .args(args)
            .output()
            .with_context(|| format!("spawning `{}`", cmd))?;
        if !out.status.success() {
            bail!(
                "`{}` failed: {}",
                cmd,
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }
}
