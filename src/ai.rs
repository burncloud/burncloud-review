use anyhow::{anyhow, Context, Result};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde::Deserialize;
use serde_json::json;

use crate::models::{AiReviewReport, PullRequestData};

#[derive(Clone)]
pub struct AiClient {
    http: reqwest::Client,
    base_url: String,
    api_key: Option<String>,
    model: String,
}

impl AiClient {
    pub fn new(base_url: String, api_key: Option<String>, model: String) -> Result<Self> {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        if let Some(key) = api_key.as_deref().filter(|v| !v.trim().is_empty()) {
            headers.insert(
                AUTHORIZATION,
                HeaderValue::from_str(&format!("Bearer {key}"))
                    .context("invalid AI API key header")?,
            );
        }
        let http = reqwest::Client::builder()
            .default_headers(headers)
            .build()
            .context("build AI HTTP client")?;
        Ok(Self {
            http,
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key,
            model,
        })
    }

    pub fn endpoint_summary(&self) -> String {
        let auth = if self.api_key.is_some() { "auth" } else { "no-auth" };
        format!("{} · {} · {}", self.model, self.base_url, auth)
    }

    pub async fn review(&self, data: &PullRequestData) -> Result<AiReviewReport> {
        let url = format!("{}/chat/completions", self.base_url);
        let prompt = build_prompt(data);
        let body = json!({
            "model": self.model,
            "temperature": 0.1,
            "messages": [
                {
                    "role": "system",
                    "content": SYSTEM_PROMPT
                },
                {
                    "role": "user",
                    "content": prompt
                }
            ]
        });

        let response = self
            .http
            .post(url)
            .json(&body)
            .send()
            .await
            .context("request AI review")?
            .error_for_status()
            .context("AI endpoint rejected review request")?
            .json::<ChatResponse>()
            .await
            .context("decode AI response")?;

        let content = response
            .choices
            .first()
            .map(|choice| choice.message.content.as_str())
            .ok_or_else(|| anyhow!("AI response did not contain message content"))?;
        let json_text = extract_json(content)
            .ok_or_else(|| anyhow!("AI review did not return a JSON object"))?;
        serde_json::from_str(json_text).context("decode AI review report JSON")
    }
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatMessage,
}

#[derive(Debug, Deserialize)]
struct ChatMessage {
    content: String,
}

const SYSTEM_PROMPT: &str = r#"You are the independent reviewer for BurnCloud pull requests.
The coding agent is not trusted to review itself. Be adversarial, evidence-driven, and concise.

Review every change through five gates:
1. Scope: did the implementation stay inside the stated change boundary?
2. Code: correctness, concurrency, error handling, security, resource cleanup, performance, compatibility.
3. Behavior: which runtime/user-visible execution paths change, including failure behavior?
4. Architecture: did a component take responsibilities that belong to another component?
5. Evidence: are CI/tests/evidence sufficient for the risk level?

Risk policy:
R0 docs-only.
R1 UI or low-impact tooling.
R2 node runtime/router/model/process/hardware changes.
R3 network control-plane/auth/security/identity changes.
R4 billing/settlement/clearing/wallet/ledger changes.
Choose the higher level when uncertain.

Finding severity:
BLOCKER = unsafe to merge.
MAJOR = material correctness/architecture/regression problem.
MINOR = real but non-blocking improvement.
NIT = style or low-value polish.

Do not invent source facts. If evidence is missing, say that evidence is missing instead of claiming a bug.
Only report a file/line when the supplied patch supports it.
Return exactly one JSON object and no Markdown fences, using this shape:
{
  "summary": "...",
  "risk": "R0|R1|R2|R3|R4",
  "merge_recommendation": "...",
  "gates": {
    "scope": {"status":"PASS|WARN|FAIL|PENDING","summary":"...","items":["..."]},
    "code": {"status":"PASS|WARN|FAIL|PENDING","summary":"...","items":["..."]},
    "behavior": {"status":"PASS|WARN|FAIL|PENDING","summary":"...","items":["..."]},
    "architecture": {"status":"PASS|WARN|FAIL|PENDING","summary":"...","items":["..."]},
    "evidence": {"status":"PASS|WARN|FAIL|PENDING","summary":"...","items":["..."]}
  },
  "affected_components": [
    {"name":"...","impact":"...","reason":"..."}
  ],
  "findings": [
    {
      "severity":"BLOCKER|MAJOR|MINOR|NIT",
      "category":"scope|code|behavior|architecture|evidence|security|performance",
      "path":"path/or/null",
      "line":123,
      "title":"...",
      "explanation":"...",
      "suggestion":"..."
    }
  ]
}
"#;

fn build_prompt(data: &PullRequestData) -> String {
    const TOTAL_PATCH_LIMIT: usize = 120_000;
    const PER_FILE_LIMIT: usize = 16_000;

    let mut out = String::new();
    out.push_str(&format!(
        "Repository: {}\nPR: #{} {}\nAuthor: {}\nState: {} draft={}\nBase: {}\nHead: {} @ {}\nStats: +{} -{} across {} files\nCI combined state: {}\n\nPR description:\n{}\n\nChanged files and patches:\n",
        data.repository,
        data.pr.number,
        data.pr.title,
        data.pr.user.login,
        data.pr.state,
        data.pr.draft,
        data.pr.base.name,
        data.pr.head.name,
        data.pr.head.sha,
        data.pr.additions,
        data.pr.deletions,
        data.pr.changed_files,
        data.ci.state,
        data.pr.body.as_deref().unwrap_or("<none>")
    ));

    let mut used = 0usize;
    let mut truncated = false;
    for file in &data.files {
        let header = format!(
            "\n=== FILE {} ({}, +{} -{}, {} changes) ===\n",
            file.filename, file.status, file.additions, file.deletions, file.changes
        );
        if used + header.len() >= TOTAL_PATCH_LIMIT {
            truncated = true;
            break;
        }
        out.push_str(&header);
        used += header.len();

        let patch = file.patch.as_deref().unwrap_or("<patch unavailable>");
        let slice = truncate_chars(patch, PER_FILE_LIMIT);
        if slice.len() < patch.len() {
            truncated = true;
        }
        if used + slice.len() >= TOTAL_PATCH_LIMIT {
            let remaining = TOTAL_PATCH_LIMIT.saturating_sub(used);
            out.push_str(truncate_chars(slice, remaining));
            truncated = true;
            break;
        }
        out.push_str(slice);
        out.push('\n');
        used += slice.len();
    }

    if truncated {
        out.push_str("\nNOTE: Some patch content was truncated for model context. Treat missing code as missing evidence.\n");
    }

    if !data.ci.statuses.is_empty() {
        out.push_str("\nCI contexts:\n");
        for status in &data.ci.statuses {
            out.push_str(&format!(
                "- {}: {} — {}\n",
                status.context,
                status.state,
                status.description.as_deref().unwrap_or("")
            ));
        }
    }
    out
}

fn truncate_chars(value: &str, max_bytes: usize) -> &str {
    if value.len() <= max_bytes {
        return value;
    }
    let mut end = max_bytes;
    while !value.is_char_boundary(end) {
        end = end.saturating_sub(1);
    }
    &value[..end]
}

fn extract_json(value: &str) -> Option<&str> {
    let start = value.find('{')?;
    let end = value.rfind('}')?;
    (end >= start).then_some(&value[start..=end])
}
