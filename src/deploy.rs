/// Does the actual deployment.
use crate::{nix, ssh, DeployOpts, NodeCfg};
use anyhow::{anyhow, Context, Result};
use std::path::Path;
use tokio::process;
use tracing::{error, info, warn};

async fn copy_config(node_location: &str, cfg_dir: &Path, cfg_hash: &str) -> Result<()> {
    info!("Copying files");
    info!("Using rsync to copy config");
    // We need to add a slash after `cfg_dir`,
    // so that rsync copies the *contents* of the directory,
    // rather than the directory itself.
    let mut cfg_dir_with_slash = cfg_dir.to_owned();
    cfg_dir_with_slash.push("");
    let rsync = process::Command::new("rsync")
        .arg("--exclude=.git/")
        .arg("-a") // Archive mode, preserve symlinks, permissions, devices, etc.
        .arg("-F") // Allow `.rsync-filter` files to be used
        .arg("--delete") // Remove files on the remote not present locally
        .arg("--mkpath") // Equivalent of `mkdir -p` on the remote path
        .arg("-e")
        .arg("ssh") // Use ssh (rsync might have been configured differently)
        .arg(cfg_dir_with_slash) // Copy the contents of the current directory...
        .arg(format!("root@{}:/etc/henix/{}", node_location, cfg_hash)) // to `/etc/henix/{hash}` on the remote
        .output()
        .await
        .context("Could not execute rsync to copy files")?;
    if !rsync.status.success() {
        return Err(anyhow!(format!(
            "Could not rsync files to location `{}` (rsync exited with {}), with stderr of:\n{}",
            rsync
                .status
                .code()
                .map_or_else(|| "<unknown>".to_owned(), |x| i32::to_string(&x)),
            node_location,
            String::from_utf8_lossy(&rsync.stderr)
        )));
    }
    info!("Copying finished");
    Ok(())
}

async fn build_config(
    dep_opts: &DeployOpts,
    remote: &mut openssh::Session,
    node_name: &str,
    cfg_hash: &str,
) -> Result<()> {
    info!("Building config on remote");
    let mut rebuild = remote.command("nixos-rebuild");
    rebuild
        .arg(if dep_opts.boot { "boot" } else { "switch" })
        .arg("--flake")
        .arg(format!("/etc/henix/{}#{}", cfg_hash, node_name)); // FIXME this doesn't escape quotes in the name.
    if dep_opts.show_trace {
        rebuild.arg("--show-trace");
    }
    let rebuild = ssh::proxy_output_to_logging("nixos-rebuild", rebuild)
        .await
        .context("Rebuild execution failed")?;
    if !rebuild.success() {
        return Err(anyhow!("Rebuild failed"));
    }
    info!("Finished building config on remote");
    Ok(())
}

/// Does the actual deployment, doesn't rollback on failure.
async fn process_node_raw(
    dep_opts: &DeployOpts,
    remote: &mut openssh::Session,
    name: &str,
    node_cfg: &NodeCfg,
    cfg_dir: &Path,
) -> Result<()> {
    let cfg_hash = nix::hash(cfg_dir).await.context("Could not get hash")?;
    info!("Configuration hash is {}", cfg_hash);
    copy_config(&node_cfg.location, cfg_dir, &cfg_hash)
        .await
        .context("Could not copy config")?;
    build_config(dep_opts, remote, name, &cfg_hash)
        .await
        .context("Could not build config")?;
    // Link the latest config
    let link_res = remote
        .command("ln")
        .arg("-s")
        .arg("-f") // Overwite existing destination files
        .arg(format!("/etc/henix/{}", cfg_hash))
        .arg("/etc/henix/latest")
        .status()
        .await;
    if let Ok(link_status) = link_res {
        if link_status.success() {
            return Ok(());
        }
    }
    warn!("Could not symlink /etc/henix/latest to /etc/henix/{hash}. This is more for convenience, but you may not be able to easily find the current configuration if it is not symlinked. Recommended command: ln -s -f /etc/henix/{hash} /etc/henix/latest", hash = cfg_hash);
    Ok(())
}

/// Handles the errors, logging, and rollback; `process_node_raw` does the actual deployment.
#[tracing::instrument(skip(dep_opts, node_cfg, cfg_dir))]
pub async fn process_node(dep_opts: &DeployOpts, name: &str, node_cfg: NodeCfg, cfg_dir: &Path) {
    let mut remote;
    match ssh::connect_to_node(name, &node_cfg).await {
        Ok(r) => remote = r,
        Err(e) => {
            error!("{:?}", e);
            return;
        }
    }
    if let Err(e) = process_node_raw(dep_opts, &mut remote, name, &node_cfg, cfg_dir).await {
        error!("Did not deploy configuration: {:?}", e);
    }
}
