use regex::Regex;

use crate::models::{DiffHunk, DiffLine, DiffLineKind};

pub fn parse_patch(patch: &str) -> Vec<DiffHunk> {
    let header_re =
        Regex::new(r"^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@(.*)$").expect("valid hunk regex");

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_line_numbers_across_add_remove_and_context() {
        let patch = "@@ -10,3 +10,4 @@ fn demo()\n context\n-old\n+new\n+extra";
        let hunks = parse_patch(patch);
        assert_eq!(hunks.len(), 1);
        assert_eq!(hunks[0].lines.len(), 4);

        let context = &hunks[0].lines[0];
        assert_eq!(context.old_line, Some(10));
        assert_eq!(context.new_line, Some(10));

        let removed = &hunks[0].lines[1];
        assert_eq!(removed.kind, DiffLineKind::Remove);
        assert_eq!(removed.old_line, Some(11));
        assert_eq!(removed.new_line, None);

        let added = &hunks[0].lines[2];
        assert_eq!(added.kind, DiffLineKind::Add);
        assert_eq!(added.new_line, Some(11));

        let changed = changed_line_indexes(&hunks[0]);
        assert_eq!(changed, vec![1, 2, 3]);
    }

    #[test]
    fn parses_multiple_hunks() {
        let patch = "@@ -1 +1 @@\n-a\n+b\n@@ -20 +21 @@\n-c\n+d";
        let hunks = parse_patch(patch);
        assert_eq!(hunks.len(), 2);
        assert_eq!(hunks[1].lines[0].old_line, Some(20));
        assert_eq!(hunks[1].lines[1].new_line, Some(21));
    }
}
