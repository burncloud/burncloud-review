use crate::{
    app::{App, NodeId},
    models::{
        AiReviewReport, ChangedFile, CombinedStatus, Finding, GateReviews, GitHubUser, GitRef,
        PullRequest, PullRequestData, RiskLevel, Severity,
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
    assert!(!initial.iter().any(|entry| matches!(entry.id, NodeId::File(_))));

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
    assert_eq!(app.selected_id(), NodeId::File(0));
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
