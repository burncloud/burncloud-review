use anyhow::{Context, Result};
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION, USER_AGENT};

use crate::models::{ChangedFile, CombinedStatus, PullRequest, PullRequestData, RecentPullRequest};

#[derive(Clone)]
pub struct GitHubClient {
    http: reqwest::Client,
}

impl GitHubClient {
    pub fn new(token: Option<&str>) -> Result<Self> {
        let mut headers = HeaderMap::new();
        headers.insert(USER_AGENT, HeaderValue::from_static("burncloud-review"));
        headers.insert(
            ACCEPT,
            HeaderValue::from_static("application/vnd.github+json"),
        );
        if let Some(token) = token.filter(|v| !v.trim().is_empty()) {
            let value = HeaderValue::from_str(&format!("Bearer {token}"))
                .context("invalid GitHub token header")?;
            headers.insert(AUTHORIZATION, value);
        }

        let http = reqwest::Client::builder()
            .default_headers(headers)
            .build()
            .context("build GitHub HTTP client")?;
        Ok(Self { http })
    }

    pub async fn load_recent_pull_requests(
        &self,
        repository: &str,
        limit: usize,
    ) -> Result<Vec<RecentPullRequest>> {
        let limit = limit.clamp(1, 100);
        let url = format!(
            "https://api.github.com/repos/{repository}/pulls?state=all&sort=updated&direction=desc&per_page={limit}"
        );
        self.http
            .get(url)
            .send()
            .await
            .context("request recent pull requests")?
            .error_for_status()
            .context("GitHub rejected recent pull request request")?
            .json()
            .await
            .context("decode recent pull requests")
    }

    pub async fn load_pull_request(
        &self,
        repository: &str,
        number: u64,
    ) -> Result<PullRequestData> {
        let pr_url = format!("https://api.github.com/repos/{repository}/pulls/{number}");
        let pr: PullRequest = self
            .http
            .get(&pr_url)
            .send()
            .await
            .context("request pull request metadata")?
            .error_for_status()
            .context("GitHub rejected pull request metadata request")?
            .json()
            .await
            .context("decode pull request metadata")?;

        let files = self.load_files(repository, number).await?;

        Ok(PullRequestData {
            repository: repository.to_string(),
            pr,
            files,
            ci: CombinedStatus {
                state: "not_run".into(),
                statuses: Vec::new(),
            },
        })
    }

    async fn load_files(&self, repository: &str, number: u64) -> Result<Vec<ChangedFile>> {
        let mut files = Vec::new();
        for page in 1..=50 {
            let url = format!(
                "https://api.github.com/repos/{repository}/pulls/{number}/files?per_page=100&page={page}"
            );
            let batch: Vec<ChangedFile> = self
                .http
                .get(&url)
                .send()
                .await
                .with_context(|| format!("request changed files page {page}"))?
                .error_for_status()
                .context("GitHub rejected changed files request")?
                .json()
                .await
                .context("decode changed files")?;
            let len = batch.len();
            files.extend(batch);
            if len < 100 {
                break;
            }
        }
        Ok(files)
    }
}
