use std::collections::BTreeSet;

use crate::models::{ChangedFile, ComponentImpact, RiskLevel};

pub fn classify_risk(files: &[ChangedFile]) -> RiskLevel {
    let mut risk = RiskLevel::R0;
    for file in files {
        let path = file.filename.to_ascii_lowercase();
        let candidate = if contains_any(
            &path,
            &[
                "billing",
                "settlement",
                "clearing",
                "wallet",
                "payment",
                "ledger",
            ],
        ) {
            RiskLevel::R4
        } else if contains_any(
            &path,
            &[
                "network",
                "auth",
                "security",
                "identity",
                "crypto",
                "permission",
            ],
        ) {
            RiskLevel::R3
        } else if contains_any(
            &path,
            &[
                "router",
                "runtime",
                "scheduler",
                "process",
                "model",
                "hardware",
                "server",
            ],
        ) {
            RiskLevel::R2
        } else if contains_any(&path, &["docs", "readme", ".md", ".github"]) {
            RiskLevel::R0
        } else {
            RiskLevel::R1
        };
        risk = max_risk(risk, candidate);
    }
    risk
}

pub fn infer_components(files: &[ChangedFile]) -> Vec<ComponentImpact> {
    let mut names = BTreeSet::new();
    for file in files {
        let p = file.filename.to_ascii_lowercase();
        if contains_any(&p, &["billing", "ledger", "settlement", "payment"]) {
            names.insert("Billing / Settlement");
        }
        if contains_any(&p, &["network", "provider", "peer", "p2p"]) {
            names.insert("BurnCloud Network");
        }
        if contains_any(&p, &["router", "gateway", "server", "api"]) {
            names.insert("API / Router");
        }
        if contains_any(&p, &["runtime", "llama", "vllm", "process"]) {
            names.insert("Runtime / Process");
        }
        if contains_any(&p, &["model", "download", "manifest", "resolver"]) {
            names.insert("Model Lifecycle");
        }
        if contains_any(&p, &["hardware", "gpu", "cuda"]) {
            names.insert("Hardware Detection");
        }
        if contains_any(&p, &["auth", "security", "identity", "permission"]) {
            names.insert("Identity / Security");
        }
        if contains_any(&p, &["client", "ui", "tui", "frontend"]) {
            names.insert("UI / Client");
        }
        if contains_any(&p, &["database", "migration", "sqlite", "postgres"]) {
            names.insert("Database / State");
        }
        if contains_any(&p, &["docs", ".md"]) {
            names.insert("Documentation");
        }
    }

    if names.is_empty() {
        names.insert("Unclassified");
    }

    names
        .into_iter()
        .map(|name| ComponentImpact {
            name: name.to_string(),
            impact: "STATIC PATH INFERENCE".to_string(),
            reason: "Inferred from changed file paths before AI review.".to_string(),
        })
        .collect()
}

fn contains_any(value: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| value.contains(needle))
}

pub fn max_risk(a: RiskLevel, b: RiskLevel) -> RiskLevel {
    use RiskLevel::*;
    match (a, b) {
        (R4, _) | (_, R4) => R4,
        (R3, _) | (_, R3) => R3,
        (R2, _) | (_, R2) => R2,
        (R1, _) | (_, R1) => R1,
        _ => R0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str) -> ChangedFile {
        ChangedFile {
            filename: path.into(),
            status: "modified".into(),
            additions: 1,
            deletions: 1,
            changes: 2,
            patch: None,
        }
    }

    #[test]
    fn docs_only_is_r0() {
        assert_eq!(classify_risk(&[file("docs/node.md")]), RiskLevel::R0);
    }

    #[test]
    fn runtime_is_r2() {
        assert_eq!(
            classify_risk(&[file("crates/runtime/src/lib.rs")]),
            RiskLevel::R2
        );
    }

    #[test]
    fn network_security_is_r3() {
        assert_eq!(
            classify_risk(&[file("crates/network/src/auth.rs")]),
            RiskLevel::R3
        );
    }

    #[test]
    fn ledger_always_escalates_to_r4() {
        assert_eq!(
            classify_risk(&[
                file("docs/readme.md"),
                file("crates/ledger/src/settlement.rs"),
            ]),
            RiskLevel::R4
        );
    }

    #[test]
    fn model_review_cannot_lower_static_risk() {
        assert_eq!(max_risk(RiskLevel::R4, RiskLevel::R1), RiskLevel::R4);
        assert_eq!(max_risk(RiskLevel::R2, RiskLevel::R3), RiskLevel::R3);
    }
}
