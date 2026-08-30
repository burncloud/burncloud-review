use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};

use crate::{
    app::{App, Focus, NodeId, TreeEntry},
    models::GateKind,
};

pub fn draw(frame: &mut Frame, app: &App, ai_endpoint: &str) {
    let area = frame.area().inner(Margin {
        horizontal: 1,
        vertical: 1,
    });
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),
            Constraint::Min(8),
            Constraint::Length(2),
        ])
        .split(area);

    draw_header(frame, app, ai_endpoint, vertical[0]);

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(44), Constraint::Percentage(56)])
        .split(vertical[1]);
    draw_tree(frame, app, body[0]);
    draw_detail(frame, app, body[1]);
    draw_footer(frame, app, vertical[2]);

    if app.show_help {
        draw_help(frame);
    }
}

fn draw_header(frame: &mut Frame, app: &App, ai_endpoint: &str, area: Rect) {
    let busy = if app.busy { " · WORKING" } else { "" };
    let title = format!(
        " BurnCloud Review · {} · PR #{} · Risk {}{} ",
        app.data.repository, app.data.pr.number, app.risk, busy
    );
    let text = Line::from(vec![
        Span::styled("AI: ", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(ai_endpoint),
    ]);
    let block = Block::default().title(title).borders(Borders::ALL);
    frame.render_widget(block, area);
    frame.render_widget(
        Paragraph::new(text).alignment(Alignment::Left),
        padded_panel_inner(area),
    );
}

fn draw_tree(frame: &mut Frame, app: &App, area: Rect) {
    let entries = app.tree_entries();
    let items: Vec<ListItem> = entries
        .iter()
        .map(|entry| {
            let indent = "  ".repeat(entry.depth);
            let marker = if entry.expandable {
                if entry.expanded {
                    "▾ "
                } else {
                    "▸ "
                }
            } else {
                "• "
            };
            let label = display_entry_label(app, entry);
            ListItem::new(Line::from(format!("{indent}{marker}{label}")))
        })
        .collect();

    let border = if app.focus == Focus::Tree {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default()
    };
    let block = Block::default()
        .title(" Review Tree ")
        .borders(Borders::ALL)
        .border_style(border);
    frame.render_widget(block, area);

    let list = List::new(items)
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");

    let mut state = ListState::default();
    state.select(Some(app.selected));
    frame.render_stateful_widget(list, padded_panel_inner(area), &mut state);
}

fn display_entry_label(app: &App, entry: &TreeEntry) -> String {
    let NodeId::Gate(kind) = entry.id else {
        return entry.label.clone();
    };
    if app.report.is_some() {
        return entry.label.clone();
    }
    format!("[{}] {}", pre_review_status(app, kind), kind.title())
}

fn pre_review_status(app: &App, kind: GateKind) -> &'static str {
    if kind == GateKind::Evidence {
        return match app.data.ci.state.to_ascii_lowercase().as_str() {
            "success" => "CI PASS",
            "failure" | "error" => "CI FAIL",
            "pending" | "expected" => "CI PENDING",
            _ => "CI UNKNOWN",
        };
    }
    if app.busy {
        "RUNNING"
    } else {
        "NOT RUN"
    }
}

fn draw_detail(frame: &mut Frame, app: &App, area: Rect) {
    let border = if app.focus == Focus::Detail {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default()
    };
    let block = Block::default()
        .title(" Evidence / Detail ")
        .borders(Borders::ALL)
        .border_style(border);
    frame.render_widget(block, area);

    let detail = Paragraph::new(display_detail_text(app))
        .wrap(Wrap { trim: false })
        .scroll((app.detail_scroll, 0));
    frame.render_widget(detail, padded_panel_inner(area));
}

fn display_detail_text(app: &App) -> String {
    if app.report.is_some() {
        return app.detail_text();
    }

    match app.selected_id() {
        NodeId::Gates => {
            let mut text = String::from(
                "REVIEW GATES\n\nThe four review gates do not run automatically. Press 'a' to start the independent Codex/AI review. Evidence shows GitHub CI separately.\n\n",
            );
            for kind in GateKind::ALL {
                text.push_str(&format!(
                    "[{}] {}\n",
                    pre_review_status(app, kind),
                    kind.title()
                ));
            }
            text.push_str("\nAfter the review finishes, statuses become PASS / WARN / FAIL.");
            text
        }
        NodeId::Gate(kind) => pre_review_gate_detail(app, kind),
        _ => app.detail_text(),
    }
}

fn pre_review_gate_detail(app: &App, kind: GateKind) -> String {
    let status = pre_review_status(app, kind);
    let explanation = match kind {
        GateKind::Scope => {
            "Checks whether the implementation stayed inside the requested change boundary."
        }
        GateKind::Code => {
            "Checks correctness, error paths, concurrency, cleanup, security and regressions."
        }
        GateKind::Behavior => {
            "Checks which runtime or user-visible execution paths changed, including failures."
        }
        GateKind::Architecture => {
            "Checks component responsibilities, dependency direction and architecture boundaries."
        }
        GateKind::Evidence => {
            "Shows deterministic GitHub CI evidence independently from the model review."
        }
    };

    let mut text = format!(
        "{} GATE\nStatus: {}\n\n{}",
        kind.title().to_uppercase(),
        status,
        explanation
    );
    if kind == GateKind::Evidence {
        text.push_str(&format!("\n\nCombined CI: {}", app.data.ci.state));
        for ci in &app.data.ci.statuses {
            text.push_str(&format!(
                "\n• {}: {} — {}",
                ci.context,
                ci.state,
                ci.description.as_deref().unwrap_or("")
            ));
        }
    } else if app.busy {
        text.push_str("\n\nIndependent review is currently running.");
    } else {
        text.push_str("\n\nNot reviewed yet. Press 'a' to run the independent review.");
    }
    text
}

fn draw_footer(frame: &mut Frame, app: &App, area: Rect) {
    let text = format!(
        "{}  |  ↑↓ move  ←→ layer  Enter expand  Tab focus  a AI review  r refresh  Esc PR list  ? help  q quit",
        app.status
    );
    frame.render_widget(
        Paragraph::new(text)
            .style(Style::default().fg(Color::DarkGray))
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn draw_help(frame: &mut Frame) {
    let area = centered_rect(72, 70, frame.area());
    frame.render_widget(Clear, area);
    let help = r#"BURNCloud REVIEW KEYBOARD

The left side is a hierarchy, not a flat file list.

↑ / ↓       Move through visible review nodes
←            Collapse current node; if already closed, go to parent
→            Expand current node; if already open, enter first child
Enter        Toggle current layer open / closed
Tab          Switch focus: review tree ↔ detail pane
PgUp/PgDn   Scroll evidence/detail by page
A            Run the independent AI/Codex review
R            Reload PR metadata, files and CI from GitHub
Esc          Return to the recent PR picker
?            Toggle this help
Q            Quit

Gate status before review:
NOT RUN      Independent review has not been started
RUNNING      Independent review is currently running
CI PENDING   GitHub CI is actually pending
PASS/WARN/FAIL are shown after review completes.

Suggested reviewer flow:
PR → Risk → Gates → Components → Files → Hunks → Lines → Findings

AI findings are evidence, not authority. Missing patch/context must remain "missing evidence" rather than becoming a guessed defect."#;
    let block = Block::default()
        .title(" Help ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));
    frame.render_widget(block, area);
    frame.render_widget(
        Paragraph::new(help).wrap(Wrap { trim: false }),
        padded_panel_inner(area),
    );
}

fn padded_panel_inner(area: Rect) -> Rect {
    area.inner(Margin {
        horizontal: 2,
        vertical: 2,
    })
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);
    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1]);
    horizontal[1]
}
