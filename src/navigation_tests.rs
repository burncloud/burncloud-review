use crate::{
    app::{App, Focus, NodeId},
    models::{
        AiReviewReport, ChangedFile, CombinedStatus, Finding, GateReview, GateReviews, GateSection,
        GitHubUser, GitRef, PullRequest, PullRequestData, RiskLevel, Severity,
    },
};

fn sample_data() -> PullRequestData {
    PullRequestData {
        repository: "burncloud/burncloud".into(),
        pr: PullRequest {
            number: 123,
            title: "Add model runtime".into(),
            body: Some("Keep the change inside the runtime boundary.".into()),
            state: "open".into(),
            draft: false,
            additions: 2,
            deletions: 1,
            changed_files: 1,
            user: GitHubUser {
                login: "review-fixture".into(),
            },
            base: GitRef {
                name: "main".into(),
                sha: "base".into(),
            },
            head: GitRef {
                name: "feature/runtime".into(),
                sha: "head".into(),
            },
        },
        files: vec![ChangedFile {
            filename: "crates/runtime/src/lib.rs".into(),
            status: "modified".into(),
            additions: 2,
            deletions: 1,
            changes: 3,
            patch: Some("@@ -10,2 +10,3 @@\n-old\n+new\n+extra\n context".into()),
        }],
        ci: CombinedStatus {
            state: "success".into(),
            statuses: Vec::new(),
        },
    }
}

fn select(app: &mut App, id: NodeId) {
    app.selected = app
        .tree_entries()
        .iter()
        .position(|entry| entry.id == id)
        .expect("node must be visible");
}

#[test]
fn drills_from_pr_to_file_hunk_and_changed_line() {
    let mut app = App::new(sample_data());

    let initial = app.tree_entries();
    assert!(initial.iter().any(|entry| entry.id == NodeId::Files));
    assert!(!initial
        .iter()
        .any(|entry| matches!(entry.id, NodeId::File(_))));

    select(&mut app, NodeId::Files);
    app.expand_selected();
    assert!(app
        .tree_entries()
        .iter()
        .any(|entry| entry.id == NodeId::File(0)));

    select(&mut app, NodeId::File(0));
    app.expand_selected();
    assert!(app
        .tree_entries()
        .iter()
        .any(|entry| entry.id == NodeId::Hunk(0, 0)));

    select(&mut app, NodeId::Hunk(0, 0));
    app.expand_selected();
    assert!(app
        .tree_entries()
        .iter()
        .any(|entry| matches!(entry.id, NodeId::Line(0, 0, _))));

    app.collapse_selected();
    assert_eq!(app.selected_id(), NodeId::Hunk(0, 0));
    assert!(!app
        .tree_entries()
        .iter()
        .any(|entry| matches!(entry.id, NodeId::Line(0, 0, _))));

    app.collapse_selected();
    assert_eq!(app.selected_id(), NodeId::File(0));
}

#[test]
fn right_arrow_enters_detail_and_arrows_scroll() {
    let mut app = App::new(sample_data());
    app.expanded.insert(NodeId::Gates);
    select(&mut app, NodeId::Gate(crate::models::GateKind::Scope));

    assert_eq!(app.focus, Focus::Tree);
    app.expand_selected();
    assert_eq!(app.focus, Focus::Detail);

    app.move_down();
    app.move_down();
    assert_eq!(app.detail_scroll, 2);
    app.move_up();
    assert_eq!(app.detail_scroll, 1);

    app.collapse_selected();
    assert_eq!(app.focus, Focus::Tree);
}

#[test]
fn deep_gate_sections_are_renderable() {
    let review = GateReview {
        summary: "核心逻辑总体可行。".into(),
        sections: vec![GateSection {
            title: "错误与异常路径".into(),
            conclusion: "需要验证失败路径。".into(),
            evidence: vec!["crates/runtime/src/lib.rs 修改了错误分支。".into()],
        }],
        missing_evidence: vec!["缺少失败路径测试。".into()],
        ..GateReview::default()
    };

    let text = review.detailed_text_cn();
    assert!(text.contains("错误与异常路径"));
    assert!(text.contains("crates/runtime/src/lib.rs"));
    assert!(text.contains("缺少失败路径测试"));
}

#[test]
fn ai_findings_become_an_interactive_review_layer() {
    let mut app = App::new(sample_data());
    app.set_report(AiReviewReport {
        summary: "One material issue found.".into(),
        risk: RiskLevel::R2,
        merge_recommendation: "Fix before merge.".into(),
        gates: GateReviews::default(),
        affected_components: Vec::new(),
        findings: vec![Finding {
            severity: Severity::Major,
            category: "code".into(),
            path: Some("crates/runtime/src/lib.rs".into()),
            line: Some(10),
            title: "Runtime error path".into(),
            explanation: "The supplied patch requires manual verification here.".into(),
            suggestion: "Add an explicit error-path test.".into(),
        }],
    });

    select(&mut app, NodeId::Findings);
    app.expand_selected();
    assert!(app
        .tree_entries()
        .iter()
        .any(|entry| entry.id == NodeId::Finding(0)));

    select(&mut app, NodeId::Finding(0));
    assert!(app.detail_text().contains("Runtime error path"));
}
