use anyhow::{anyhow, Context, Result};
use futures::Future;
use serde::{de::DeserializeOwned, Deserialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
use structopt::StructOpt;
use tokio::process;
use tracing::{error, info};

// https://stackoverflow.com/questions/63434977/how-can-i-spawn-asynchronous-methods-in-a-loop
async fn join_parallel<T: Send + 'static>(
    futs: impl IntoIterator<Item = impl Future<Output = T> + Send + 'static>,
) -> Vec<T> {
    let tasks: Vec<_> = futs.into_iter().map(tokio::spawn).collect();
    // unwrap the Result because it is introduced by tokio::spawn()
    // and isn't something our caller can handle
    futures::future::join_all(tasks)
        .await
        .into_iter()
        .map(Result::unwrap)
        .collect()
}

#[derive(Deserialize)]
struct DeployCfg {
    // (name, config)
    pub nodes: BTreeMap<String, NodeCfg>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct NodeCfg {
    pub location: String,
    pub rollback_on_failure: bool,
    pub rollback_on_unreachable: bool,
}

async fn nix_eval<Schema: DeserializeOwned>(cfg_dir: &Path, arg: &str) -> anyhow::Result<Schema> {
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
            "Could not execute `nix eval {}` command:\n{}",
            arg,
            &String::from_utf8_lossy(&out.stderr)
        )));
    }
    serde_json::from_slice(&out.stdout).context(format!("`{}` does not match JSON schema", arg))
}

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
        .status()
        .await
        .context("Could not invoke mv to restore /etc/nixos.old to /etc/nixos")?;
    if !mv.success() {
        return Err(anyhow!(format!(
                    "Could not move /etc/nixos.old back to /etc/nixos (mv exited with {}), you will have to do it manually",
                    mv.code()
                        .map(|x| i32::to_string(&x))
                        .unwrap_or("<unknown>".to_owned())
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
    remote: &mut openssh::Session,
    node_name: &str,
    rollback_on_failure: bool,
) -> Result<()> {
    info!("Building config on remote");
    let rebuild = remote
        .command("nixos-rebuild")
        .arg("switch")
        .arg("--flake")
        .arg(format!("/etc/nixos#{}", node_name)) // FIXME this doesn't escape quotes in the name.
        .output()
        .await
        .context("Could not execute nixos-rebuild on remote")?;
    if !rebuild.status.success() {
        error!(
            "Rebuild failed with stderr:\n{}",
            String::from_utf8_lossy(&rebuild.stderr)
        );
        if rollback_on_failure {
            return rollback(remote).await;
        }
    } else {
        info!("Finished building config on remote");
    }
    Ok(())
}

async fn connect_to_node(node_name: &str, node_cfg: &NodeCfg) -> Result<openssh::Session> {
    info!("Establishing SSH session");
    let remote = openssh::Session::connect(
        format!("root@{}", &node_cfg.location),
        openssh::KnownHosts::Add,
    )
    .await
    .context(format!(
        "Could not connect to node with name `{}`",
        node_name
    ))?;
    info!("SSH session established");
    Ok(remote)
}

async fn process_node_raw(
    remote: &mut openssh::Session,
    name: &str,
    node_cfg: &NodeCfg,
    cfg_dir: &Path,
) -> Result<()> {
    copy_config(remote, &node_cfg.location, &cfg_dir)
        .await
        .context("Could not copy config")?;
    build_config(remote, &name, node_cfg.rollback_on_failure)
        .await
        .context("Could not build config")?;
    if node_cfg.rollback_on_unreachable {
        // dead man's switch
        todo!()
    }
    Ok(())
}

#[tracing::instrument(skip(node_cfg, cfg_dir))]
async fn process_node(name: String, node_cfg: NodeCfg, cfg_dir: PathBuf) {
    let mut remote;
    match connect_to_node(&name, &node_cfg).await {
        Ok(r) => remote = r,
        Err(e) => {
            error!("{:?}", e);
            return;
        }
    }
    if let Err(e) = process_node_raw(&mut remote, &name, &node_cfg, &cfg_dir).await {
        error!("{:?}", e);
        if node_cfg.rollback_on_failure {
            if let Err(e) = rollback(&mut remote).await {
                error!("Error while rolling back: \n{:?}", e);
            }
        }
    }
}

#[derive(StructOpt, Debug)]
#[structopt(name = "henix")]
struct Opts {
    #[structopt(parse(from_os_str), long, env)]
    cfg_dir: Option<PathBuf>,
    #[structopt(subcommand)]
    cmd: OptCmd,
}

#[derive(StructOpt, Debug)]
enum OptCmd {
    Deploy(DeployOpts),
}

#[derive(StructOpt, Debug)]
struct DeployOpts {
    #[structopt(name = "rollback-on-failure", long)]
    /// Overrides the `rollbackOnFailure` value in the configuration.
    force_rollback_on_failure: Option<bool>,

    #[structopt(long)]
    /// Makes the rebuild only restart at boot, equivalent to `nixos-rebuild boot`.
    boot: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    std::env::set_var("RUST_LOG", "INFO");
    tracing_subscriber::fmt::init();

    let opts = Opts::from_args();
    match opts.cmd {
        OptCmd::Deploy {
            force_rollback_on_failure,
            boot,
        } => {
            let cfg_dir = opts.cfg_dir.unwrap_or(std::env::current_dir().unwrap());
            info!("Gathering deploy information");
            let mut deploy_cfg: DeployCfg = nix_eval(&cfg_dir, ".#deploy")
                .await
                .context("Could not get deploy configuration")?;
            if let Some(forced_rollback) = force_rollback_on_failure {
                for (_, node_cfg) in &mut deploy_cfg.nodes {
                    node_cfg.rollback_on_failure = forced_rollback;
                }
            }
            join_parallel(
                deploy_cfg
                    .nodes
                    .into_iter()
                    .map(|(name, node_cfg)| process_node(name, node_cfg, cfg_dir.clone())),
            )
            .await;
            Ok(())
        }
    }
}
