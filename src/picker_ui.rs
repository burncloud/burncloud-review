use ratatui::{
    layout::{Constraint, Direction, Layout, Margin},
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};

use crate::picker::PrPicker;

pub fn draw(frame: &mut Frame, picker: &PrPicker, backend: &str) {
    let area = frame.area().inner(Margin {
        horizontal: 1,
        vertical: 1,
    });
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Min(8),
            Constraint::Length(1),
            Constraint::Length(2),
        ])
        .split(area);

    let header = Paragraph::new(format!("Reviewer: {backend}")).block(
        Block::default()
            .title(format!(" BurnCloud Review · {} ", picker.repository))
            .borders(Borders::ALL),
    );
    frame.render_widget(header, vertical[0]);

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(52),
            Constraint::Length(1),
            Constraint::Percentage(48),
        ])
        .split(vertical[2]);

    let items: Vec<ListItem> = picker
        .prs
        .iter()
        .map(|pr| {
            ListItem::new(Line::from(format!(
                "#{:<5} [{:<6}] {}  · {}",
                pr.number,
                pr.state_label(),
                pr.title,
                pr.user.login
            )))
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .title(" Recent Pull Requests ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .highlight_symbol("▶ ")
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );
    let mut state = ListState::default();
    if !picker.prs.is_empty() {
        state.select(Some(picker.selected));
    }
    frame.render_stateful_widget(list, body[0], &mut state);

    let detail = Paragraph::new(picker.detail_text())
        .block(
            Block::default()
                .title(" PR Overview ")
                .borders(Borders::ALL),
        )
        .wrap(Wrap { trim: false });
    frame.render_widget(detail, body[2]);

    let footer = Paragraph::new(format!(
        "{}  |  ↑↓ select  Enter review  r refresh  q quit",
        picker.status
    ))
    .style(Style::default().fg(Color::DarkGray))
    .wrap(Wrap { trim: true });
    frame.render_widget(footer, vertical[4]);
}

pub fn draw_loading(frame: &mut Frame, repository: &str, backend: &str, message: &str) {
    let area = frame.area().inner(Margin {
        horizontal: 1,
        vertical: 1,
    });
    let block = Block::default()
        .title(format!(" BurnCloud Review · {repository} "))
        .borders(Borders::ALL);
    let text = format!("{message}\n\nReviewer: {backend}");
    frame.render_widget(
        Paragraph::new(text).block(block).wrap(Wrap { trim: true }),
        area,
    );
}
