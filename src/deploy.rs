/// Does the actual deployment.
use crate::{ssh, DeployOpts, NodeCfg};
use anyhow::{anyhow, Context, Result};
use std::{path::Path, process::Stdio};
use tokio::process;
use tracing::{error, info};

async fn rollback(remote: &mut openssh::Session) -> Result<()> {
    info!("Rolling back");
    let rm = remote
        .command("rm")
        .arg("-r")
        .arg("/etc/nixos")
        .status()
        .await
        .context("Could not invoke rm to remove new /etc/nixos")?;
    if !rm.success() {
        return Err(anyhow!(format!(
            "Could not remove new /etc/nixos (rm exited with {}), you will have to do it manually",
            rm.code()
                .map(|x| i32::to_string(&x))
                .unwrap_or("<unknown>".to_owned())
        )));
    }
    let mv = remote
        .command("mv")
        .arg("/etc/nixos.old")
        .arg("/etc/nixos")
        .output()
        .await
        .context("Could not invoke mv to restore /etc/nixos.old to /etc/nixos")?;
    if !mv.status.success() {
        return Err(anyhow!(format!(
                    "Could not move /etc/nixos.old back to /etc/nixos (mv stderr: {}), you will have to do it manually",
                    String::from_utf8_lossy(&mv.stderr)
                )));
    }
    info!("Rollback complete");
    Ok(())
}

async fn copy_config(
    remote: &mut openssh::Session,
    node_location: &str,
    cfg_dir: &Path,
) -> Result<()> {
    info!("Copying files");
    info!("Moving the remote's /etc/nixos to /etc/nixos.old");
    // Move the old `/etc/nixos` out of the way first.
    let mv = remote
        .command("mv")
        .arg("/etc/nixos")
        .arg("/etc/nixos.old")
        .stderr(Stdio::inherit())
        .status()
        .await
        .context("Could not invoke mv to move old /etc/nixos out of the way")?;
    if !mv.success() {
        return Err(anyhow!(format!(
            "Could not move old /etc/nixos out of the way (mv exited with {}), aborting",
            mv.code()
                .map(|x| i32::to_string(&x))
                .unwrap_or("<unknown>".to_owned())
        )));
    }
    info!("Using rsync to copy config");
    // We need to add a slash after `cfg_dir`,
    // so that rsync copies the *contents* of the directory,
    // rather than the directory itself.
    let mut cfg_dir_with_slash = cfg_dir.to_owned();
    cfg_dir_with_slash.push("");
    let out = process::Command::new("rsync")
        .arg("--exclude=.git/")
        .arg("-a") // Archive mode, preserve symlinks, permissions, devices, etc.
        .arg("-F") // Allow `.rsync-filter` files to be used
        .arg("--delete") // Remove files on the remote not present locally
        .arg("-e")
        .arg("ssh") // Use ssh (rsync might have been configured differently)
        .arg(cfg_dir_with_slash) // Copy the contents of the current directory...
        .arg(format!("root@{}:/etc/nixos", node_location)) // to `/etc/nixos` on the remote
        .output()
        .await
        .context("Could not execute rsync to copy files")?;
    if !out.status.success() {
        return Err(anyhow!(format!(
            "Could not rsync files to location `{}` (rsync exited with {}), with stderr of:\n{}",
            out.status
                .code()
                .map(|x| i32::to_string(&x))
                .unwrap_or("<unknown>".to_owned()),
            node_location,
            String::from_utf8_lossy(&out.stderr)
        )));
    }
    info!("Copying finished");
    Ok(())
}

async fn build_config(
    dep_opts: &DeployOpts,
    remote: &mut openssh::Session,
    node_name: &str,
) -> Result<()> {
    
    info!("Building config on remote");
    let mut rebuild = remote
        .command("nixos-rebuild");
    rebuild
        .arg(if dep_opts.boot { "boot" } else { "switch" })
        .arg("--flake")
        .arg(format!("/etc/nixos#{}", node_name)); // FIXME this doesn't escape quotes in the name.

    let rebuild = ssh::proxy_output_to_logging("nixos-rebuild", rebuild).await.context("Rebuild execution failed")?;
    if !rebuild.success() {
        return Err(anyhow!("Rebuild failed"));
    } else {
        info!("Finished building config on remote");
    }
    Ok(())
}

async fn process_node_raw(
    dep_opts: &DeployOpts,
    remote: &mut openssh::Session,
    name: &str,
    node_cfg: &NodeCfg,
    cfg_dir: &Path,
) -> Result<()> {
    copy_config(remote, &node_cfg.location, cfg_dir)
        .await
        .context("Could not copy config")?;
    build_config(dep_opts, remote, &name)
        .await
        .context("Could not build config")?;
    // if let Err(e) = build_res {
    //     // If the forced rollback option is set, use it,
    //     // else use the rollback_on_failure configuration.
    //     if dep_opts.force_rollback_on_failure.unwrap_or(node_cfg.rollback_on_failure) {
    //         rollback(remote).await.context("Could not rollback")?;
    //     }
    //     return Err(e);
    // }
    // if node_cfg.rollback_on_unreachable {
    //     // dead man's switch
    //     todo!()
    // }
    Ok(())
}

// This handles the errors and the logging, `process_node_raw` does the actual work.
#[tracing::instrument(skip(dep_opts, node_cfg, cfg_dir))]
pub async fn process_node(dep_opts: &DeployOpts, name: &str, node_cfg: NodeCfg, cfg_dir: &Path) {
    let mut remote;
    match ssh::connect_to_node(&name, &node_cfg).await {
        Ok(r) => remote = r,
        Err(e) => {
            error!("{:?}", e);
            return;
        }
    }
    if let Err(e) = process_node_raw(dep_opts, &mut remote, name, &node_cfg, &cfg_dir).await {
        error!("{:?}", e);
        if node_cfg.rollback_on_failure {
            if let Err(e) = rollback(&mut remote).await {
                error!("Error while rolling back: \n{:?}", e);
            }
        }
    }
}
