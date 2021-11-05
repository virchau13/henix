use anyhow::{Context, Result};
use std::process::Stdio;
use tokio::io::AsyncBufReadExt;
use tokio::io::BufReader;
use tokio::process;
use tracing::{info, warn};

/// This proxies the output of a Tokio command (`tokio::process::Command`)
/// to the tracing logger, line-by-line.
/// The child's stdout and stderr are both sent to `info!`.
/// This is extremely similar to `ssh::proxy_output_to_logging`,
/// but must be redone because `openssh::Command` and `tokio::process::Command`
/// don't share a trait for this.
#[tracing::instrument(name = "exec", skip(cmd))]
pub async fn proxy_output_to_logging(
    program: &str,
    mut cmd: process::Command,
) -> Result<std::process::ExitStatus> {
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("Could not spawn process")?;

    let stdout;
    if let Some(child_stdout) = child.stdout.take() {
        stdout = BufReader::new(child_stdout);
    } else {
        warn!("Could not take child stdout, stdout will not be logged");
        return child
            .wait()
            .await
            .context("Could not wait for child to finish");
    }
    let stderr;
    if let Some(child_stderr) = child.stderr.take() {
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
