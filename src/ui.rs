use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};

use crate::app::{App, Focus};

pub fn draw(frame: &mut Frame, app: &App, ai_endpoint: &str) {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(8),
            Constraint::Length(2),
        ])
        .split(frame.area());

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
    let widget = Paragraph::new(text)
        .block(Block::default().title(title).borders(Borders::ALL))
        .alignment(Alignment::Left);
    frame.render_widget(widget, area);
}

fn draw_tree(frame: &mut Frame, app: &App, area: Rect) {
    let entries = app.tree_entries();
    let items: Vec<ListItem> = entries
        .iter()
        .map(|entry| {
            let indent = "  ".repeat(entry.depth);
            let marker = if entry.expandable {
                if entry.expanded { "▾ " } else { "▸ " }
            } else {
                "• "
            };
            ListItem::new(Line::from(format!("{indent}{marker}{}", entry.label)))
        })
        .collect();

    let border = if app.focus == Focus::Tree {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default()
    };
    let list = List::new(items)
        .block(
            Block::default()
                .title(" Review Tree ")
                .borders(Borders::ALL)
                .border_style(border),
        )
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");

    let mut state = ListState::default();
    state.select(Some(app.selected));
    frame.render_stateful_widget(list, area, &mut state);
}

fn draw_detail(frame: &mut Frame, app: &App, area: Rect) {
    let border = if app.focus == Focus::Detail {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default()
    };
    let detail = Paragraph::new(app.detail_text())
        .block(
            Block::default()
                .title(" Evidence / Detail ")
                .borders(Borders::ALL)
                .border_style(border),
        )
        .wrap(Wrap { trim: false })
        .scroll((app.detail_scroll, 0));
    frame.render_widget(detail, area);
}

fn draw_footer(frame: &mut Frame, app: &App, area: Rect) {
    let text = format!(
        "{}  |  ↑↓ move  ←→ layer  Enter expand  Tab focus  PgUp/PgDn scroll  a AI review  r refresh  ? help  q quit",
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
A            Run the independent LLM review
R            Reload PR metadata, files and CI from GitHub
?            Toggle this help
Q            Quit

Suggested reviewer flow:
PR → Risk → Gates → Components → Files → Hunks → Lines → Findings

AI findings are evidence, not authority. Missing patch/context must remain "missing evidence" rather than becoming a guessed defect."#;
    let popup = Paragraph::new(help)
        .block(
            Block::default()
                .title(" Help ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow)),
        )
        .wrap(Wrap { trim: false });
    frame.render_widget(popup, area);
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
