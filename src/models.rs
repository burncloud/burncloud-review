use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
pub struct PullRequest {
    pub number: u64,
    pub title: String,
    pub body: Option<String>,
    pub state: String,
    pub draft: bool,
    pub additions: u64,
    pub deletions: u64,
    pub changed_files: u64,
    pub user: GitHubUser,
    pub base: GitRef,
    pub head: GitRef,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RecentPullRequest {
    pub number: u64,
    pub title: String,
    pub body: Option<String>,
    pub state: String,
    #[serde(default)]
    pub draft: bool,
    #[serde(default)]
    pub merged_at: Option<String>,
    #[serde(default)]
    pub updated_at: String,
    pub user: GitHubUser,
    pub base: GitRef,
    pub head: GitRef,
}

impl RecentPullRequest {
    pub fn state_label(&self) -> &'static str {
        if self.merged_at.is_some() {
            "MERGED"
        } else if self.draft {
            "DRAFT"
        } else if self.state.eq_ignore_ascii_case("open") {
            "OPEN"
        } else {
            "CLOSED"
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct GitHubUser {
    pub login: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GitRef {
    #[serde(rename = "ref")]
    pub name: String,
    pub sha: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChangedFile {
    pub filename: String,
    pub status: String,
    pub additions: u64,
    pub deletions: u64,
    pub changes: u64,
    pub patch: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CombinedStatus {
    pub state: String,
    #[serde(default)]
    pub statuses: Vec<CommitStatus>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CommitStatus {
    pub state: String,
    pub context: String,
    pub description: Option<String>,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub duration_ms: Option<u64>,
    #[serde(default)]
    pub exit_code: Option<i32>,
    #[serde(default)]
    pub output: Option<String>,
}

impl CommitStatus {
    pub fn evidence_text(&self) -> String {
        if let Some(description) = &self.description {
            if self.command.is_some() && description.contains("\n命令: ") {
                return description.clone();
            }
        }

        let mut text = self.description.clone().unwrap_or_default();
        if let Some(command) = &self.command {
            if !text.is_empty() {
                text.push('\n');
            }
            text.push_str(&format!("命令: {command}"));
        }
        if let Some(duration_ms) = self.duration_ms {
            text.push_str(&format!("\n耗时: {:.1}s", duration_ms as f64 / 1000.0));
        }
        if let Some(exit_code) = self.exit_code {
            text.push_str(&format!("\n退出码: {exit_code}"));
        }
        if let Some(output) = &self.output {
            text.push_str("\n输出:\n");
            if output.trim().is_empty() {
                text.push_str("<无输出>");
            } else {
                text.push_str(output.trim());
            }
        }
        text
    }
}

#[derive(Debug, Clone)]
pub struct PullRequestData {
    pub repository: String,
    pub pr: PullRequest,
    pub files: Vec<ChangedFile>,
    pub ci: CombinedStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RiskLevel {
    R0,
    R1,
    R2,
    R3,
    R4,
}

impl std::fmt::Display for RiskLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "UPPERCASE")]
pub enum GateStatus {
    #[default]
    Pending,
    Pass,
    Warn,
    Fail,
}

impl std::fmt::Display for GateStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GateKind {
    Scope,
    Code,
    Behavior,
    Architecture,
    Evidence,
}

impl GateKind {
    pub const ALL: [GateKind; 5] = [
        GateKind::Scope,
        GateKind::Code,
        GateKind::Behavior,
        GateKind::Architecture,
        GateKind::Evidence,
    ];

    pub fn title(self) -> &'static str {
        match self {
            GateKind::Scope => "Scope",
            GateKind::Code => "Code",
            GateKind::Behavior => "Behavior",
            GateKind::Architecture => "Architecture",
            GateKind::Evidence => "Evidence",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GateSection {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub conclusion: String,
    #[serde(default)]
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GateReview {
    #[serde(default)]
    pub status: GateStatus,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub items: Vec<String>,
    #[serde(default)]
    pub sections: Vec<GateSection>,
    #[serde(default)]
    pub missing_evidence: Vec<String>,
}

impl GateReview {
    pub fn detailed_text_cn(&self) -> String {
        let mut text = String::new();
        if !self.summary.trim().is_empty() {
            text.push_str("结论摘要\n");
            text.push_str(self.summary.trim());
        }

        if !self.items.is_empty() {
            if !text.is_empty() {
                text.push_str("\n\n");
            }
            text.push_str("关键判断\n");
            for item in &self.items {
                text.push_str(&format!("• {item}\n"));
            }
        }

        for (index, section) in self.sections.iter().enumerate() {
            if !text.is_empty() {
                text.push('\n');
            }
            let title = if section.title.trim().is_empty() {
                format!("审查项 {}", index + 1)
            } else {
                section.title.trim().to_string()
            };
            text.push_str(&format!("\n━━ {title} ━━\n"));
            if !section.conclusion.trim().is_empty() {
                text.push_str(section.conclusion.trim());
                text.push('\n');
            }
            if !section.evidence.is_empty() {
                text.push_str("证据 / 推理依据:\n");
                for evidence in &section.evidence {
                    text.push_str(&format!("  • {evidence}\n"));
                }
            }
        }

        if !self.missing_evidence.is_empty() {
            if !text.is_empty() {
                text.push('\n');
            }
            text.push_str("\n━━ 缺失证据 / 仍需验证 ━━\n");
            for item in &self.missing_evidence {
                text.push_str(&format!("• {item}\n"));
            }
        }

        text
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GateReviews {
    #[serde(default)]
    pub scope: GateReview,
    #[serde(default)]
    pub code: GateReview,
    #[serde(default)]
    pub behavior: GateReview,
    #[serde(default)]
    pub architecture: GateReview,
    #[serde(default)]
    pub evidence: GateReview,
}

impl GateReviews {
    pub fn get(&self, kind: GateKind) -> &GateReview {
        match kind {
            GateKind::Scope => &self.scope,
            GateKind::Code => &self.code,
            GateKind::Behavior => &self.behavior,
            GateKind::Architecture => &self.architecture,
            GateKind::Evidence => &self.evidence,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentImpact {
    pub name: String,
    #[serde(default)]
    pub impact: String,
    #[serde(default)]
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Severity {
    Blocker,
    Major,
    Minor,
    Nit,
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    pub severity: Severity,
    pub category: String,
    pub path: Option<String>,
    pub line: Option<u64>,
    pub title: String,
    pub explanation: String,
    #[serde(default)]
    pub suggestion: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiReviewReport {
    pub summary: String,
    pub risk: RiskLevel,
    #[serde(default)]
    pub merge_recommendation: String,
    #[serde(default)]
    pub gates: GateReviews,
    #[serde(default)]
    pub affected_components: Vec<ComponentImpact>,
    #[serde(default)]
    pub findings: Vec<Finding>,
}

#[derive(Debug, Clone)]
pub struct DiffHunk {
    pub header: String,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffLineKind {
    Add,
    Remove,
    Context,
    Meta,
}

#[derive(Debug, Clone)]
pub struct DiffLine {
    pub kind: DiffLineKind,
    pub old_line: Option<u64>,
    pub new_line: Option<u64>,
    pub content: String,
}
