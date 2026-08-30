use ratatui::{
    layout::{Constraint, Direction, Layout, Margin, Rect},
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
            Constraint::Length(5),
            Constraint::Min(8),
            Constraint::Length(2),
        ])
        .split(area);

    let header_block = Block::default()
        .title(format!(" BurnCloud Review · {} ", picker.repository))
        .borders(Borders::ALL);
    frame.render_widget(header_block, vertical[0]);
    frame.render_widget(
        Paragraph::new(format!("Reviewer: {backend}")),
        padded_panel_inner(vertical[0]),
    );

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(52), Constraint::Percentage(48)])
        .split(vertical[1]);

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

    let list_block = Block::default()
        .title(" Recent Pull Requests ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    frame.render_widget(list_block, body[0]);

    let list = List::new(items).highlight_symbol("▶ ").highlight_style(
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    );
    let mut state = ListState::default();
    if !picker.prs.is_empty() {
        state.select(Some(picker.selected));
    }
    frame.render_stateful_widget(list, padded_panel_inner(body[0]), &mut state);

    let detail_block = Block::default()
        .title(" PR Overview ")
        .borders(Borders::ALL);
    frame.render_widget(detail_block, body[1]);
    frame.render_widget(
        Paragraph::new(picker.detail_text()).wrap(Wrap { trim: false }),
        padded_panel_inner(body[1]),
    );

    let footer = Paragraph::new(format!(
        "{}  |  ↑↓ select  Enter review  r refresh  q quit",
        picker.status
    ))
    .style(Style::default().fg(Color::DarkGray))
    .wrap(Wrap { trim: true });
    frame.render_widget(footer, vertical[2]);
}

pub fn draw_loading(frame: &mut Frame, repository: &str, backend: &str, message: &str) {
    let area = frame.area().inner(Margin {
        horizontal: 1,
        vertical: 1,
    });
    let block = Block::default()
        .title(format!(" BurnCloud Review · {repository} "))
        .borders(Borders::ALL);
    frame.render_widget(block, area);
    let text = format!("{message}\n\nReviewer: {backend}");
    frame.render_widget(
        Paragraph::new(text).wrap(Wrap { trim: true }),
        padded_panel_inner(area),
    );
}

fn padded_panel_inner(area: Rect) -> Rect {
    area.inner(Margin {
        horizontal: 2,
        vertical: 2,
    })
}
