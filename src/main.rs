/// Handles command line options, getting the deployment configuration,
/// and calling `deploy::process_node`.

mod deploy;
mod nix;
mod ssh;

use anyhow::{Context, Result};
use serde::Deserialize;
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};
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
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging.
    {
        let mut env_var_exists = false;
        // If environment var is empty or does not exist, set it to INFO by default.
        if std::env::var("RUST_LOG")
            .map(|x| x.is_empty())
            .unwrap_or(true)
        {
            std::env::set_var("RUST_LOG", "INFO");
        } else {
            env_var_exists = true;
        }
        tracing_subscriber::fmt::init();
        if env_var_exists {
            info!("Picked up $RUST_LOG");
        }
    }

    // Get the command line arguments.
    let opts = Opts::from_args();

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
            // Join all node deployments.
            futures::future::join_all(deploy_cfg.nodes.into_iter().map(|(name, node_cfg)| async {
                let name = name; // move `name`
                let dep_opts = dep_opts.clone();
                let cfg_dir = cfg_dir.clone();
                deploy::process_node(&dep_opts, &name, node_cfg, &cfg_dir).await;
            }))
            .await;
            Ok(())
        }
    }
}
