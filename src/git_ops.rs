use anyhow::{Result, anyhow};
use std::path::{Path, PathBuf};
use tempfile::TempDir;
use tokio::process::Command;
use tracing::{info, warn};

#[allow(dead_code)]
pub struct GitWorktree {
    #[allow(dead_code)]
    pub dir: TempDir,
    pub path: PathBuf,
}

impl GitWorktree {
    #[allow(dead_code)]
    pub async fn new(repo_path: &Path, commit_hash: &str) -> Result<Self> {
        let temp_dir = TempDir::new()?;
        let path = temp_dir.path().to_path_buf();

        info!("Creating worktree at {:?}", path);

        // git worktree add --detach <path> <commit>
        let output = Command::new("git")
            .current_dir(repo_path)
            .arg("worktree")
            .arg("add")
            .arg("--detach")
            .arg(&path)
            .arg(commit_hash)
            .output()
            .await?;

        if !output.status.success() {
            return Err(anyhow!(
                "Failed to create worktree: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        Ok(Self {
            dir: temp_dir,
            path,
        })
    }

    #[allow(dead_code)]
    pub async fn apply_patch(&self, patch_content: &str) -> Result<()> {
        info!("Applying patch in {:?}", self.path);

        // git am
        // We pipe content to stdin
        let mut child = Command::new("git")
            .current_dir(&self.path)
            .arg("am")
            .arg("--3way")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;

        if let Some(mut stdin) = child.stdin.take() {
            use tokio::io::AsyncWriteExt;
            stdin.write_all(patch_content.as_bytes()).await?;
        }

        let output = child.wait_with_output().await?;

        if !output.status.success() {
            // Abort am if it failed
            let _ = Command::new("git")
                .current_dir(&self.path)
                .arg("am")
                .arg("--abort")
                .output()
                .await;

            return Err(anyhow!(
                "git am failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        Ok(())
    }
}

impl Drop for GitWorktree {
    fn drop(&mut self) {
        // Pruning worktrees is usually manual or implicit when dir is removed.
        // But 'git worktree prune' is needed to update the main repo's index of worktrees.
        // We can't easily run async code in Drop.
        // In a real system, we'd have a separate garbage collector or use `git worktree remove` explicitly before drop.
        // For now, we rely on `TempDir` deleting the directory, and `git worktree prune` can be run periodically.
        warn!(
            "Dropping worktree at {:?}. Remember to run 'git worktree prune'",
            self.path
        );
    }
}
