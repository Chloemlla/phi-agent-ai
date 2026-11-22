use std::path::{Component, Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use async_trait::async_trait;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

use phi_contracts::SLOT_SANDBOX_EXECUTE;
use phi_contracts::ports::{ExecOutput, SandboxPort};
use phi_i18n as i18n;
use phi_kernel::lifecycle::{Plugin, PluginContext};
use phi_kernel::registry::Contribution;

pub struct FsSandboxPlugin;

impl Plugin for FsSandboxPlugin {
    fn id(&self) -> &'static str {
        "plugin-sandbox"
    }

    fn activate(&self, _ctx: &PluginContext) -> Result<Vec<Contribution>, String> {
        Ok(vec![Contribution {
            plugin_id: self.id().to_string(),
            slot: SLOT_SANDBOX_EXECUTE.to_string(),
            id: "fs-jail".to_string(),
        }])
    }
}

#[async_trait]
impl SandboxPort for FsSandboxPlugin {
    fn resolve_path(&self, workspace: &Path, requested: &str) -> Result<PathBuf, String> {
        let workspace = workspace.canonicalize().map_err(|e| {
            i18n::t_args(
                "Workspace is not available: {error}",
                &[("error", e.to_string())],
            )
        })?;
        let requested = requested.trim();
        let requested = if requested.is_empty() { "." } else { requested };
        let relative = Path::new(requested);

        for component in relative.components() {
            match component {
                Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                    return Err(i18n::t("Access outside the workspace is denied"));
                }
                _ => {}
            }
        }

        let joined = workspace.join(relative);
        let mut ancestor = joined.as_path();
        while !ancestor.exists() {
            ancestor = ancestor.parent().ok_or_else(|| i18n::t("Invalid path"))?;
            if ancestor.as_os_str().is_empty() {
                return Err(i18n::t("Invalid path"));
            }
        }
        let canonical_ancestor = ancestor.canonicalize().map_err(|e| {
            i18n::t_args(
                "Path is not available: {error}",
                &[("error", e.to_string())],
            )
        })?;
        if !canonical_ancestor.starts_with(&workspace) {
            return Err(i18n::t("Access outside the workspace is denied"));
        }
        let suffix = joined
            .strip_prefix(ancestor)
            .map_err(|_| i18n::t("Failed to resolve path"))?;
        if suffix.as_os_str().is_empty() {
            Ok(canonical_ancestor)
        } else {
            Ok(canonical_ancestor.join(suffix))
        }
    }

    async fn exec(
        &self,
        workspace: &Path,
        command: &str,
        timeout: Duration,
        cancel: CancellationToken,
    ) -> Result<ExecOutput, String> {
        let mut cmd = build_command(command);
        cmd.current_dir(workspace)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        {
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }

        let mut child = cmd.spawn().map_err(|e| {
            i18n::t_args(
                "Failed to start command: {error}",
                &[("error", e.to_string())],
            )
        })?;
        let pid = child.id();

        let mut stdout_pipe = child.stdout.take();
        let mut stderr_pipe = child.stderr.take();
        let stdout_task = tokio::spawn(async move {
            match stdout_pipe.take() {
                Some(pipe) => drain_pipe(pipe, MAX_PIPE_BYTES).await,
                None => (Vec::new(), false),
            }
        });
        let stderr_task = tokio::spawn(async move {
            match stderr_pipe.take() {
                Some(pipe) => drain_pipe(pipe, MAX_PIPE_BYTES).await,
                None => (Vec::new(), false),
            }
        });

        let deadline = tokio::time::Instant::now() + timeout;
        let mut timed_out = false;
        let mut cancelled = false;
        let mut status = None;

        loop {
            if cancel.is_cancelled() {
                cancelled = true;
                kill_process_tree(pid, &mut child).await;
                break;
            }
            match child.try_wait() {
                Ok(Some(exit)) => {
                    status = Some(exit);
                    break;
                }
                Ok(None) => {}
                Err(e) => {
                    return Err(i18n::t_args(
                        "Failed to wait for command: {error}",
                        &[("error", e.to_string())],
                    ));
                }
            }
            if tokio::time::Instant::now() >= deadline {
                timed_out = true;
                kill_process_tree(pid, &mut child).await;
                break;
            }
            tokio::time::sleep(Duration::from_millis(40)).await;
        }

        if status.is_none() {
            status = child.wait().await.ok();
        }

        let collect = |task: tokio::task::JoinHandle<(Vec<u8>, bool)>| async move {
            match tokio::time::timeout(Duration::from_secs(2), task).await {
                Ok(Ok(result)) => result,
                _ => (Vec::new(), false),
            }
        };
        let (stdout, stdout_truncated) = collect(stdout_task).await;
        let (stderr, stderr_truncated) = collect(stderr_task).await;

        if cancelled {
            return Err("cancelled".to_string());
        }

        Ok(ExecOutput {
            exit_code: status.and_then(|s| s.code()),
            stdout: String::from_utf8_lossy(&stdout).to_string(),
            stderr: String::from_utf8_lossy(&stderr).to_string(),
            timed_out,
            stdout_truncated,
            stderr_truncated,
        })
    }
}

const MAX_PIPE_BYTES: usize = 1_000_000;

/// Keep reading the pipe so the child never blocks on a full buffer, but only
/// retain the first `keep` bytes for the tool result.
async fn drain_pipe<R>(mut pipe: R, keep: usize) -> (Vec<u8>, bool)
where
    R: AsyncRead + Unpin,
{
    let mut kept = Vec::with_capacity(keep.min(64 * 1024));
    let mut buf = vec![0u8; 16 * 1024];
    let mut truncated = false;
    loop {
        match pipe.read(&mut buf).await {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                let room = keep.saturating_sub(kept.len());
                let take = n.min(room);
                kept.extend_from_slice(&buf[..take]);
                if take < n {
                    truncated = true;
                }
            }
        }
    }
    (kept, truncated)
}

fn build_command(command: &str) -> Command {
    #[cfg(windows)]
    {
        let mut cmd = Command::new("cmd");
        cmd.arg("/C").arg(command);
        cmd
    }
    #[cfg(not(windows))]
    {
        let mut cmd = Command::new("sh");
        cmd.arg("-c").arg(command);
        // Give the shell its own process group so cancellation can terminate
        // the whole tree, not just the shell.
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            cmd.as_std_mut().process_group(0);
        }
        cmd
    }
}

async fn kill_process_tree(pid: Option<u32>, child: &mut tokio::process::Child) {
    #[cfg(windows)]
    if let Some(pid) = pid {
        let _ = Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await;
    }
    #[cfg(unix)]
    if let Some(pid) = pid {
        // Negative pid targets the process group created in `build_command`.
        unsafe {
            libc::kill(-(pid as i32), libc::SIGKILL);
        }
    }
    let _ = child.start_kill();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_ws(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("phi-ws-{name}-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn rejects_parent_escape() {
        let ws = temp_ws("escape");
        let sandbox = FsSandboxPlugin;
        assert!(sandbox.resolve_path(&ws, "../secret.txt").is_err());
        assert!(sandbox.resolve_path(&ws, "a/../../secret.txt").is_err());
        // Drive letters only exist on Windows; on other systems this is an
        // ordinary relative name that stays inside the workspace.
        #[cfg(windows)]
        assert!(sandbox.resolve_path(&ws, "C:/Windows/win.ini").is_err());
    }

    #[test]
    fn allows_workspace_relative() {
        let ws = temp_ws("inside");
        let sandbox = FsSandboxPlugin;
        let resolved = sandbox.resolve_path(&ws, "src/main.rs").unwrap();
        assert!(resolved.starts_with(ws.canonicalize().unwrap()));
    }

    #[test]
    fn existing_file_keeps_its_name_without_a_trailing_separator() {
        let ws = temp_ws("existing");
        fs::write(ws.join("large.txt"), "x").unwrap();
        let sandbox = FsSandboxPlugin;
        let resolved = sandbox.resolve_path(&ws, "large.txt").unwrap();
        assert!(resolved.is_file(), "resolved path must point at the file");
        assert_eq!(resolved.file_name().unwrap(), "large.txt");
    }

    #[tokio::test]
    async fn drains_output_past_the_retained_limit() {
        let (mut writer, reader) = tokio::io::duplex(64);
        let writer_task = tokio::spawn(async move {
            let payload = vec![b'x'; 4_096];
            tokio::io::AsyncWriteExt::write_all(&mut writer, &payload)
                .await
                .unwrap();
        });
        let (kept, truncated) = drain_pipe(reader, 1_024).await;
        writer_task.await.unwrap();
        assert_eq!(kept.len(), 1_024);
        assert!(
            truncated,
            "output beyond the retained limit must be flagged"
        );
    }

    #[tokio::test]
    async fn executes_command_in_workspace() {
        let ws = temp_ws("exec");
        let sandbox = FsSandboxPlugin;
        let out = sandbox
            .exec(
                &ws,
                "echo phi-sandbox-ok",
                Duration::from_secs(10),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert!(out.stdout.contains("phi-sandbox-ok"));
    }
}
