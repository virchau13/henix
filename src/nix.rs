/// Nix utilities.
use std::path::Path;

use anyhow::{anyhow, Context};
use serde::de::DeserializeOwned;
use tokio::process;

/// Equivalent to `nix eval --json "$arg"`.
pub async fn eval<Schema: DeserializeOwned>(cfg_dir: &Path, arg: &str) -> anyhow::Result<Schema> {
    let out = process::Command::new("nix")
        .current_dir(cfg_dir)
        .arg("eval")
        .arg("--json")
        .arg("--")
        .arg(arg)
        .output()
        .await
        .context("Could not execute nix eval command")?;
    if !out.status.success() {
        return Err(anyhow!(format!(
            "Could not execute `nix eval {}` command, with stderr:\n{}",
            arg,
            &String::from_utf8_lossy(&out.stderr)
        )));
    }
    serde_json::from_slice(&out.stdout).context(format!("`{}` does not match JSON schema", arg))
}

/// Equivalent to `nix-hash "$dir"`.
pub async fn hash(dir: &Path) -> anyhow::Result<String> {
    let out = process::Command::new("nix-hash")
        .arg(dir)
        .output()
        .await
        .context("Could not execute nix-hash command")?;
    if !out.status.success() {
        return Err(anyhow!(format!(
            "Could not execute `nix-hash {}` command, with stderr:\n{}",
            dir.display(),
            &String::from_utf8_lossy(&out.stderr)
        )));
    }
    let hash = String::from_utf8(out.stdout).context("Could not decode nix-hash's output as UTF-8")?;
    Ok(hash.trim().to_string())
}
