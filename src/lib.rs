/// Handles command line options, getting the deployment configuration,
/// and calling `deploy::process_node`.
mod deploy;
mod nix;
mod ssh;
mod util;

use anyhow::{anyhow, Context, Result};
use serde::Deserialize;
use std::{collections::BTreeMap, ffi::OsString, path::PathBuf, sync::Arc};
use structopt::StructOpt;
use tracing::info;

#[derive(Deserialize)]
struct DeployCfg {
    // (name, config)
    pub nodes: BTreeMap<String, NodeCfg>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeCfg {
    pub location: String,
    pub ssh_port: Option<u16>,
}

#[derive(StructOpt, Debug)]
#[structopt(name = "henix")]
struct Opts {
    #[structopt(parse(from_os_str), long, env = "HENIX_CFG_DIR")]
    /// Specifies the path to the directory containing the configuration.
    cfg_dir: Option<PathBuf>,
    #[structopt(subcommand)]
    cmd: OptCmd,
}

#[derive(StructOpt, Debug)]
enum OptCmd {
    /// Deploy nodes.
    Deploy(DeployOpts),
}

#[derive(StructOpt, Debug)]
pub struct DeployOpts {
    #[structopt(long)]
    /// Makes the rebuild only restart at boot, equivalent to `nixos-rebuild boot`.
    boot: bool,

    #[structopt(short, long = "target")]
    /// Specifies which targets to deploy to. If a non-present target is specified, an error will
    /// be thrown.
    targets: Option<Vec<String>>,

    #[structopt(long)]
    /// Passes `--show-trace` to `nixos-rebuild`.
    show_trace: bool,
}

pub async fn run<Args: Iterator<Item = OsString>>(args: Args) -> Result<()> {
    // Get the command line arguments.
    let opts = Opts::from_iter(args);

    match opts.cmd {
        OptCmd::Deploy(dep_opts) => {
            let cfg_dir = opts
                .cfg_dir
                .unwrap_or_else(|| std::env::current_dir().unwrap());
            info!("Gathering deploy information");
            let deploy_cfg: DeployCfg = nix::eval(&cfg_dir, ".#deploy")
                .await
                .context("Could not get deploy configuration")?;
            let dep_opts = Arc::new(dep_opts);
            let cfg_dir = Arc::new(cfg_dir);
            // Check if all targets exist
            if let Some(targets) = dep_opts.targets.as_ref() {
                for target in targets {
                    if deploy_cfg.nodes.get(target).is_none() {
                        return Err(anyhow!("Node name `{}` (specified using --target) does not exist. Did you remember to `git add` its configuration?", target));
                    }
                }
            }
            // Join all node deployments.
            futures::future::join_all(deploy_cfg.nodes.into_iter().map(|(name, node_cfg)| async {
                let name = name; // move `name`
                let dep_opts = dep_opts.clone();
                let cfg_dir = cfg_dir.clone();
                // If the user-specified `dep_opts.targets` exists, check if the node is specified
                // in it.
                // Otherwise, just allow it through.
                if dep_opts
                    .targets
                    .as_ref()
                    .map_or(true, |targets| targets.iter().any(|t| t == &name))
                {
                    deploy::process_node(&dep_opts, &name, node_cfg, &cfg_dir).await;
                }
            }))
            .await;
            Ok(())
        }
    }
}

