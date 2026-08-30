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
pub struct GateReview {
    #[serde(default)]
    pub status: GateStatus,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub items: Vec<String>,
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
