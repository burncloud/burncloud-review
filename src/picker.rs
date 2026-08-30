use crate::models::RecentPullRequest;

pub struct PrPicker {
    pub repository: String,
    pub prs: Vec<RecentPullRequest>,
    pub selected: usize,
    pub status: String,
}

impl PrPicker {
    pub fn new(repository: String, prs: Vec<RecentPullRequest>) -> Self {
        let status = if prs.is_empty() {
            "No pull requests found. Press r to refresh or q to quit.".into()
        } else {
            format!("{} recent pull requests. Enter to review.", prs.len())
        };
        Self {
            repository,
            prs,
            selected: 0,
            status,
        }
    }

    pub fn replace(&mut self, prs: Vec<RecentPullRequest>) {
        self.prs = prs;
        self.selected = self.selected.min(self.prs.len().saturating_sub(1));
        self.status = if self.prs.is_empty() {
            "No pull requests found. Press r to refresh or q to quit.".into()
        } else {
            format!("Refreshed {} recent pull requests.", self.prs.len())
        };
    }

    pub fn move_up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    pub fn move_down(&mut self) {
        if self.selected + 1 < self.prs.len() {
            self.selected += 1;
        }
    }

    pub fn selected_number(&self) -> Option<u64> {
        self.prs.get(self.selected).map(|pr| pr.number)
    }

    pub fn selected_pr(&self) -> Option<&RecentPullRequest> {
        self.prs.get(self.selected)
    }

    pub fn detail_text(&self) -> String {
        let Some(pr) = self.selected_pr() else {
            return "No pull request is currently available.".into();
        };
        format!(
            "PR #{} — {}\n\nState: {}\nAuthor: {}\nBase: {}\nHead: {}\nUpdated: {}\n\n{}",
            pr.number,
            pr.title,
            pr.state_label(),
            pr.user.login,
            pr.base.name,
            pr.head.name,
            if pr.updated_at.is_empty() {
                "unknown"
            } else {
                &pr.updated_at
            },
            pr.body.as_deref().unwrap_or("No PR description supplied.")
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{GitHubUser, GitRef};

    fn pr(number: u64) -> RecentPullRequest {
        RecentPullRequest {
            number,
            title: format!("PR {number}"),
            body: None,
            state: "open".into(),
            draft: false,
            merged_at: None,
            updated_at: "2026-08-30T00:00:00Z".into(),
            user: GitHubUser {
                login: "fixture".into(),
            },
            base: GitRef {
                name: "main".into(),
                sha: "base".into(),
            },
            head: GitRef {
                name: "feature".into(),
                sha: "head".into(),
            },
        }
    }

    #[test]
    fn moves_and_selects_recent_prs() {
        let mut picker = PrPicker::new("burncloud/burncloud".into(), vec![pr(10), pr(9)]);
        assert_eq!(picker.selected_number(), Some(10));
        picker.move_down();
        assert_eq!(picker.selected_number(), Some(9));
        picker.move_up();
        assert_eq!(picker.selected_number(), Some(10));
    }
}
