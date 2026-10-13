//! Real shell execution using PTY
//!
//! Note: This module contains infrastructure for real PTY-based shell execution,
//! which is planned for future releases. Currently, the application uses a simpler
//! command execution approach.

use anyhow::Result;
use portable_pty::{native_pty_system, CommandBuilder, PtySize, PtySystem};
use std::io::Read;
use std::sync::{Arc, Mutex};
use vte::{Params, Parser, Perform};

/// Error returned when a command exceeds its timeout and is killed
#[derive(Debug)]
pub struct CommandTimeout(pub std::time::Duration);

impl std::fmt::Display for CommandTimeout {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Command timed out after {} seconds", self.0.as_secs())
    }
}

impl std::error::Error for CommandTimeout {}

/// Shell executor using PTY (reserved for future real shell integration)
#[allow(dead_code)]
pub struct ShellExecutor {
    pty_system: Box<dyn PtySystem>,
    output_buffer: Arc<Mutex<Vec<u8>>>,
}

impl ShellExecutor {
    pub fn new() -> Result<Self> {
        let pty_system = native_pty_system();

        Ok(Self {
            pty_system,
            output_buffer: Arc::new(Mutex::new(Vec::new())),
        })
    }

    /// Execute a command and return the output (blocking version for internal use)
    #[allow(dead_code)]
    fn execute_blocking(&mut self, command: &str) -> Result<String> {
        // Create a new PTY
        let pair = self.pty_system.openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })?;

        // Spawn the shell command
        let mut cmd = CommandBuilder::new("sh");
        cmd.arg("-c");
        cmd.arg(command);

        let mut child = pair.slave.spawn_command(cmd)?;
        // The parent must not hold the slave end: on Linux the master never
        // reaches EOF while it is open, and the read below would block
        // after the child exits.
        drop(pair.slave);

        // Read output from master
        let mut reader = pair.master.try_clone_reader()?;
        let mut output = Vec::new();
        let mut buffer = [0u8; 8192];

        // Read with timeout
        let timeout = std::time::Duration::from_secs(5);
        let start = std::time::Instant::now();

        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break, // EOF
                Ok(n) => {
                    output.extend_from_slice(&buffer[..n]);
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    if start.elapsed() > timeout {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(10));
                    continue;
                }
                Err(_) => break,
            }

            // Check if command finished
            if let Ok(Some(_)) = child.try_wait() {
                // Read any remaining output
                std::thread::sleep(std::time::Duration::from_millis(50));
                if let Ok(n) = reader.read(&mut buffer) {
                    if n > 0 {
                        output.extend_from_slice(&buffer[..n]);
                    }
                }
                break;
            }

            if start.elapsed() > timeout {
                break;
            }
        }

        // Convert output to string
        let output_str = String::from_utf8_lossy(&output).to_string();

        // Strip ANSI codes for now (we can keep them later for colors)
        let stripped = strip_ansi_codes(&output_str);

        Ok(stripped)
    }

    /// Execute a command asynchronously without blocking the UI
    ///
    /// The child process is killed (not abandoned) if it does not complete
    /// within `timeout`; a `CommandTimeout` error is returned in that case.
    ///
    /// When `cwd` is `Some`, the command runs in that directory instead of
    /// inheriting the process working directory (used by lesson real-practice
    /// mode to pin execution inside the playground).
    pub async fn execute(
        &mut self,
        command: String,
        env_vars: std::collections::HashMap<String, String>,
        timeout: std::time::Duration,
        cwd: Option<std::path::PathBuf>,
        jail: Option<&crate::jail::Jail>,
    ) -> Result<String> {
        use std::process::Stdio;
        use tokio::io::AsyncReadExt;
        use tokio::process::Command;

        // Build command with environment variables: sh -c, inside the OS
        // jail when one is given
        let mut cmd = match jail {
            Some(jail) => {
                let (program, args) = jail.wrap(&command);
                let mut cmd = Command::new(program);
                cmd.args(args).env("TMPDIR", jail.tmp());
                cmd
            }
            None => {
                let mut cmd = Command::new("sh");
                cmd.arg("-c").arg(&command);
                cmd
            }
        };
        cmd.env("TERM", "xterm-256color")
           .stdin(Stdio::null())
           .stdout(Stdio::piped())
           .stderr(Stdio::piped())
           // Safety net: if this future is dropped, kill the child too
           .kill_on_drop(true);

        // Pin the working directory when requested (playground practice mode)
        if let Some(dir) = cwd {
            cmd.current_dir(dir);
        }

        // Add custom environment variables
        for (key, value) in env_vars {
            cmd.env(key, value);
        }

        let mut child = cmd.spawn()
            .map_err(|e| anyhow::anyhow!("Failed to execute command: {}", e))?;

        // Drain stdout/stderr concurrently so a chatty child can't fill the
        // pipe buffers and deadlock against wait()
        let mut stdout_pipe = child.stdout.take();
        let stdout_task = tokio::spawn(async move {
            let mut buf = Vec::new();
            if let Some(ref mut pipe) = stdout_pipe {
                let _ = pipe.read_to_end(&mut buf).await;
            }
            buf
        });
        let mut stderr_pipe = child.stderr.take();
        let stderr_task = tokio::spawn(async move {
            let mut buf = Vec::new();
            if let Some(ref mut pipe) = stderr_pipe {
                let _ = pipe.read_to_end(&mut buf).await;
            }
            buf
        });

        let status = match tokio::time::timeout(timeout, child.wait()).await {
            Ok(status) => status
                .map_err(|e| anyhow::anyhow!("Failed to wait for command: {}", e))?,
            Err(_) => {
                // Timeout fired: explicitly kill and reap the child instead
                // of abandoning it to run forever
                let _ = child.kill().await;
                stdout_task.abort();
                stderr_task.abort();
                return Err(CommandTimeout(timeout).into());
            }
        };

        let stdout_buf = stdout_task.await.unwrap_or_default();
        let stderr_buf = stderr_task.await.unwrap_or_default();
        let stdout = String::from_utf8_lossy(&stdout_buf);
        let stderr = String::from_utf8_lossy(&stderr_buf);

        let mut result = String::new();
        if !stdout.is_empty() {
            // Keep ANSI codes for colored output!
            result.push_str(&stdout);
        }
        // Errors read like they do in a terminal: just the message, after
        // anything the command printed
        if !stderr.is_empty() {
            if !result.is_empty() && !result.ends_with('\n') {
                result.push('\n');
            }
            result.push_str(&stderr);
        }

        let code = status.code().unwrap_or(-1);
        if status.success() {
            if result.is_empty() {
                result = "✓ Command completed".to_string();
            }
        } else {
            if !result.is_empty() && !result.ends_with('\n') {
                result.push('\n');
            }
            result.push_str(&format!(
                "{}The command failed (exit code {code})",
                crate::icons::error().content
            ));
        }

        Ok(result)
    }
}

impl Default for ShellExecutor {
    fn default() -> Self {
        Self::new().expect("Failed to create shell executor")
    }
}

/// Strip ANSI escape codes from text (reserved for future PTY output processing)
#[allow(dead_code)]
fn strip_ansi_codes(text: &str) -> String {
    let mut result = String::new();
    let mut parser = Parser::new();
    let mut performer = StripAnsiPerformer {
        output: &mut result,
    };

    for byte in text.bytes() {
        parser.advance(&mut performer, byte);
    }

    result
}

#[allow(dead_code)]
struct StripAnsiPerformer<'a> {
    output: &'a mut String,
}

impl<'a> Perform for StripAnsiPerformer<'a> {
    fn print(&mut self, c: char) {
        self.output.push(c);
    }

    fn execute(&mut self, byte: u8) {
        if byte == b'\n' || byte == b'\r' || byte == b'\t' {
            self.output.push(byte as char);
        }
    }

    fn hook(&mut self, _params: &Params, _intermediates: &[u8], _ignore: bool, _c: char) {}
    fn put(&mut self, _byte: u8) {}
    fn unhook(&mut self) {}
    fn osc_dispatch(&mut self, _params: &[&[u8]], _bell_terminated: bool) {}
    fn csi_dispatch(&mut self, _params: &Params, _intermediates: &[u8], _ignore: bool, _c: char) {}
    fn esc_dispatch(&mut self, _intermediates: &[u8], _ignore: bool, _byte: u8) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_execute_simple_command() {
        let mut executor = ShellExecutor::new().unwrap();
        let output = executor.execute_blocking("echo 'hello world'").unwrap();
        assert!(output.contains("hello world"));
    }

    #[test]
    fn test_execute_ls() {
        let mut executor = ShellExecutor::new().unwrap();
        let output = executor.execute_blocking("ls").unwrap();
        assert!(!output.is_empty());
    }

    #[tokio::test]
    async fn test_execute_async_completes_within_timeout() {
        let mut executor = ShellExecutor::new().unwrap();
        let output = executor
            .execute(
                "echo 'hello async'".to_string(),
                std::collections::HashMap::new(),
                std::time::Duration::from_secs(5),
                None,
                None,
            )
            .await
            .unwrap();
        assert!(output.contains("hello async"));
    }

    #[tokio::test]
    async fn test_execute_with_explicit_cwd() {
        let mut executor = ShellExecutor::new().unwrap();
        let dir = std::env::temp_dir()
            .join(format!("arct-shell-cwd-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let canonical = dir.canonicalize().unwrap();

        let output = executor
            .execute(
                "pwd".to_string(),
                std::collections::HashMap::new(),
                std::time::Duration::from_secs(5),
                Some(canonical.clone()),
                None,
            )
            .await
            .unwrap();
        assert!(
            output.trim().ends_with(canonical.to_str().unwrap()),
            "pwd output {output:?} should be the requested cwd"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_timeout_kills_child_process() {
        use std::time::{Duration, Instant};

        let mut executor = ShellExecutor::new().unwrap();

        let pid_file = std::env::temp_dir()
            .join(format!("arct-shell-timeout-test-{}.pid", std::process::id()));
        let _ = std::fs::remove_file(&pid_file);

        // `exec` makes sleep replace the sh process, so the child pid we
        // spawned IS the sleep pid
        let command = format!("echo $$ > '{}'; exec sleep 30", pid_file.display());

        let start = Instant::now();
        let result = executor
            .execute(command, std::collections::HashMap::new(), Duration::from_secs(1), None, None)
            .await;

        // Must report the timeout and return promptly
        let err = result.expect_err("expected timeout error");
        assert!(
            err.downcast_ref::<CommandTimeout>().is_some(),
            "expected CommandTimeout, got: {err}"
        );
        assert!(start.elapsed() < Duration::from_secs(10));

        let pid: i32 = std::fs::read_to_string(&pid_file)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        let _ = std::fs::remove_file(&pid_file);

        // Poll until the process is actually dead (`kill -0` fails once the
        // process no longer exists; child.kill().await also reaps it, so no
        // zombie can keep the pid alive)
        let mut alive = true;
        for _ in 0..40 {
            let status = std::process::Command::new("kill")
                .arg("-0")
                .arg(pid.to_string())
                .stderr(std::process::Stdio::null())
                .status()
                .unwrap();
            if !status.success() {
                alive = false;
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert!(!alive, "child process {pid} is still running after timeout");
    }
}
