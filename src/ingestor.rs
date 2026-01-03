use crate::db::Database;
use crate::events::Event;
use crate::nntp::NntpClient;
use crate::settings::Settings;
use anyhow::{Result, anyhow};
use serde_json::Value;
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc::Sender;
use tokio::time::{Duration, sleep};
use tracing::{error, info, warn};

pub struct Ingestor {
    settings: Settings,
    db: Arc<Database>,
    sender: Sender<Event>,
    download: Option<usize>,
    no_nntp: bool,
}

impl Ingestor {
    pub fn new(
        settings: Settings,
        db: Arc<Database>,
        sender: Sender<Event>,
        download: Option<usize>,
        no_nntp: bool,
    ) -> Self {
        Self {
            settings,
            db,
            sender,
            download,
            no_nntp,
        }
    }

    pub async fn run(&self) -> Result<()> {
        if let Some(n) = self.download {
            info!(
                "Bootstrap requested: downloading/ingesting last {} messages from git archive",
                n
            );
            if let Err(e) = self.run_git_bootstrap(n).await {
                error!("Git bootstrap failed: {}", e);
            }
        }

        if !self.no_nntp {
            self.run_nntp().await?;
        } else {
            info!("NNTP ingestor disabled via command line.");
        }

        Ok(())
    }

    async fn run_git_bootstrap(&self, n: usize) -> Result<()> {
        for group in &self.settings.nntp.groups {
            match self.resolve_git_info(group).await {
                Ok((url, path)) => {
                    info!("Bootstrapping group {} from {} to {:?}", group, url, path);

                    if let Err(e) = self.bootstrap_repo(&url, &path, n).await {
                        error!("Failed to bootstrap group {}: {}", group, e);
                    } else if let Err(e) = self.ingest_git_objects(&path, Some(n)).await {
                        error!("Failed to ingest objects for group {}: {}", group, e);
                    }
                }
                Err(e) => {
                    error!("Failed to resolve git info for group {}: {}", group, e);
                }
            }
        }
        Ok(())
    }

    async fn resolve_git_info(&self, group: &str) -> Result<(String, std::path::PathBuf)> {
        // Dynamic path: archives/<group_name>
        let path = std::path::PathBuf::from("archives").join(group);

        // Dynamic URL heuristic
        // org.kernel.vger.linux-kernel -> lkml
        // org.kernel.vger.netdev -> netdev
        // etc.
        let list_id = if group == "org.kernel.vger.linux-kernel" {
            "lkml"
        } else {
            group.split('.').next_back().unwrap_or(group)
        };

        let url = self.find_latest_epoch(list_id).await?;

        Ok((url, path))
    }

    async fn find_latest_epoch(&self, list_id: &str) -> Result<String> {
        info!("Fetching manifest to find latest epoch for {}", list_id);

        let output = Command::new("bash")
            .arg("-c")
            .arg("curl -s https://lore.kernel.org/manifest.js.gz | gunzip")
            .output()
            .await?;

        if !output.status.success() {
            return Err(anyhow!("Failed to fetch manifest"));
        }

        let json: Value = serde_json::from_slice(&output.stdout)?;
        let map = json
            .as_object()
            .ok_or_else(|| anyhow!("Manifest is not a JSON object"))?;

        let mut max_epoch = -1;
        let mut best_url = String::new();

        // Pattern: /list_id/git/N.git
        let prefix = format!("/{}/git/", list_id);

        for (key, _val) in map {
            if key.starts_with(&prefix) && key.ends_with(".git") {
                // extract N
                // key looks like /lkml/git/0.git
                // suffix is 0
                let suffix = &key[prefix.len()..key.len() - 4];
                if let Ok(epoch) = suffix.parse::<i32>() {
                    if epoch > max_epoch {
                        max_epoch = epoch;
                        best_url = format!("https://lore.kernel.org{}", key);
                    }
                }
            }
        }

        if max_epoch == -1 {
            // Fallback to 0.git if nothing found
            warn!(
                "Could not find any epochs for {}, defaulting to 0.git",
                list_id
            );
            return Ok(format!("https://lore.kernel.org/{}/0.git", list_id));
        }

        info!(
            "Found latest epoch {} for {}: {}",
            max_epoch, list_id, best_url
        );
        Ok(best_url)
    }

    async fn bootstrap_repo(&self, url: &str, path: &std::path::Path, n: usize) -> Result<()> {
        // 1. Ensure repo exists
        if !path.exists() {
            info!(
                "Cloning archive from {} to {:?} with depth {}",
                url, path, n
            );
            // Parent directory must exist
            if let Some(parent) = path.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }

            let output = Command::new("git")
                .arg("clone")
                .arg("--bare")
                .arg(format!("--depth={}", n))
                .arg(url)
                .arg(path)
                .output()
                .await?;

            if !output.status.success() {
                return Err(anyhow!(
                    "Git clone failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                ));
            }
        } else {
            // Repo exists, ensure remote is correct then fetch
            let remote_output = Command::new("git")
                .arg("-c")
                .arg("safe.bareRepository=all")
                .current_dir(path)
                .arg("remote")
                .arg("get-url")
                .arg("origin")
                .output()
                .await?;

            if remote_output.status.success() {
                let current_url = String::from_utf8_lossy(&remote_output.stdout).trim().to_string();
                if current_url != url {
                    info!("Updating remote origin from {} to {}", current_url, url);
                    let set_url_output = Command::new("git")
                        .arg("-c")
                        .arg("safe.bareRepository=all")
                        .current_dir(path)
                        .arg("remote")
                        .arg("set-url")
                        .arg("origin")
                        .arg(url)
                        .output()
                        .await?;
                    
                    if !set_url_output.status.success() {
                         warn!("Failed to update remote url: {}", String::from_utf8_lossy(&set_url_output.stderr));
                    }
                }
            }

            info!("Fetching latest changes in {:?} with depth {}", path, n);
            let output = Command::new("git")
                .arg("-c")
                .arg("safe.bareRepository=all")
                .current_dir(path)
                .arg("fetch")
                .arg(format!("--depth={}", n))
                .arg("origin")
                .arg("+refs/heads/*:refs/heads/*") // Fetch all heads
                .output()
                .await?;

            if !output.status.success() {
                // Warn but continue, maybe we are offline
                warn!(
                    "Git fetch failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        }
        Ok(())
    }
    async fn run_nntp(&self) -> Result<()> {
        info!(
            "Starting NNTP Ingestor for groups: {:?}",
            self.settings.nntp.groups
        );

        loop {
            if let Err(e) = self.process_nntp_cycle().await {
                error!("NNTP Ingestion cycle failed: {}", e);
            }
            sleep(Duration::from_secs(60)).await;
        }
    }

    async fn process_nntp_cycle(&self) -> Result<()> {
        let mut client =
            NntpClient::connect(&self.settings.nntp.server, self.settings.nntp.port).await?;

        for group_name in &self.settings.nntp.groups {
            self.db.ensure_mailing_list(group_name, group_name).await?;

            let info = client.group(group_name).await?;
            let last_known = self.db.get_last_article_num(group_name).await?;

            info!(
                "Group {}: estimated count={}, low={}, high={}, last_known={}",
                group_name, info.number, info.low, info.high, last_known
            );

            let mut current = last_known;
            if current == 0 && info.high > 0 {
                // If we have bootstrapped from git, we might want to be smarter here.
                // But git archives don't easily map to NNTP article numbers unless we query the Message-ID.
                // For now, if we are fresh, start from high - 5 (as per original logic).
                // TODO: Sync git message-ids to NNTP article numbers if possible?
                current = info.high.saturating_sub(5);
                self.db.update_last_article_num(group_name, current).await?;
                info!("Initialized high-water mark to {}", current);
            }

            if current < info.high {
                let next_id = current + 1;
                info!("Fetching article {}", next_id);
                match client.article(&next_id.to_string()).await {
                    Ok(lines) => {
                        self.sender
                            .send(Event::ArticleFetched {
                                group: group_name.clone(),
                                article_id: next_id.to_string(),
                                content: lines,
                                raw: None,
                            })
                            .await?;
                        self.db.update_last_article_num(group_name, next_id).await?;
                        info!("Updated high-water mark to {}", next_id);
                    }
                    Err(e) => {
                        error!("Failed to fetch article {}: {}", next_id, e);
                    }
                }
            }
        }

        client.quit().await?;
        Ok(())
    }

    async fn ingest_git_objects(&self, path: &std::path::Path, limit: Option<usize>) -> Result<()> {
        info!("Starting Git Ingestion from {:?}", path);

        // 1. Get list of all object SHAs
        info!("Listing git objects...");
        let mut command = Command::new("git");
        command
            .arg("-c")
            .arg("safe.bareRepository=all")
            .current_dir(path)
            .arg("rev-list")
            .arg("--all")
            .arg("--objects");

        if let Some(n) = limit {
            command.arg(format!("--max-count={}", n));
        }

        let output = command.output().await?;

        if !output.status.success() {
            return Err(anyhow!(
                "Failed to list git objects: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let shas: Vec<String> = stdout
            .lines()
            .filter_map(|line| line.split_whitespace().next().map(|s| s.to_string()))
            .collect();

        info!("Found {} objects. Starting batch processing...", shas.len());

        // 2. Start git cat-file --batch
        let mut child = Command::new("git")
            .arg("-c")
            .arg("safe.bareRepository=all")
            .current_dir(path)
            .arg("cat-file")
            .arg("--batch")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?;

        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow!("Failed to open stdin for git cat-file"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("Failed to open stdout for git cat-file"))?;
        let mut reader = BufReader::new(stdout);

        let mut count = 0;
        let mut processed_blobs = 0;

        for hash in shas {
            // Write SHA to stdin
            stdin.write_all(format!("{}\n", hash).as_bytes()).await?;
            stdin.flush().await?;

            // Read header: <sha> <type> <size>
            let mut header = String::new();
            if reader.read_line(&mut header).await? == 0 {
                break; // EOF
            }

            let parts: Vec<&str> = header.split_whitespace().collect();
            if parts.len() < 3 {
                warn!("Invalid batch header for {}: {}", hash, header);
                continue;
            }

            let obj_type = parts[1];
            let size: usize = parts[2].parse().unwrap_or(0);

            // Read content + newline
            let mut content = vec![0u8; size];
            reader.read_exact(&mut content).await?;

            // Consume the trailing newline that --batch outputs
            let mut newline = [0u8; 1];
            reader.read_exact(&mut newline).await?;

            if obj_type == "blob" {
                // Convert raw to lines for the 'content' field (legacy)
                let content_str = String::from_utf8_lossy(&content);
                let lines: Vec<String> = content_str.lines().map(|s| s.to_string()).collect();

                self.sender
                    .send(Event::ArticleFetched {
                        group: "git-archive".to_string(),
                        article_id: hash.clone(),
                        content: lines,
                        raw: Some(content),
                    })
                    .await?;

                processed_blobs += 1;
                if processed_blobs % 1000 == 0 {
                    info!("Processed {} blobs", processed_blobs);
                }
            }

            count += 1;
        }

        info!(
            "Git ingestion completed. Scanned {} objects, processed {} blobs.",
            count, processed_blobs
        );
        Ok(())
    }
}
