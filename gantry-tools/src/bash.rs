use std::process::Command;
use std::time::Duration;

use thiserror::Error;

const DEFAULT_TIMEOUT_MS: u64 = 30_000;

#[derive(Debug, Error)]
pub enum BashError {
    #[error("failed to spawn process: {0}")]
    Spawn(std::io::Error),
    #[error("command timed out after {0}ms")]
    Timeout(u64),
}

/// Runs `command` in a bash shell and returns its combined stdout and stderr output.
///
/// `timeout_ms` caps execution time; defaults to 30 000 ms. If the process exceeds
/// the limit it is killed and `BashError::Timeout` is returned.
pub fn run_bash(command: &str, timeout_ms: Option<u64>) -> Result<String, BashError> {
    let timeout = Duration::from_millis(timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS));

    let mut child = Command::new("bash")
        .args(["-c", command])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(BashError::Spawn)?;

    let poll_interval = Duration::from_millis(50);
    let mut elapsed = Duration::ZERO;

    loop {
        match child.try_wait().map_err(BashError::Spawn)? {
            Some(status) => {
                let output = child.wait_with_output().map_err(BashError::Spawn)?;
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);

                let mut result = String::new();
                if !stdout.is_empty() {
                    result.push_str(&stdout);
                }
                if !stderr.is_empty() {
                    if !result.is_empty() {
                        result.push('\n');
                    }
                    result.push_str(&stderr);
                }
                if !status.success() && result.is_empty() {
                    result = format!("exit code: {}", status.code().unwrap_or(-1));
                }
                return Ok(result);
            }
            None => {
                if elapsed >= timeout {
                    let _ = child.kill();
                    return Err(BashError::Timeout(timeout.as_millis() as u64));
                }
                std::thread::sleep(poll_interval);
                elapsed += poll_interval;
            }
        }
    }
}
