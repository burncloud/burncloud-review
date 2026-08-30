use std::collections::HashSet;

use crate::{
    diff::{changed_line_indexes, parse_patch},
    models::{
        AiReviewReport, ComponentImpact, DiffHunk, DiffLineKind, GateKind, GateReview, GateStatus,
        PullRequestData, RiskLevel, Severity,
    },
    review::{classify_risk, infer_components},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Tree,
    Detail,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NodeId {
    Root,
    Gates,
    Gate(GateKind),
    Components,
    Component(usize),
    Files,
    File(usize),
    Hunk(usize, usize),
    Line(usize, usize, usize),
    Findings,
    Finding(usize),
}

#[derive(Debug, Clone)]
pub struct TreeEntry {
    pub id: NodeId,
    pub depth: usize,
    pub label: String,
    pub expandable: bool,
    pub expanded: bool,
}

pub struct App {
    pub data: PullRequestData,
    pub risk: RiskLevel,
    pub report: Option<AiReviewReport>,
    pub fallback_components: Vec<ComponentImpact>,
    pub hunks: Vec<Vec<DiffHunk>>,
    pub expanded: HashSet<NodeId>,
    pub selected: usize,
    pub focus: Focus,
    pub detail_scroll: u16,
    pub status: String,
    pub show_help: bool,
    pub busy: bool,
    pub should_quit: bool,
}

impl App {
    pub fn new(data: PullRequestData) -> Self {
        let risk = classify_risk(&data.files);
        let fallback_components = infer_components(&data.files);
        let hunks = parse_hunks(&data);
        let mut expanded = HashSet::new();
        expanded.insert(NodeId::Root);
        Self {
            data,
            risk,
            report: None,
            fallback_components,
            hunks,
            expanded,
            selected: 0,
            focus: Focus::Tree,
            detail_scroll: 0,
            status: "Ready. Press a to run independent AI review.".into(),
            show_help: false,
            busy: false,
            should_quit: false,
        }
    }

    pub fn replace_data(&mut self, data: PullRequestData) {
        self.data = data;
        self.risk = classify_risk(&self.data.files);
        self.fallback_components = infer_components(&self.data.files);
        self.hunks = parse_hunks(&self.data);
        self.report = None;
        self.expanded.clear();
        self.expanded.insert(NodeId::Root);
        self.selected = 0;
        self.detail_scroll = 0;
        self.status = "Pull request refreshed. AI report cleared; press a to review again.".into();
    }

    pub fn set_report(&mut self, report: AiReviewReport) {
        self.risk = report.risk;
        self.report = Some(report);
        self.status = "AI review complete. Findings are evidence-linked where possible.".into();
        self.ensure_selected_valid();
    }

    pub fn components(&self) -> &[ComponentImpact] {
        match self.report.as_ref() {
            Some(report) if !report.affected_components.is_empty() => &report.affected_components,
            _ => &self.fallback_components,
        }
    }

    pub fn tree_entries(&self) -> Vec<TreeEntry> {
        let mut entries = Vec::new();
        let root_label = format!(
            "PR #{}  [{}] {}",
            self.data.pr.number, self.risk, self.data.pr.title
        );
        self.push_entry(&mut entries, NodeId::Root, 0, root_label, true);
        if !self.expanded.contains(&NodeId::Root) {
            return entries;
        }

        self.push_entry(&mut entries, NodeId::Gates, 1, "Review Gates".into(), true);
        if self.expanded.contains(&NodeId::Gates) {
            for gate in GateKind::ALL {
                let status = self.gate_status(gate);
                self.push_entry(
                    &mut entries,
                    NodeId::Gate(gate),
                    2,
                    format!("[{}] {}", status_label(status), gate.title()),
                    false,
                );
            }
        }

        let components = self.components();
        self.push_entry(
            &mut entries,
            NodeId::Components,
            1,
            format!("Affected Components ({})", components.len()),
            !components.is_empty(),
        );
        if self.expanded.contains(&NodeId::Components) {
            for (idx, component) in components.iter().enumerate() {
                self.push_entry(
                    &mut entries,
                    NodeId::Component(idx),
                    2,
                    component.name.clone(),
                    false,
                );
            }
        }

        self.push_entry(
            &mut entries,
            NodeId::Files,
            1,
            format!("Changed Files ({})", self.data.files.len()),
            !self.data.files.is_empty(),
        );
        if self.expanded.contains(&NodeId::Files) {
            for (file_idx, file) in self.data.files.iter().enumerate() {
                let file_hunks = self.hunks.get(file_idx).map(Vec::as_slice).unwrap_or(&[]);
                self.push_entry(
                    &mut entries,
                    NodeId::File(file_idx),
                    2,
                    format!("{}  +{} -{}", file.filename, file.additions, file.deletions),
                    !file_hunks.is_empty(),
                );
                if self.expanded.contains(&NodeId::File(file_idx)) {
                    for (hunk_idx, hunk) in file_hunks.iter().enumerate() {
                        let changed = changed_line_indexes(hunk);
                        self.push_entry(
                            &mut entries,
                            NodeId::Hunk(file_idx, hunk_idx),
                            3,
                            hunk.header.clone(),
                            !changed.is_empty(),
                        );
                        if self.expanded.contains(&NodeId::Hunk(file_idx, hunk_idx)) {
                            for line_idx in changed {
                                let line = &hunk.lines[line_idx];
                                let number = line.new_line.or(line.old_line).unwrap_or(0);
                                let text = trim_label(&line.content, 72);
                                self.push_entry(
                                    &mut entries,
                                    NodeId::Line(file_idx, hunk_idx, line_idx),
                                    4,
                                    format!("L{} {}", number, text),
                                    false,
                                );
                            }
                        }
                    }
                }
            }
        }

        let finding_count = self.report.as_ref().map(|r| r.findings.len()).unwrap_or(0);
        self.push_entry(
            &mut entries,
            NodeId::Findings,
            1,
            format!("AI Findings ({finding_count})"),
            finding_count > 0,
        );
        if self.expanded.contains(&NodeId::Findings) {
            if let Some(report) = &self.report {
                for (idx, finding) in report.findings.iter().enumerate() {
                    self.push_entry(
                        &mut entries,
                        NodeId::Finding(idx),
                        2,
                        format!("[{}] {}", finding.severity, finding.title),
                        false,
                    );
                }
            }
        }

        entries
    }

    fn push_entry(
        &self,
        entries: &mut Vec<TreeEntry>,
        id: NodeId,
        depth: usize,
        label: String,
        expandable: bool,
    ) {
        entries.push(TreeEntry {
            id,
            depth,
            label,
            expandable,
            expanded: expandable && self.expanded.contains(&id),
        });
    }

    pub fn selected_id(&self) -> NodeId {
        let entries = self.tree_entries();
        entries
            .get(self.selected.min(entries.len().saturating_sub(1)))
            .map(|entry| entry.id)
            .unwrap_or(NodeId::Root)
    }

    pub fn move_up(&mut self) {
        if self.focus == Focus::Detail {
            self.detail_scroll = self.detail_scroll.saturating_sub(1);
            return;
        }
        self.selected = self.selected.saturating_sub(1);
        self.detail_scroll = 0;
    }

    pub fn move_down(&mut self) {
        if self.focus == Focus::Detail {
            self.detail_scroll = self.detail_scroll.saturating_add(1);
            return;
        }
        let len = self.tree_entries().len();
        if self.selected + 1 < len {
            self.selected += 1;
            self.detail_scroll = 0;
        }
    }

    pub fn page_up(&mut self) {
        self.detail_scroll = self.detail_scroll.saturating_sub(10);
    }

    pub fn page_down(&mut self) {
        self.detail_scroll = self.detail_scroll.saturating_add(10);
    }

    pub fn expand_selected(&mut self) {
        if self.focus == Focus::Detail {
            self.detail_scroll = self.detail_scroll.saturating_add(4);
            return;
        }
        let entries = self.tree_entries();
        let Some(entry) = entries.get(self.selected) else {
            return;
        };
        if entry.expandable && !entry.expanded {
            self.expanded.insert(entry.id);
            return;
        }
        if entry.expanded {
            if let Some((idx, _)) = entries
                .iter()
                .enumerate()
                .skip(self.selected + 1)
                .find(|(_, candidate)| candidate.depth == entry.depth + 1)
            {
                self.selected = idx;
                self.detail_scroll = 0;
            }
        }
    }

    pub fn collapse_selected(&mut self) {
        if self.focus == Focus::Detail {
            self.detail_scroll = self.detail_scroll.saturating_sub(4);
            return;
        }
        let entries = self.tree_entries();
        let Some(entry) = entries.get(self.selected) else {
            return;
        };
        if entry.expanded {
            self.expanded.remove(&entry.id);
            self.ensure_selected_valid();
            return;
        }
        let depth = entry.depth;
        if depth == 0 {
            return;
        }
        if let Some((idx, _)) = entries[..self.selected]
            .iter()
            .enumerate()
            .rev()
            .find(|(_, candidate)| candidate.depth < depth)
        {
            self.selected = idx;
            self.detail_scroll = 0;
        }
    }

    pub fn toggle_selected(&mut self) {
        if self.focus == Focus::Detail {
            return;
        }
        let id = self.selected_id();
        let expandable = self
            .tree_entries()
            .get(self.selected)
            .map(|entry| entry.expandable)
            .unwrap_or(false);
        if !expandable {
            return;
        }
        if self.expanded.contains(&id) {
            self.expanded.remove(&id);
        } else {
            self.expanded.insert(id);
        }
        self.ensure_selected_valid();
    }

    pub fn toggle_focus(&mut self) {
        self.focus = match self.focus {
            Focus::Tree => Focus::Detail,
            Focus::Detail => Focus::Tree,
        };
    }

    pub fn gate_status(&self, kind: GateKind) -> GateStatus {
        if let Some(report) = &self.report {
            return report.gates.get(kind).status;
        }
        if kind == GateKind::Evidence {
            return match self.data.ci.state.as_str() {
                "success" => GateStatus::Pass,
                "failure" | "error" => GateStatus::Fail,
                "pending" => GateStatus::Pending,
                _ => GateStatus::Warn,
            };
        }
        GateStatus::Pending
    }

    pub fn gate_review(&self, kind: GateKind) -> Option<&GateReview> {
        self.report.as_ref().map(|r| r.gates.get(kind))
    }

    pub fn detail_text(&self) -> String {
        match self.selected_id() {
            NodeId::Root => self.root_detail(),
            NodeId::Gates => self.gates_detail(),
            NodeId::Gate(kind) => self.gate_detail(kind),
            NodeId::Components => self.components_detail(),
            NodeId::Component(idx) => self.component_detail(idx),
            NodeId::Files => self.files_detail(),
            NodeId::File(idx) => self.file_detail(idx),
            NodeId::Hunk(file_idx, hunk_idx) => self.hunk_detail(file_idx, hunk_idx),
            NodeId::Line(file_idx, hunk_idx, line_idx) => {
                self.line_detail(file_idx, hunk_idx, line_idx)
            }
            NodeId::Findings => self.findings_detail(),
            NodeId::Finding(idx) => self.finding_detail(idx),
        }
    }

    fn root_detail(&self) -> String {
        let pr = &self.data.pr;
        let mut text = format!(
            "PR #{} — {}\n\nRepository: {}\nAuthor: {}\nState: {}{}\nBase: {}\nHead: {} @ {}\nRisk: {}\nCI: {}\nChanges: +{} -{} across {} files\n\n{}",
            pr.number,
            pr.title,
            self.data.repository,
            pr.user.login,
            pr.state,
            if pr.draft { " (draft)" } else { "" },
            pr.base.name,
            pr.head.name,
            pr.head.sha,
            self.risk,
            self.data.ci.state,
            pr.additions,
            pr.deletions,
            pr.changed_files,
            pr.body.as_deref().unwrap_or("No PR description supplied.")
        );
        if let Some(report) = &self.report {
            text.push_str(&format!(
                "\n\nAI REVIEW SUMMARY\n{}\n\nMERGE RECOMMENDATION\n{}",
                report.summary, report.merge_recommendation
            ));
        } else {
            text.push_str("\n\nAI review has not run yet. Press 'a' to run an independent review.");
        }
        text
    }

    fn gates_detail(&self) -> String {
        let mut text = String::from(
            "REVIEW GATES\n\nA PR is mergeable only when its risk-relevant gates have enough evidence.\n\n",
        );
        for kind in GateKind::ALL {
            text.push_str(&format!(
                "[{}] {}\n",
                status_label(self.gate_status(kind)),
                kind.title()
            ));
        }
        text.push_str("\nExpand this node and inspect each gate independently.");
        text
    }

    fn gate_detail(&self, kind: GateKind) -> String {
        let status = self.gate_status(kind);
        let mut text = format!(
            "{} GATE\nStatus: {}\n\n",
            kind.title().to_uppercase(),
            status_label(status)
        );
        if let Some(review) = self.gate_review(kind) {
            text.push_str(&review.summary);
            if !review.items.is_empty() {
                text.push_str("\n\nEvidence / concerns:\n");
                for item in &review.items {
                    text.push_str(&format!("• {item}\n"));
                }
            }
        } else {
            text.push_str(match kind {
                GateKind::Scope => "AI review pending. Compare requested intent with changed files and reject unrelated edits.",
                GateKind::Code => "AI review pending. Inspect correctness, error paths, concurrency, cleanup, security and regressions.",
                GateKind::Behavior => "AI review pending. Identify changed request/runtime paths and failure behavior.",
                GateKind::Architecture => "AI review pending. Verify component responsibilities and dependency direction.",
                GateKind::Evidence => "AI review pending. Raw GitHub commit status is shown below.",
            });
        }
        if kind == GateKind::Evidence {
            text.push_str(&format!("\n\nCombined CI: {}\n", self.data.ci.state));
            for status in &self.data.ci.statuses {
                text.push_str(&format!(
                    "• {}: {} — {}\n",
                    status.context,
                    status.state,
                    status.description.as_deref().unwrap_or("")
                ));
            }
        }
        text
    }

    fn components_detail(&self) -> String {
        let mut text = String::from("AFFECTED COMPONENTS\n\n");
        for component in self.components() {
            text.push_str(&format!("• {} — {}\n", component.name, component.impact));
        }
        text.push_str("\nBefore AI review, this list is deterministic path inference. After AI review it becomes model-assisted impact analysis.");
        text
    }

    fn component_detail(&self, idx: usize) -> String {
        self.components()
            .get(idx)
            .map(|component| {
                format!(
                    "{}\n\nImpact: {}\n\nReason:\n{}",
                    component.name, component.impact, component.reason
                )
            })
            .unwrap_or_else(|| "Component is no longer available.".into())
    }

    fn files_detail(&self) -> String {
        format!(
            "CHANGED FILES\n\n{} files · +{} -{}\n\nExpand to inspect files, hunks, and changed lines without losing the PR-level context.",
            self.data.pr.changed_files, self.data.pr.additions, self.data.pr.deletions
        )
    }

    fn file_detail(&self, idx: usize) -> String {
        let Some(file) = self.data.files.get(idx) else {
            return "File is no longer available.".into();
        };
        let hunk_count = self.hunks.get(idx).map(Vec::len).unwrap_or(0);
        let mut text = format!(
            "{}\n\nStatus: {}\nChanges: +{} -{} ({} total)\nHunks: {}\n",
            file.filename, file.status, file.additions, file.deletions, file.changes, hunk_count
        );
        if let Some(report) = &self.report {
            let matching: Vec<_> = report
                .findings
                .iter()
                .filter(|finding| finding.path.as_deref() == Some(file.filename.as_str()))
                .collect();
            if !matching.is_empty() {
                text.push_str("\nAI findings for this file:\n");
                for finding in matching {
                    text.push_str(&format!("• [{}] {}\n", finding.severity, finding.title));
                }
            }
        }
        if file.patch.is_none() {
            text.push_str("\nPatch unavailable from GitHub (binary or too large). Treat this as missing review evidence.");
        }
        text
    }

    fn hunk_detail(&self, file_idx: usize, hunk_idx: usize) -> String {
        let Some(file) = self.data.files.get(file_idx) else {
            return "File is no longer available.".into();
        };
        let Some(hunk) = self.hunks.get(file_idx).and_then(|v| v.get(hunk_idx)) else {
            return "Hunk is no longer available.".into();
        };
        let mut text = format!("{}\n{}\n\n", file.filename, hunk.header);
        for line in &hunk.lines {
            let old = line.old_line.map(|n| n.to_string()).unwrap_or_default();
            let new = line.new_line.map(|n| n.to_string()).unwrap_or_default();
            text.push_str(&format!("{:>5} {:>5} {}\n", old, new, line.content));
        }
        text
    }

    fn line_detail(&self, file_idx: usize, hunk_idx: usize, line_idx: usize) -> String {
        let Some(file) = self.data.files.get(file_idx) else {
            return "File is no longer available.".into();
        };
        let Some(line) = self
            .hunks
            .get(file_idx)
            .and_then(|v| v.get(hunk_idx))
            .and_then(|h| h.lines.get(line_idx))
        else {
            return "Line is no longer available.".into();
        };
        let side = match line.kind {
            DiffLineKind::Add => "ADDED",
            DiffLineKind::Remove => "REMOVED",
            DiffLineKind::Context => "CONTEXT",
            DiffLineKind::Meta => "META",
        };
        let number = line.new_line.or(line.old_line).unwrap_or(0);
        let mut text = format!(
            "{}\nLine {} · {}\n\n{}",
            file.filename, number, side, line.content
        );
        if let Some(report) = &self.report {
            let matching: Vec<_> = report
                .findings
                .iter()
                .filter(|finding| {
                    finding.path.as_deref() == Some(file.filename.as_str())
                        && finding.line == Some(number)
                })
                .collect();
            if !matching.is_empty() {
                text.push_str("\n\nAI findings anchored here:\n");
                for finding in matching {
                    text.push_str(&format!(
                        "• [{}] {}\n  {}\n",
                        finding.severity, finding.title, finding.explanation
                    ));
                }
            }
        }
        text
    }

    fn findings_detail(&self) -> String {
        let Some(report) = &self.report else {
            return "AI FINDINGS\n\nNo AI review yet. Press 'a'.".into();
        };
        let counts = |severity| {
            report
                .findings
                .iter()
                .filter(|f| f.severity == severity)
                .count()
        };
        format!(
            "AI FINDINGS\n\nBLOCKER: {}\nMAJOR: {}\nMINOR: {}\nNIT: {}\n\nFindings are advisory evidence. The reviewer still owns the merge decision.",
            counts(Severity::Blocker),
            counts(Severity::Major),
            counts(Severity::Minor),
            counts(Severity::Nit)
        )
    }

    fn finding_detail(&self, idx: usize) -> String {
        let Some(finding) = self.report.as_ref().and_then(|r| r.findings.get(idx)) else {
            return "Finding is no longer available.".into();
        };
        format!(
            "[{}] {}\n\nCategory: {}\nLocation: {}{}\n\n{}\n\nSuggested direction:\n{}",
            finding.severity,
            finding.title,
            finding.category,
            finding.path.as_deref().unwrap_or("<not line-anchored>"),
            finding.line.map(|n| format!(":{n}")).unwrap_or_default(),
            finding.explanation,
            if finding.suggestion.is_empty() {
                "No specific fix supplied; verify the evidence manually."
            } else {
                &finding.suggestion
            }
        )
    }

    fn ensure_selected_valid(&mut self) {
        let len = self.tree_entries().len();
        self.selected = self.selected.min(len.saturating_sub(1));
    }
}

fn parse_hunks(data: &PullRequestData) -> Vec<Vec<DiffHunk>> {
    data.files
        .iter()
        .map(|file| file.patch.as_deref().map(parse_patch).unwrap_or_default())
        .collect()
}

fn status_label(status: GateStatus) -> &'static str {
    match status {
        GateStatus::Pending => "PENDING",
        GateStatus::Pass => "PASS",
        GateStatus::Warn => "WARN",
        GateStatus::Fail => "FAIL",
    }
}

fn trim_label(value: &str, max: usize) -> String {
    let value = value.trim();
    if value.chars().count() <= max {
        return value.to_string();
    }
    let prefix: String = value.chars().take(max.saturating_sub(1)).collect();
    format!("{prefix}…")
}
