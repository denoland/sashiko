use crate::db::Database;
use crate::events::Event;
use crate::nntp::NntpClient;
use crate::settings::NntpSettings;
use anyhow::Result;
use tokio::sync::mpsc::Sender;
use tokio::time::{Duration, sleep};
use tracing::{error, info};

pub struct Ingestor {
    settings: NntpSettings,
    db: Database,
    sender: Sender<Event>,
}

impl Ingestor {
    pub fn new(settings: NntpSettings, db: Database, sender: Sender<Event>) -> Self {
        Self {
            settings,
            db,
            sender,
        }
    }

    pub async fn run(&self) -> Result<()> {
        info!(
            "Starting NNTP Ingestor for groups: {:?}",
            self.settings.groups
        );

        loop {
            if let Err(e) = self.process_cycle().await {
                error!("Ingestion cycle failed: {}", e);
            }
            // Poll every 60 seconds for now
            sleep(Duration::from_secs(60)).await;
        }
    }

    async fn process_cycle(&self) -> Result<()> {
        let mut client = NntpClient::connect(&self.settings.server, self.settings.port).await?;

        for group_name in &self.settings.groups {
            // Ensure group exists in DB
            self.db.ensure_mailing_list(group_name, group_name).await?;

            let info = client.group(group_name).await?;
            let last_known = self.db.get_last_article_num(group_name).await?;

            info!(
                "Group {}: estimated count={}, low={}, high={}, last_known={}",
                group_name, info.number, info.low, info.high, last_known
            );

            let mut current = last_known;
            if current == 0 && info.high > 0 {
                // First run: jump to near the end to avoid fetching millions of articles
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
                            })
                            .await?;
                        self.db.update_last_article_num(group_name, next_id).await?;
                        info!("Updated high-water mark to {}", next_id);
                    }
                    Err(e) => {
                        error!("Failed to fetch article {}: {}", next_id, e);
                        // If it's a 423 (no such article number in group) we might need to skip?
                        // For now just log error.
                    }
                }
            }
        }

        client.quit().await?;
        Ok(())
    }
}
