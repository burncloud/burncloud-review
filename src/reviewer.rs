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
    models::{
        AiReviewReport, ChangedFile, Finding, GateStatus, PullRequestData, RiskLevel, Severity,
    },
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
        let report = match self {
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
        }?;

        Ok(apply_execution_guardrails(report, data))
    }
}

fn apply_execution_guardrails(
    mut report: AiReviewReport,
    data: &PullRequestData,
) -> AiReviewReport {
    let findings = deterministic_execution_findings(&data.files);
    if findings.is_empty() {
        return report;
    }

    let has_blocker = findings
        .iter()
        .any(|finding| finding.severity == Severity::Blocker);
    let has_major = findings
        .iter()
        .any(|finding| finding.severity == Severity::Major);

    if has_blocker {
        report.risk = RiskLevel::R4;
        report.gates.code.status = GateStatus::Fail;
        report.gates.evidence.status = GateStatus::Fail;
    } else if has_major {
        if matches!(report.risk, RiskLevel::R0 | RiskLevel::R1 | RiskLevel::R2) {
            report.risk = RiskLevel::R3;
        }
        if report.gates.code.status == GateStatus::Pass {
            report.gates.code.status = GateStatus::Warn;
        }
        if report.gates.evidence.status == GateStatus::Pass {
            report.gates.evidence.status = GateStatus::Warn;
        }
    }

    let summary = static_guardrail_summary(&findings);
    report.gates.code.items.insert(0, summary.clone());
    report.gates.evidence.items.insert(0, summary);

    for finding in findings {
        if !report.findings.iter().any(|existing| {
            existing.path == finding.path && existing.title == finding.title
        }) {
            report.findings.push(finding);
        }
    }

    report
}

fn deterministic_execution_findings(files: &[ChangedFile]) -> Vec<Finding> {
    let mut findings = Vec::new();

    for file in files {
        let path = file.filename.replace('\\', "/").to_ascii_lowercase();
        let added = added_patch_text(file);

        if file.patch.is_none() {
            push_finding(
                &mut findings,
                Severity::Blocker,
                file,
                "缺少完整 Patch，禁止执行 PR",
                "该文件没有可供静态和模型审查的 Patch。在不能证明实际新增内容安全之前，任何本地 build/test 都可能执行未审查代码。",
                "保持 fail-closed；重新获取完整 Patch 后再进行安全预检。",
            );
            continue;
        }

        if is_automatic_execution_hook(&path) {
            push_finding(
                &mut findings,
                Severity::Blocker,
                file,
                "修改了自动执行钩子，禁止在宿主机运行",
                "该文件可能在 cargo/build/CI 初始化阶段自动执行，不需要测试代码显式调用即可获得执行机会。当前 Local CI 仍运行在宿主机，因此不能安全放行。",
                "必须使用无凭据、默认断网、可销毁的 Container/VM executor 后才能执行。",
            );
        }

        if is_script_or_workflow(&path) {
            push_finding(
                &mut findings,
                Severity::Blocker,
                file,
                "修改了脚本或 CI 执行入口，禁止在宿主机运行",
                "脚本和工作流文件可以直接定义命令执行、网络访问或凭据处理。当前宿主机 Local CI 不具备足够隔离。",
                "将该 PR 升级到 Disposable VM/强隔离 Sandbox；不要在管理员宿主机执行。",
            );
        }

        if path.ends_with("cargo.toml") || path.ends_with("cargo.lock") {
            push_finding(
                &mut findings,
                Severity::Major,
                file,
                "依赖或构建图发生变化，需要隔离执行",
                "Cargo 依赖变化可能引入新的 build script、proc-macro 或供应链执行面。仅凭普通源码 diff 无法证明依赖安装和构建阶段安全。",
                "在 Container/Disposable VM 中执行，并限制网络、凭据和宿主文件系统访问。",
            );
        }

        for pattern in EXECUTION_PATTERNS {
            if added.contains(pattern.needle) {
                push_finding(
                    &mut findings,
                    pattern.severity,
                    file,
                    pattern.title,
                    pattern.explanation,
                    pattern.suggestion,
                );
            }
        }
    }

    findings
}

fn added_patch_text(file: &ChangedFile) -> String {
    file.patch
        .as_deref()
        .unwrap_or_default()
        .lines()
        .filter(|line| line.starts_with('+') && !line.starts_with("+++"))
        .map(|line| line.trim_start_matches('+').to_ascii_lowercase())
        .collect::<Vec<_>>()
        .join("\n")
}

fn is_automatic_execution_hook(path: &str) -> bool {
    path.ends_with("build.rs")
        || path == ".cargo/config"
        || path == ".cargo/config.toml"
        || path.contains("/.cargo/config")
        || path.contains("proc-macro")
        || path.contains("proc_macro")
}

fn is_script_or_workflow(path: &str) -> bool {
    path.starts_with(".github/workflows/")
        || path.starts_with("scripts/")
        || path.ends_with(".ps1")
        || path.ends_with(".bat")
        || path.ends_with(".cmd")
        || path.ends_with(".sh")
}

fn push_finding(
    findings: &mut Vec<Finding>,
    severity: Severity,
    file: &ChangedFile,
    title: &str,
    explanation: &str,
    suggestion: &str,
) {
    if findings
        .iter()
        .any(|existing| existing.path.as_deref() == Some(&file.filename) && existing.title == title)
    {
        return;
    }
    findings.push(Finding {
        severity,
        category: "security".into(),
        path: Some(file.filename.clone()),
        line: None,
        title: title.into(),
        explanation: explanation.into(),
        suggestion: suggestion.into(),
    });
}

fn static_guardrail_summary(findings: &[Finding]) -> String {
    let blockers = findings
        .iter()
        .filter(|finding| finding.severity == Severity::Blocker)
        .count();
    let majors = findings
        .iter()
        .filter(|finding| finding.severity == Severity::Major)
        .count();
    format!(
        "确定性执行风控命中：BLOCKER {blockers} / MAJOR {majors}。该结果由静态规则产生，AI 无权降级。"
    )
}

struct ExecutionPattern {
    needle: &'static str,
    severity: Severity,
    title: &'static str,
    explanation: &'static str,
    suggestion: &'static str,
}

const EXECUTION_PATTERNS: &[ExecutionPattern] = &[
    ExecutionPattern {
        needle: "std::process::command",
        severity: Severity::Blocker,
        title: "新增子进程执行能力，禁止在宿主机运行",
        explanation: "新增代码可启动任意子进程；cargo test/build 一旦触达该路径，可能执行宿主机命令。",
        suggestion: "仅在无凭据、默认断网、可销毁的 VM/Container 中执行。",
    },
    ExecutionPattern {
        needle: "command::new",
        severity: Severity::Blocker,
        title: "新增命令执行能力，禁止在宿主机运行",
        explanation: "新增代码构造外部命令，属于明确的本机执行面。",
        suggestion: "升级到 Disposable VM，并限制网络、凭据和宿主挂载。",
    },
    ExecutionPattern {
        needle: "powershell",
        severity: Severity::Blocker,
        title: "新增 PowerShell 执行路径",
        explanation: "PowerShell 可以访问文件系统、注册表、网络和凭据，不能在管理员宿主机直接验证不可信 PR。",
        suggestion: "使用 Windows Sandbox/Hyper-V Disposable VM。",
    },
    ExecutionPattern {
        needle: "cmd.exe",
        severity: Severity::Blocker,
        title: "新增 Windows Shell 执行路径",
        explanation: "PR 新增了 Windows shell 执行能力。",
        suggestion: "使用 Disposable VM，且不要注入任何长期凭据。",
    },
    ExecutionPattern {
        needle: "bash -c",
        severity: Severity::Blocker,
        title: "新增 Shell 执行路径",
        explanation: "PR 新增了可解释任意 shell 字符串的执行面。",
        suggestion: "仅在强隔离 Sandbox 中执行。",
    },
    ExecutionPattern {
        needle: "sh -c",
        severity: Severity::Blocker,
        title: "新增 Shell 执行路径",
        explanation: "PR 新增了可解释任意 shell 字符串的执行面。",
        suggestion: "仅在强隔离 Sandbox 中执行。",
    },
    ExecutionPattern {
        needle: "remove_dir_all",
        severity: Severity::Blocker,
        title: "新增递归目录删除能力",
        explanation: "代码可递归删除文件系统目录，对宿主工作区存在直接破坏能力。",
        suggestion: "禁止宿主机执行；使用一次性文件系统。",
    },
    ExecutionPattern {
        needle: "invoke-webrequest",
        severity: Severity::Blocker,
        title: "新增网络下载执行链",
        explanation: "代码或脚本可以从外部网络拉取未审查内容。",
        suggestion: "默认断网运行；确需网络时使用显式 allowlist。",
    },
    ExecutionPattern {
        needle: "reqwest::",
        severity: Severity::Major,
        title: "新增主动网络访问能力",
        explanation: "新增代码可主动访问网络；在宿主机 CI 中可能造成数据外传或下载二阶段内容。",
        suggestion: "使用默认断网 Container/VM，并按需开放目标域名。",
    },
    ExecutionPattern {
        needle: "tcpstream",
        severity: Severity::Major,
        title: "新增原始网络连接能力",
        explanation: "新增代码可以直接建立 TCP 连接，扩大执行时的数据外传面。",
        suggestion: "使用网络隔离 Sandbox。",
    },
    ExecutionPattern {
        needle: "std::env::var",
        severity: Severity::Major,
        title: "新增环境变量读取能力",
        explanation: "不可信代码读取环境变量时可能接触 GitHub Token、API Key、代理配置或其他宿主秘密。",
        suggestion: "Sandbox 中清空敏感环境变量，只注入最小必要配置。",
    },
    ExecutionPattern {
        needle: "github_token",
        severity: Severity::Blocker,
        title: "代码涉及 GitHub Token",
        explanation: "PR 新增内容直接引用 GitHub Token，存在凭据读取或外传风险。",
        suggestion: "禁止宿主机执行；Sandbox 中不得注入真实 Token。",
    },
    ExecutionPattern {
        needle: "api_key",
        severity: Severity::Blocker,
        title: "代码涉及 API Key",
        explanation: "PR 新增内容直接引用 API Key，存在凭据访问风险。",
        suggestion: "禁止宿主机执行；使用空/伪造测试凭据。",
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        CombinedStatus, GitHubUser, GitRef, PullRequest, PullRequestData,
    };

    fn file(path: &str, patch: Option<&str>) -> ChangedFile {
        ChangedFile {
            filename: path.into(),
            status: "modified".into(),
            additions: 1,
            deletions: 0,
            changes: 1,
            patch: patch.map(str::to_string),
        }
    }

    fn data(files: Vec<ChangedFile>) -> PullRequestData {
        PullRequestData {
            repository: "burncloud/burncloud".into(),
            pr: PullRequest {
                number: 1,
                title: "test".into(),
                body: None,
                state: "open".into(),
                draft: false,
                additions: 1,
                deletions: 0,
                changed_files: files.len() as u64,
                user: GitHubUser { login: "u".into() },
                base: GitRef { name: "main".into(), sha: "base".into() },
                head: GitRef { name: "pr".into(), sha: "head".into() },
            },
            files,
            ci: CombinedStatus { state: "not_run".into(), statuses: vec![] },
        }
    }

    fn report() -> AiReviewReport {
        AiReviewReport {
            summary: "模型未发现执行风险".into(),
            risk: RiskLevel::R1,
            merge_recommendation: String::new(),
            gates: Default::default(),
            affected_components: vec![],
            findings: vec![],
        }
    }

    #[test]
    fn removed_dangerous_code_does_not_block() {
        let d = data(vec![file(
            "src/lib.rs",
            Some("@@ -1 +1 @@\n-std::process::Command::new(\"powershell\");\n+let ok = true;"),
        )]);
        let guarded = apply_execution_guardrails(report(), &d);
        assert!(guarded.findings.is_empty());
    }

    #[test]
    fn added_process_execution_becomes_blocker_even_if_ai_says_safe() {
        let d = data(vec![file(
            "src/lib.rs",
            Some("@@ -1 +1 @@\n+std::process::Command::new(\"powershell\");"),
        )]);
        let guarded = apply_execution_guardrails(report(), &d);
        assert_eq!(guarded.risk, RiskLevel::R4);
        assert!(guarded
            .findings
            .iter()
            .any(|finding| finding.severity == Severity::Blocker && finding.category == "security"));
    }

    #[test]
    fn build_rs_is_blocked_without_relying_on_model_judgement() {
        let d = data(vec![file("build.rs", Some("@@ -0,0 +1 @@\n+fn main() {}"))]);
        let guarded = apply_execution_guardrails(report(), &d);
        assert!(guarded
            .findings
            .iter()
            .any(|finding| finding.severity == Severity::Blocker));
    }

    #[test]
    fn dependency_change_requires_isolation() {
        let d = data(vec![file(
            "Cargo.toml",
            Some("@@ -1 +1 @@\n+some-crate = \"1\""),
        )]);
        let guarded = apply_execution_guardrails(report(), &d);
        assert!(guarded
            .findings
            .iter()
            .any(|finding| finding.severity == Severity::Major && finding.category == "security"));
    }
}
