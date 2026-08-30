use anyhow::{anyhow, Result};

use crate::{
    ai::AiClient,
    codex::CodexClient,
    models::{AiReviewReport, PullRequestData},
};

#[derive(Clone)]
pub enum ReviewBackend {
    Codex(CodexClient),
    Http(AiClient),
}

pub struct ReviewBackendOptions {
    pub backend: String,
    pub codex_bin: Option<String>,
    pub codex_model: Option<String>,
    pub http_base_url: String,
    pub http_api_key: Option<String>,
    pub http_model: String,
}

impl ReviewBackend {
    pub fn from_options(options: ReviewBackendOptions) -> Result<Self> {
        match options.backend.to_ascii_lowercase().as_str() {
            "auto" => {
                match CodexClient::discover(options.codex_bin.clone(), options.codex_model.clone())
                {
                    Ok(codex) => Ok(Self::Codex(codex)),
                    Err(_) => Ok(Self::Http(AiClient::new(
                        options.http_base_url,
                        options.http_api_key,
                        options.http_model,
                    )?)),
                }
            }
            "codex" => Ok(Self::Codex(CodexClient::discover(
                options.codex_bin,
                options.codex_model,
            )?)),
            "http" => Ok(Self::Http(AiClient::new(
                options.http_base_url,
                options.http_api_key,
                options.http_model,
            )?)),
            other => Err(anyhow!(
                "unknown --ai-backend '{other}'; expected auto, codex, or http"
            )),
        }
    }

    pub fn summary(&self) -> String {
        match self {
            Self::Codex(client) => client.summary(),
            Self::Http(client) => client.endpoint_summary(),
        }
    }

    pub async fn review(&self, data: &PullRequestData) -> Result<AiReviewReport> {
        match self {
            Self::Codex(client) => client.review(data).await,
            Self::Http(client) => client.review(data).await,
        }
    }
}
