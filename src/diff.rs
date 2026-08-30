use regex::Regex;

use crate::models::{DiffHunk, DiffLine, DiffLineKind};

pub fn parse_patch(patch: &str) -> Vec<DiffHunk> {
    let header_re = Regex::new(r"^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@(.*)$")
        .expect("valid hunk regex");

    let mut hunks = Vec::new();
    let mut current: Option<DiffHunk> = None;
    let mut old_line = 0_u64;
    let mut new_line = 0_u64;

    for raw in patch.lines() {
        if let Some(caps) = header_re.captures(raw) {
            if let Some(hunk) = current.take() {
                hunks.push(hunk);
            }
            old_line = caps[1].parse().unwrap_or(0);
            new_line = caps[2].parse().unwrap_or(0);
            current = Some(DiffHunk {
                header: raw.to_string(),
                lines: Vec::new(),
            });
            continue;
        }

        let Some(hunk) = current.as_mut() else {
            continue;
        };

        let (kind, old, new) = if raw.starts_with('+') && !raw.starts_with("+++") {
            let n = new_line;
            new_line = new_line.saturating_add(1);
            (DiffLineKind::Add, None, Some(n))
        } else if raw.starts_with('-') && !raw.starts_with("---") {
            let o = old_line;
            old_line = old_line.saturating_add(1);
            (DiffLineKind::Remove, Some(o), None)
        } else if raw.starts_with(' ') {
            let o = old_line;
            let n = new_line;
            old_line = old_line.saturating_add(1);
            new_line = new_line.saturating_add(1);
            (DiffLineKind::Context, Some(o), Some(n))
        } else {
            (DiffLineKind::Meta, None, None)
        };

        hunk.lines.push(DiffLine {
            kind,
            old_line: old,
            new_line: new,
            content: raw.to_string(),
        });
    }

    if let Some(hunk) = current.take() {
        hunks.push(hunk);
    }

    hunks
}

pub fn changed_line_indexes(hunk: &DiffHunk) -> Vec<usize> {
    hunk.lines
        .iter()
        .enumerate()
        .filter_map(|(idx, line)| {
            matches!(line.kind, DiffLineKind::Add | DiffLineKind::Remove).then_some(idx)
        })
        .collect()
}
