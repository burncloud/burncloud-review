use std::collections::BTreeSet;

use crate::models::{ChangedFile, ComponentImpact, RiskLevel};

pub fn classify_risk(files: &[ChangedFile]) -> RiskLevel {
    let mut risk = RiskLevel::R0;
    for file in files {
        let path = file.filename.to_ascii_lowercase();
        let candidate = if contains_any(
            &path,
            &["billing", "settlement", "clearing", "wallet", "payment", "ledger"],
        ) {
            RiskLevel::R4
        } else if contains_any(
            &path,
            &["network", "auth", "security", "identity", "crypto", "permission"],
        ) {
            RiskLevel::R3
        } else if contains_any(
            &path,
            &["router", "runtime", "scheduler", "process", "model", "hardware", "server"],
        ) {
            RiskLevel::R2
        } else if contains_any(&path, &["ui", "client", "tui", "frontend", "css"]) {
            RiskLevel::R1
        } else if !contains_any(&path, &["docs", "readme", ".md", ".github"]) {
            RiskLevel::R1
        } else {
            RiskLevel::R0
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

fn max_risk(a: RiskLevel, b: RiskLevel) -> RiskLevel {
    use RiskLevel::*;
    match (a, b) {
        (R4, _) | (_, R4) => R4,
        (R3, _) | (_, R3) => R3,
        (R2, _) | (_, R2) => R2,
        (R1, _) | (_, R1) => R1,
        _ => R0,
    }
}
