// Copyright 2026 The Sashiko Authors
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     https://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use crate::db::Database;
use crate::events::Event;
use crate::github::GitHubClient;
use crate::settings::Settings;
use anyhow::{Result, anyhow};
use std::sync::Arc;
use tokio::process::Command;
use tokio::sync::mpsc::Sender;
use tokio::time::{Duration, sleep};
use tracing::{error, info, warn};

pub struct Ingestor {
    settings: Settings,
    db: Arc<Database>,
    sender: Sender<Event>,
    download: Option<usize>,
    track: bool,
}

impl Ingestor {
    pub fn new(
        settings: Settings,
        db: Arc<Database>,
        sender: Sender<Event>,
        download: Option<usize>,
        track: bool,
    ) -> Self {
        Self {
            settings,
            db,
            sender,
            download,
            track,
        }
    }

    fn github_client(&self) -> GitHubClient {
        GitHubClient::new(
            &self.settings.github.owner,
            &self.settings.github.repo,
            self.settings.github.token.clone(),
        )
    }

    fn repo_path(&self) -> &str {
        &self.settings.git.repository_path
    }

    pub async fn run(&self) -> Result<()> {
        // Ensure the mailing list entry exists for linking
        self.db.ensure_mailing_list("github", "github").await?;

        if let Some(n) = self.download {
            info!(
                "Download mode: fetching last {} merged PRs from {}/{}",
                n, self.settings.github.owner, self.settings.github.repo
            );
            if let Err(e) = self.run_download(n).await {
                error!("Download mode failed: {}", e);
            }
        }

        if self.track {
            self.run_poll().await?;
        } else {
            info!("Live tracking disabled (default). Use --track to enable.");
        }

        Ok(())
    }

    /// Download mode: fetch the last N recently merged PRs and process them.
    async fn run_download(&self, limit: usize) -> Result<()> {
        let client = self.github_client();

        // List closed PRs sorted by recently updated, filter for merged ones.
        let mut merged_prs = Vec::new();
        let mut page = 1u32;

        while merged_prs.len() < limit {
            let prs = client.list_closed_prs(100, page).await?;
            if prs.is_empty() {
                break;
            }

            for pr in prs {
                if merged_prs.len() >= limit {
                    break;
                }
                // A merged PR has merged_at set
                if pr.merged_at.is_some() {
                    merged_prs.push((pr.number, pr.title, pr.base.ref_name, pr.user.login));
                }
            }

            page += 1;
        }

        info!("Found {} merged PRs to process", merged_prs.len());

        for (pr_number, title, base_branch, author_login) in merged_prs {
            if let Err(e) = self
                .process_pr(pr_number, &title, &base_branch, &author_login)
                .await
            {
                error!("Failed to process PR #{}: {}", pr_number, e);
            }
        }

        Ok(())
    }

    /// Poll mode: continuously poll for new/updated open PRs.
    async fn run_poll(&self) -> Result<()> {
        let poll_interval = self.settings.github.poll_interval_secs;
        info!(
            "Starting GitHub PR poller for {}/{} (interval: {}s)",
            self.settings.github.owner, self.settings.github.repo, poll_interval
        );

        loop {
            if let Err(e) = self.poll_cycle().await {
                error!("GitHub poll cycle failed: {}", e);
            }
            sleep(Duration::from_secs(poll_interval)).await;
        }
    }

    async fn poll_cycle(&self) -> Result<()> {
        let client = self.github_client();

        // Fetch recently updated open PRs
        let prs = client.list_open_prs(100, 1).await?;
        info!("Found {} open PRs", prs.len());

        let last_known = self.db.get_last_article_num("github").await?;

        for pr in prs {
            // Use PR number as the article ID for tracking.
            // Skip PRs we've already processed (PR number <= last_known).
            if pr.number <= last_known {
                continue;
            }

            if let Err(e) = self
                .process_pr(pr.number, &pr.title, &pr.base.ref_name, &pr.user.login)
                .await
            {
                error!("Failed to process PR #{}: {}", pr.number, e);
                continue;
            }

            // Update high-water mark
            self.db.update_last_article_num("github", pr.number).await?;
        }

        Ok(())
    }

    /// Process a single PR: fetch the PR diff as a single patch for review.
    async fn process_pr(
        &self,
        pr_number: u64,
        title: &str,
        base_branch: &str,
        author_login: &str,
    ) -> Result<()> {
        let repo_path = self.repo_path();
        info!("Processing PR #{}: {}", pr_number, title);

        let client = self.github_client();

        // 1. Fetch both the PR head and the base branch with enough depth for merge-base
        let pr_ref = format!("pull/{}/head", pr_number);
        let local_ref = format!("refs/pull/{}/head", pr_number);
        let fetch_refspec = format!("{}:{}", pr_ref, local_ref);

        let output = Command::new("git")
            .current_dir(repo_path)
            .args(["fetch", "--depth", "200", "origin", &fetch_refspec, &format!("+refs/heads/{}:refs/remotes/origin/{}", base_branch, base_branch)])
            .output()
            .await?;

        if !output.status.success() {
            return Err(anyhow!(
                "git fetch PR #{} failed: {}",
                pr_number,
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }

        // 2. Get the merge base — deepen incrementally if needed
        let origin_base = format!("origin/{}", base_branch);
        let mut base_sha = None;

        for depth in &[0, 200, 500] {
            if *depth > 0 {
                // Deepen the repo to find the merge base
                let _ = Command::new("git")
                    .current_dir(repo_path)
                    .args(["fetch", "--deepen", &depth.to_string(), "origin", &format!("+refs/heads/{}:refs/remotes/origin/{}", base_branch, base_branch)])
                    .output()
                    .await;
            }

            let output = Command::new("git")
                .current_dir(repo_path)
                .args(["merge-base", &origin_base, &local_ref])
                .output()
                .await?;

            if output.status.success() {
                base_sha = Some(String::from_utf8_lossy(&output.stdout).trim().to_string());
                break;
            }
        }

        let base_sha = match base_sha {
            Some(sha) => sha,
            None => {
                // Last resort: use origin/base_branch HEAD directly
                warn!("merge-base failed for PR #{} after deepening, using origin/{}", pr_number, base_branch);
                let output = Command::new("git")
                    .current_dir(repo_path)
                    .args(["rev-parse", &origin_base])
                    .output()
                    .await?;
                String::from_utf8_lossy(&output.stdout).trim().to_string()
            }
        };

        // 3. Get the head SHA
        let output = Command::new("git")
            .current_dir(repo_path)
            .args(["rev-parse", &local_ref])
            .output()
            .await?;

        if !output.status.success() {
            return Err(anyhow!("Failed to resolve PR head ref for #{}", pr_number));
        }

        let head_sha = String::from_utf8_lossy(&output.stdout).trim().to_string();

        // 4. Get the full PR diff (base...head)
        info!("PR #{}: computing diff {}..{} in {}", pr_number, &base_sha[..12], &head_sha[..12], repo_path);
        let output = Command::new("git")
            .current_dir(repo_path)
            .args(["diff", &base_sha, &head_sha])
            .output()
            .await?;

        let diff = if output.status.success() {
            let d = String::from_utf8_lossy(&output.stdout).to_string();
            info!("PR #{}: diff size = {} bytes", pr_number, d.len());
            d
        } else {
            return Err(anyhow!(
                "git diff failed for PR #{}: {}",
                pr_number,
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        };

        if diff.is_empty() {
            warn!("PR #{} has empty diff (base={} head={})", pr_number, base_sha, head_sha);
            return Ok(());
        }

        // 5. Resolve author from GitHub API
        let author = match client.get_pr_commits(pr_number).await {
            Ok(commits) if !commits.is_empty() => {
                let last = commits.last().unwrap();
                format!("{} <{}>", last.commit.author.name, last.commit.author.email)
            }
            _ => format!("{} <{}@users.noreply.github.com>", author_login, author_login),
        };

        let group = format!("github-pr:1:{}..{}", base_sha, head_sha);
        let article_id = pr_number.to_string();

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs() as i64;

        // 6. Emit a single patch for the whole PR
        // Use a synthetic message_id (not the head SHA) to avoid merge commit detection
        let message_id = format!("pr-{}@github.sashiko", pr_number);
        let event = Event::PatchSubmitted {
            group: group.clone(),
            article_id,
            message_id,
            subject: title.to_string(),
            author,
            message: format!("PR #{}: {}", pr_number, title),
            diff,
            base_commit: Some(base_sha),
            timestamp,
            index: 1,
            total: 1,
        };

        if let Err(e) = self.sender.send(event).await {
            error!("Failed to send PatchSubmitted event for PR #{}: {}", pr_number, e);
        }

        info!("Successfully processed PR #{}", pr_number);
        Ok(())
    }
}

pub fn split_mbox(raw: &[u8]) -> Vec<Vec<u8>> {
    let mut emails = Vec::new();
    let mut current_email = Vec::new();

    for line in raw.split_inclusive(|&b| b == b'\n') {
        if is_mbox_separator(line) {
            if !current_email.is_empty() {
                emails.push(std::mem::take(&mut current_email));
            }
            // Skip the "From " line
        } else {
            current_email.extend_from_slice(line);
        }
    }

    if !current_email.is_empty() {
        emails.push(current_email);
    }

    emails
}

pub fn is_mbox_separator(line: &[u8]) -> bool {
    if !line.starts_with(b"From ") {
        return false;
    }
    // Heuristic: Mbox separator lines (From_ lines) usually contain a timestamp.
    // We look for at least two colons (HH:MM:SS) to distinguish from
    // "From " starting a sentence in the body.
    line.iter().filter(|&&b| b == b':').count() >= 2
}

pub fn extract_message_id(raw_bytes: &[u8]) -> String {
    let raw_str = String::from_utf8_lossy(raw_bytes);
    for line in raw_str.lines() {
        if line.to_lowercase().starts_with("message-id:") {
            let val = line.split_once(':').map(|x| x.1).unwrap_or("").trim();
            // Remove brackets
            let clean = val.trim_start_matches('<').trim_end_matches('>');
            if !clean.is_empty() {
                return clean.to_string();
            }
        }
    }
    "unknown".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_mbox_separator() {
        assert!(is_mbox_separator(
            b"From user@example.com Mon Jan 1 00:00:00 2023\n"
        ));
        assert!(!is_mbox_separator(b"From: user@example.com\n"));
        assert!(!is_mbox_separator(b"Subject: Test\n"));
        assert!(!is_mbox_separator(b"Some body text\n"));
    }

    #[test]
    fn test_extract_message_id() {
        let email = b"Subject: Test\nMessage-ID: <12345@example.com>\n\nBody";
        assert_eq!(extract_message_id(email), "12345@example.com");

        let email_no_brackets = b"Subject: Test\nMessage-ID: 12345@example.com\n\nBody";
        assert_eq!(extract_message_id(email_no_brackets), "12345@example.com");

        let email_mixed_case = b"Subject: Test\nmessage-id: <12345@example.com>\n\nBody";
        assert_eq!(extract_message_id(email_mixed_case), "12345@example.com");

        let email_missing = b"Subject: Test\n\nBody";
        assert_eq!(extract_message_id(email_missing), "unknown");
    }

    #[test]
    fn test_split_mbox() {
        let mbox = b"From user@example.com Mon Jan 1 00:00:00 2023\n\
Subject: Patch 1\n\
Message-ID: <1@example.com>\n\
\n\
Body 1\n\
\n\
From user@example.com Mon Jan 1 00:00:01 2023\n\
Subject: Patch 2\n\
Message-ID: <2@example.com>\n\
\n\
Body 2\n";

        let messages = split_mbox(mbox);
        assert_eq!(messages.len(), 2);

        let msg1 = String::from_utf8_lossy(&messages[0]);
        assert!(msg1.contains("Subject: Patch 1"));
        assert!(msg1.contains("Body 1"));
        assert!(!msg1.contains("From user@example.com"));

        let msg2 = String::from_utf8_lossy(&messages[1]);
        assert!(msg2.contains("Subject: Patch 2"));
        assert!(msg2.contains("Body 2"));
        assert!(!msg2.contains("From user@example.com"));
    }

    #[test]
    fn test_split_mbox_single() {
        let mbox = b"From user@example.com Mon Jan 1 00:00:00 2023\n\
Subject: Patch 1\n\
Message-ID: <1@example.com>\n\
\n\
Body 1\n";

        let messages = split_mbox(mbox);
        assert_eq!(messages.len(), 1);
        let msg1 = String::from_utf8_lossy(&messages[0]);
        assert!(msg1.contains("Subject: Patch 1"));
    }

    #[test]
    fn test_split_git_format_patch() {
        let raw = b"From b99d70c0d1380f1368fd4a82271280c4fd28558b Mon Sep 17 00:00:00 2001
From: Tony Luck <tony.luck@intel.com>
Date: Wed, 25 Oct 2023 13:25:13 -0700
Subject: [PATCH 1/5] x86/cpu: Add model number for Intel Arrow Lake mobile
 processor

For \"reasons\" Intel has code-named this CPU with a \"_H\" suffix.

[ dhansen: As usual, apply this and send it upstream quickly to
\t   make it easier for anyone who is doing work that
\t   consumes this. ]

Signed-off-by: Tony Luck <tony.luck@intel.com>
Signed-off-by: Dave Hansen <dave.hansen@linux.intel.com>
Link: https://lore.kernel.org/all/20231025202513.12358-1-tony.luck%40intel.com
---
 arch/x86/include/asm/intel-family.h | 2 ++
 1 file changed, 2 insertions(+)

diff --git a/arch/x86/include/asm/intel-family.h b/arch/x86/include/asm/intel-family.h
index 5fcd85fd64fd..197316121f04 100644
--- a/arch/x86/include/asm/intel-family.h
+++ b/arch/x86/include/asm/intel-family.h
@@ -27,6 +27,7 @@
  *\t\t_X\t- regular server parts
  *\t\t_D\t- micro server parts
  *\t\t_N,_P\t- other mobile parts
+ *\t\t_H\t- premium mobile parts
  *\t\t_S\t- other client parts
  *
  *\t\tHistorical OPTDIFFs:
@@ -124,6 +125,7 @@
 #define INTEL_FAM6_METEORLAKE\t\t0xAC
 #define INTEL_FAM6_METEORLAKE_L\t\t0xAA

+#define INTEL_FAM6_ARROWLAKE_H\t\t0xC5
 #define INTEL_FAM6_ARROWLAKE\t\t0xC6

 #define INTEL_FAM6_LUNARLAKE_M\t\t0xBD
--
2.53.0.rc2.204.g2597b5adb4-goog


From 128b0c9781c9f2651bea163cb85e52a6c7be0f9e Mon Sep 17 00:00:00 2001
From: Thomas Gleixner <tglx@linutronix.de>
Date: Wed, 25 Oct 2023 23:04:15 +0200
Subject: [PATCH 2/5] x86/i8259: Skip probing when ACPI/MADT advertises PCAT
 compatibility

David and a few others reported that on certain newer systems some legacy
interrupts fail to work correctly.

Debugging revealed that the BIOS of these systems leaves the legacy PIC in
uninitialized state which makes the PIC detection fail and the kernel
switches to a dummy implementation.

Unfortunately this fallback causes quite some code to fail as it depends on
checks for the number of legacy PIC interrupts or the availability of the
real PIC.

In theory there is no reason to use the PIC on any modern system when
IO/APIC is available, but the dependencies on the related checks cannot be
resolved trivially and on short notice. This needs lots of analysis and
rework.

The PIC detection has been added to avoid quirky checks and force selection
of the dummy implementation all over the place, especially in VM guest
scenarios. So it's not an option to revert the relevant commit as that
would break a lot of other scenarios.

One solution would be to try to initialize the PIC on detection fail and
retry the detection, but that puts the burden on everything which does not
have a PIC.

Fortunately the ACPI/MADT table header has a flag field, which advertises
in bit 0 that the system is PCAT compatible, which means it has a legacy
8259 PIC.

Evaluate that bit and if set avoid the detection routine and keep the real
PIC installed, which then gets initialized (for nothing) and makes the rest
of the code with all the dependencies work again.

Fixes: e179f6914152 (\"x86, irq, pic: Probe for legacy PIC and set legacy_pic appropriately\")
Reported-by: David Lazar <dlazar@gmail.com>
Signed-off-by: Thomas Gleixner <tglx@linutronix.de>
Tested-by: David Lazar <dlazar@gmail.com>
Reviewed-by: Hans de Goede <hdegoede@redhat.com>
Reviewed-by: Mario Limonciello <mario.limonciello@amd.com>
Cc: stable@vger.kernel.org
Closes: https://bugzilla.kernel.org/show_bug.cgi?id=218003
Link: https://lore.kernel.org/r/875y2u5s8g.ffs@tglx
---
 arch/x86/include/asm/i8259.h |  2 ++
 arch/x86/kernel/acpi/boot.c  |  3 +++
 arch/x86/kernel/i8259.c      | 38 ++++++++++++++++++++++++++++--------
 3 files changed, 35 insertions(+), 8 deletions(-)

diff --git a/arch/x86/include/asm/i8259.h b/arch/x86/include/asm/i8259.h
--- a/arch/x86/include/asm/i8259.h
+++ b/arch/x86/include/asm/i8259.h
@@ -69,6 +69,8 @@ struct legacy_pic {
 \tvoid (*make_irq)(unsigned int irq);
 };

+void legacy_pic_pcat_compat(void);
+
 extern struct legacy_pic *legacy_pic;
 extern struct legacy_pic null_legacy_pic;
";

        let messages = split_mbox(raw);
        assert_eq!(messages.len(), 3);

        let msg1 = String::from_utf8_lossy(&messages[0]);
        assert!(msg1.contains(
            "Subject: [PATCH 1/5] x86/cpu: Add model number for Intel Arrow Lake mobile"
        ));
        assert!(msg1.contains("arch/x86/include/asm/intel-family.h | 2 ++"));

        let msg2 = String::from_utf8_lossy(&messages[1]);
        assert!(msg2.contains(
            "Subject: [PATCH 2/5] x86/i8259: Skip probing when ACPI/MADT advertises PCAT"
        ));
        assert!(
            msg2.contains("arch/x86/kernel/i8259.c      | 38 ++++++++++++++++++++++++++++--------")
        );

        let msg3 = String::from_utf8_lossy(&messages[2]);
        assert!(
            msg3.contains("Subject: [PATCH 3/5] x86/tsc: Defer marking TSC unstable to a worker")
        );
        assert!(msg3.contains("static DECLARE_WORK(tsc_sync_work, tsc_sync_mark_tsc_unstable);"));
    }

    #[test]
    fn test_extract_message_id_regression_no_brackets() {
        let raw = b"From: user\nMessage-ID: 12345@example.com\nSubject: Hi";
        assert_eq!(extract_message_id(raw), "12345@example.com");
    }
}
