use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use anyhow::{anyhow, Result};

use crate::{
    ai::AiClient,
    codex::CodexClient,
    models::{AiReviewReport, PullRequestData},
};

#[derive(Clone)]
pub enum ReviewBackend {
    Codex(CodexClient),
    Http { client: AiClient, timeout: Duration },
}

pub struct ReviewBackendOptions {
    pub backend: String,
    pub codex_bin: Option<String>,
    pub codex_model: Option<String>,
    pub http_base_url: String,
    pub http_api_key: Option<String>,
    pub http_model: String,
    pub review_timeout_secs: u64,
}

impl ReviewBackend {
    pub fn from_options(options: ReviewBackendOptions) -> Result<Self> {
        let timeout = Duration::from_secs(options.review_timeout_secs.max(1));
        match options.backend.to_ascii_lowercase().as_str() {
            "auto" => match CodexClient::discover(
                options.codex_bin.clone(),
                options.codex_model.clone(),
                timeout,
            ) {
                Ok(codex) => Ok(Self::Codex(codex)),
                Err(_) => Ok(Self::Http {
                    client: AiClient::new(
                        options.http_base_url,
                        options.http_api_key,
                        options.http_model,
                    )?,
                    timeout,
                }),
            },
            "codex" => Ok(Self::Codex(CodexClient::discover(
                options.codex_bin,
                options.codex_model,
                timeout,
            )?)),
            "http" => Ok(Self::Http {
                client: AiClient::new(
                    options.http_base_url,
                    options.http_api_key,
                    options.http_model,
                )?,
                timeout,
            }),
            other => Err(anyhow!(
                "unknown --ai-backend '{other}'; expected auto, codex, or http"
            )),
        }
    }

    pub fn summary(&self) -> String {
        match self {
            Self::Codex(client) => client.summary(),
            Self::Http { client, .. } => client.endpoint_summary(),
        }
    }

    pub fn timeout_secs(&self) -> u64 {
        match self {
            Self::Codex(client) => client.timeout_secs(),
            Self::Http { timeout, .. } => timeout.as_secs(),
        }
    }

    pub async fn review(
        &self,
        data: &PullRequestData,
        cancel: Arc<AtomicBool>,
    ) -> Result<AiReviewReport> {
        match self {
            Self::Codex(client) => client.review(data, cancel).await,
            Self::Http { client, timeout } => {
                let cancel_watch = async {
                    loop {
                        if cancel.load(Ordering::Relaxed) {
                            break;
                        }
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                };

                tokio::select! {
                    result = client.review(data) => result,
                    _ = tokio::time::sleep(*timeout) => Err(anyhow!(
                        "AI 审查超过 {} 秒，已自动停止",
                        timeout.as_secs()
                    )),
                    _ = cancel_watch => Err(anyhow!("AI 审查已取消")),
                }
            }
        }
    }
}
