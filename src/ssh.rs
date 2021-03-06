use std::process::Stdio;

/// SSH utilities.
use crate::NodeCfg;
use anyhow::{Context, Result};
use tokio::io::{AsyncBufReadExt, BufReader};
use tracing::{info, warn};

pub async fn connect_to_node(node_name: &str, node_cfg: &NodeCfg) -> Result<openssh::Session> {
    info!("Establishing SSH session");
    let mut builder = openssh::SessionBuilder::default();
    if let Some(ssh_port) = node_cfg.ssh_port {
        builder.port(ssh_port);
    }
    let remote = builder
        .user("root".to_string())
        .control_directory("/tmp") // Default is "./", which is not nice to nix-hash.
        .connect(&node_cfg.location)
        .await
        .context(format!(
            "Could not connect to node with name `{}`",
            node_name
        ))?;
    info!("SSH session established");
    Ok(remote)
}

/// This proxies the output of an SSH command (`openssh::Command`)
/// to the tracing logger, line-by-line.
/// The child's stdout and stderr are both sent to `info!`.
/// This is extremely similar to `util::proxy_output_to_logging`,
/// but must be redone because `openssh::Command` and `tokio::process::Command`
/// don't share a trait for this.
#[tracing::instrument(name = "ssh_exec", skip(cmd))]
pub async fn proxy_output_to_logging<'a>(
    program: &str,
    mut cmd: openssh::Command<'a>,
) -> Result<std::process::ExitStatus> {
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("Could not spawn process")?;

    let stdout;
    if let Some(child_stdout) = child.stdout().take() {
        stdout = BufReader::new(child_stdout);
    } else {
        warn!("Could not take child stdout, stdout will not be logged");
        return child
            .wait()
            .await
            .context("Could not wait for child to finish");
    }
    let stderr;
    if let Some(child_stderr) = child.stderr().take() {
        stderr = BufReader::new(child_stderr);
    } else {
        warn!("Could not take child stderr, stderr will not be logged");
        return child
            .wait()
            .await
            .context("Could not wait for child to finish");
    }
    let mut stdout_lines = stdout.lines();
    let mut stderr_lines = stderr.lines();

    // While there is still output...
    loop {
        // race both streams
        // and process whichever one returns first.
        tokio::select! {
            Ok(Some(line)) = stdout_lines.next_line() => {
                info!("stdout: {}", line);
            }
            Ok(Some(line)) = stderr_lines.next_line() => {
                info!("stderr: {}", line);
            }
            else => break
        }
    }
    // All lines have been processed, return status.

    child
        .wait()
        .await
        .context("Could not wait for child status")
}
