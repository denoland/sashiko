use anyhow::{Result, anyhow};
use std::path::{Path, PathBuf};
use tempfile::TempDir;
use tokio::process::Command;
use tracing::{info, warn};

#[allow(dead_code)]
pub struct GitWorktree {
    pub dir: TempDir,
    pub path: PathBuf,
    pub repo_path: PathBuf,
}

impl GitWorktree {
    #[allow(dead_code)]
    pub async fn new(repo_path: &Path, commit_hash: &str, parent_dir: Option<&Path>) -> Result<Self> {
        let temp_dir = if let Some(parent) = parent_dir {
            if !parent.exists() {
                std::fs::create_dir_all(parent)?;
            }
            tempfile::Builder::new().prefix("sashiko-worktree-").tempdir_in(parent)?
        } else {
            TempDir::new()?
        };
        let path = temp_dir.path().to_path_buf();

        info!("Creating worktree at {:?}", path);

        let output = Command::new("git")
            .current_dir(repo_path)
            .args(["-c", "safe.bareRepository=all"])
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
            repo_path: repo_path.to_path_buf(),
        })
    }

    #[allow(dead_code)]
    pub async fn apply_patch(&self, patch_content: &str) -> Result<()> {
        info!("Applying patch in {:?}", self.path);

        let mut child = Command::new("git")
            .current_dir(&self.path)
            .args(["-c", "safe.bareRepository=all"])
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
            let _ = Command::new("git")
                .current_dir(&self.path)
                .args(["-c", "safe.bareRepository=all"])
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

    #[allow(dead_code)]
    pub async fn apply_raw_diff(&self, diff_content: &str) -> Result<std::process::Output> {
        info!("Applying raw diff in {:?}", self.path);

        let mut child = Command::new("git")
            .current_dir(&self.path)
            .args(["-c", "safe.bareRepository=all"])
            .arg("apply")
            .arg("-") // Read from stdin
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;

        if let Some(mut stdin) = child.stdin.take() {
            use tokio::io::AsyncWriteExt;
            stdin.write_all(diff_content.as_bytes()).await?;
        }

        let output = child.wait_with_output().await?;

        Ok(output)
    }

    #[allow(dead_code)]
    pub async fn remove(self) -> Result<()> {
        info!("Removing worktree at {:?}", self.path);
        let output = Command::new("git")
            .current_dir(&self.repo_path)
            .args(["-c", "safe.bareRepository=all"])
            .arg("worktree")
            .arg("remove")
            .arg("-f")
            .arg(&self.path)
            .output()
            .await?;

        if !output.status.success() {
            return Err(anyhow!(
                "Failed to remove worktree: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        Ok(())
    }
}

#[allow(dead_code)]
pub async fn read_blob(repo_path: &Path, hash: &str) -> Result<Vec<u8>> {
    let output = Command::new("git")
        .current_dir(repo_path)
        .args(["-c", "safe.bareRepository=all"])
        .arg("cat-file")
        .arg("-p")
        .arg(hash)
        .output()
        .await?;

    if !output.status.success() {
        return Err(anyhow!(
            "git cat-file failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(output.stdout)
}

#[allow(dead_code)]
pub async fn prune_worktrees(repo_path: &Path) -> Result<()> {
    info!("Pruning git worktrees in {:?}", repo_path);
    let output = Command::new("git")
        .current_dir(repo_path)
        .args(["-c", "safe.bareRepository=all"])
        .arg("worktree")
        .arg("prune")
        .output()
        .await?;

    if !output.status.success() {
        return Err(anyhow!(
            "git worktree prune failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}

#[allow(dead_code)]
pub async fn check_disk_usage(path: &Path) -> Result<String> {
    let output = Command::new("du").arg("-sh").arg(path).output().await?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Err(anyhow!(
            "Failed to check disk usage: {}",
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

impl Drop for GitWorktree {
    fn drop(&mut self) {
        warn!(
            "Dropping worktree at {:?}. Use explicit .remove() for clean git state.",
            self.path
        );
    }
}

pub async fn ensure_remote(repo_path: &Path, name: &str, url: &str, force_fetch: bool) -> Result<()> {
    // 1. Security Check
    if !url.contains("kernel.org") {
        return Err(anyhow!("Refusing to add non-kernel.org remote: {}", url));
    }

    let mut just_added = false;

    // 2. Check if exists
    let check = Command::new("git")
        .current_dir(repo_path)
        .args(["remote", "get-url", name])
        .output()
        .await?;

    if !check.status.success() {
        info!("Adding remote {} ({})", name, url);
        let add = Command::new("git")
            .current_dir(repo_path)
            .args(["remote", "add", name, url])
            .output()
            .await?;
        if !add.status.success() {
            return Err(anyhow!("Failed to add remote: {}", String::from_utf8_lossy(&add.stderr)));
        }
        just_added = true;
    }

    // 3. Lazy Fetch Check
    let timestamp_dir = repo_path.join(".sashiko/fetch_timestamps");
    if !timestamp_dir.exists() {
        std::fs::create_dir_all(&timestamp_dir)?;
    }
    let timestamp_file = timestamp_dir.join(name);

    let age = std::fs::metadata(&timestamp_file)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|m| std::time::SystemTime::now().duration_since(m).ok());
    
    let should_fetch = if just_added {
        true
    } else {
        match age {
            Some(a) => {
                if force_fetch {
                    // If forced (retry after failure), only fetch if last fetch was > 1 hour ago
                    a > std::time::Duration::from_secs(3600)
                } else {
                    // Standard lazy fetch: 12 hours
                    a > std::time::Duration::from_secs(12 * 3600)
                }
            }
            None => true, // No timestamp file, fetch to be safe
        }
    };

    if !should_fetch {
        let reason = if force_fetch { "forced but recently fetched" } else { "fresh" };
        info!("Skipping fetch for {} ({})", name, reason);
        return Ok(());
    }

    // 4. Fetch
    info!("Fetching remote {}", name);
    let fetch = Command::new("git")
        .current_dir(repo_path)
        .args(["fetch", name])
        .output()
        .await?;

    if !fetch.status.success() {
        return Err(anyhow!("Failed to fetch remote {}: {}", name, String::from_utf8_lossy(&fetch.stderr)));
    }

    // Update timestamp
    let _ = std::fs::File::create(&timestamp_file); // Touch file

    Ok(())
}

pub async fn get_commit_hash(path: &Path, ref_name: &str) -> Result<String> {
    let output = Command::new("git")
        .current_dir(path)
        .args(["rev-parse", ref_name])
        .output()
        .await?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Err(anyhow!("Failed to resolve {}: {}", ref_name, String::from_utf8_lossy(&output.stderr)))
    }
}
