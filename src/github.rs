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

use anyhow::{Result, anyhow};
use reqwest::header::{ACCEPT, AUTHORIZATION, USER_AGENT};
use serde::Deserialize;
use tracing::debug;

#[derive(Debug, Deserialize)]
pub struct PullRequest {
    pub number: u64,
    pub title: String,
    pub user: GitHubUser,
    pub base: GitHubRef,
    pub head: GitHubRef,
    pub updated_at: String,
    pub state: String,
    pub merged_at: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct GitHubUser {
    pub login: String,
}

#[derive(Debug, Deserialize)]
pub struct GitHubRef {
    pub sha: String,
    #[serde(rename = "ref")]
    pub ref_name: String,
}

#[derive(Debug, Deserialize)]
pub struct GitHubCommit {
    pub sha: String,
    pub commit: GitHubCommitDetail,
}

#[derive(Debug, Deserialize)]
pub struct GitHubCommitDetail {
    pub message: String,
    pub author: GitHubCommitAuthor,
}

#[derive(Debug, Deserialize)]
pub struct GitHubCommitAuthor {
    pub name: String,
    pub email: String,
    pub date: String,
}

pub struct GitHubClient {
    owner: String,
    repo: String,
    token: Option<String>,
    client: reqwest::Client,
}

impl GitHubClient {
    pub fn new(owner: &str, repo: &str, token: Option<String>) -> Self {
        Self {
            owner: owner.to_string(),
            repo: repo.to_string(),
            token,
            client: reqwest::Client::new(),
        }
    }

    fn build_request(&self, url: &str) -> reqwest::RequestBuilder {
        let mut req = self
            .client
            .get(url)
            .header(USER_AGENT, "sashiko")
            .header(ACCEPT, "application/vnd.github.v3+json");
        if let Some(ref token) = self.token {
            req = req.header(AUTHORIZATION, format!("Bearer {}", token));
        }
        req
    }

    /// List open pull requests, sorted by recently updated.
    /// Returns up to `per_page` results (max 100).
    pub async fn list_open_prs(
        &self,
        per_page: u32,
        page: u32,
    ) -> Result<Vec<PullRequest>> {
        let url = format!(
            "https://api.github.com/repos/{}/{}/pulls?state=open&sort=updated&direction=desc&per_page={}&page={}",
            self.owner, self.repo, per_page, page
        );
        debug!("Fetching PRs: {}", url);
        let resp = self.build_request(&url).send().await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow!("GitHub API error {}: {}", status, body));
        }
        let prs: Vec<PullRequest> = resp.json().await?;
        Ok(prs)
    }

    /// Get commits for a specific PR.
    pub async fn get_pr_commits(
        &self,
        pr_number: u64,
    ) -> Result<Vec<GitHubCommit>> {
        let mut all_commits = Vec::new();
        let mut page = 1u32;
        loop {
            let url = format!(
                "https://api.github.com/repos/{}/{}/pulls/{}/commits?per_page=100&page={}",
                self.owner, self.repo, pr_number, page
            );
            debug!("Fetching PR commits: {}", url);
            let resp = self.build_request(&url).send().await?;
            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                return Err(anyhow!("GitHub API error {}: {}", status, body));
            }
            let commits: Vec<GitHubCommit> = resp.json().await?;
            let count = commits.len();
            all_commits.extend(commits);
            if count < 100 {
                break;
            }
            page += 1;
        }
        Ok(all_commits)
    }

    /// List closed pull requests, sorted by recently updated.
    /// Returns up to `per_page` results (max 100).
    pub async fn list_closed_prs(
        &self,
        per_page: u32,
        page: u32,
    ) -> Result<Vec<PullRequest>> {
        let url = format!(
            "https://api.github.com/repos/{}/{}/pulls?state=closed&sort=updated&direction=desc&per_page={}&page={}",
            self.owner, self.repo, per_page, page
        );
        debug!("Fetching closed PRs: {}", url);
        let resp = self.build_request(&url).send().await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow!("GitHub API error {}: {}", status, body));
        }
        let prs: Vec<PullRequest> = resp.json().await?;
        Ok(prs)
    }

    /// Get a single pull request by number.
    pub async fn get_pr(&self, pr_number: u64) -> Result<PullRequest> {
        let url = format!(
            "https://api.github.com/repos/{}/{}/pulls/{}",
            self.owner, self.repo, pr_number
        );
        let resp = self.build_request(&url).send().await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow!("GitHub API error {}: {}", status, body));
        }
        Ok(resp.json().await?)
    }
}
