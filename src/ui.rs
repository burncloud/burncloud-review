use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};

use crate::{
    app::{App, Focus, NodeId, TreeEntry},
    models::{DiffLineKind, GateKind, GateStatus, Severity},
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
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
        .split(vertical[1]);
    draw_tree(frame, app, body[0]);
    draw_detail(frame, app, body[1]);
    draw_footer(frame, app, vertical[2]);

    if app.show_help {
        draw_help(frame);
    }
}

fn draw_header(frame: &mut Frame, app: &App, ai_endpoint: &str, area: Rect) {
    let busy = if app.busy { " · 审查中" } else { "" };
    let title = format!(
        " BurnCloud Review · {} · PR #{} · 风险 {}{} ",
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
        .title(" 审查树 ")
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
    match entry.id {
        NodeId::Gates => "审查关卡".into(),
        NodeId::Gate(kind) => {
            let status = if app.report.is_some() {
                gate_status_label(app.gate_status(kind))
            } else {
                pre_review_status(app, kind)
            };
            format!("[{status}] {}", gate_title_cn(kind))
        }
        NodeId::Components => format!("受影响组件 ({})", app.components().len()),
        NodeId::Files => format!("修改文件 ({})", app.data.files.len()),
        NodeId::Findings => format!(
            "AI 审查发现 ({})",
            app.report.as_ref().map(|r| r.findings.len()).unwrap_or(0)
        ),
        _ => entry.label.clone(),
    }
}

fn pre_review_status(app: &App, kind: GateKind) -> &'static str {
    if kind == GateKind::Evidence {
        return match app.data.ci.state.to_ascii_lowercase().as_str() {
            "success" => "CI通过",
            "failure" | "error" => "CI失败",
            "pending" | "expected" => "CI等待",
            _ => "CI未知",
        };
    }
    if app.busy {
        "审查中"
    } else {
        "未审查"
    }
}

fn gate_status_label(status: GateStatus) -> &'static str {
    match status {
        GateStatus::Pending => "等待",
        GateStatus::Pass => "通过",
        GateStatus::Warn => "警告",
        GateStatus::Fail => "失败",
    }
}

fn gate_title_cn(kind: GateKind) -> &'static str {
    match kind {
        GateKind::Scope => "范围",
        GateKind::Code => "代码",
        GateKind::Behavior => "行为",
        GateKind::Architecture => "架构",
        GateKind::Evidence => "证据",
    }
}

fn draw_detail(frame: &mut Frame, app: &App, area: Rect) {
    let border = if app.focus == Focus::Detail {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default()
    };
    let block = Block::default()
        .title(" 证据 / 详情 ")
        .borders(Borders::ALL)
        .border_style(border);
    frame.render_widget(block, area);

    let detail = Paragraph::new(display_detail_text(app))
        .wrap(Wrap { trim: false })
        .scroll((app.detail_scroll, 0));
    frame.render_widget(detail, padded_panel_inner(area));
}

fn display_detail_text(app: &App) -> String {
    match app.selected_id() {
        NodeId::Root => root_detail_cn(app),
        NodeId::Gates => gates_detail_cn(app),
        NodeId::Gate(kind) => gate_detail_cn(app, kind),
        NodeId::Components => components_detail_cn(app),
        NodeId::Component(idx) => component_detail_cn(app, idx),
        NodeId::Files => files_detail_cn(app),
        NodeId::File(idx) => file_detail_cn(app, idx),
        NodeId::Hunk(file_idx, hunk_idx) => hunk_detail_cn(app, file_idx, hunk_idx),
        NodeId::Line(file_idx, hunk_idx, line_idx) => {
            line_detail_cn(app, file_idx, hunk_idx, line_idx)
        }
        NodeId::Findings => findings_detail_cn(app),
        NodeId::Finding(idx) => finding_detail_cn(app, idx),
    }
}

fn root_detail_cn(app: &App) -> String {
    let pr = &app.data.pr;
    let mut text = format!(
        "PR #{} — {}\n\n仓库: {}\n作者: {}\n状态: {}{}\n目标分支: {}\n来源分支: {} @ {}\n风险等级: {}\nCI: {}\n改动: +{} -{}，共 {} 个文件\n\n{}",
        pr.number,
        pr.title,
        app.data.repository,
        pr.user.login,
        pr.state,
        if pr.draft { "（草稿）" } else { "" },
        pr.base.name,
        pr.head.name,
        pr.head.sha,
        app.risk,
        app.data.ci.state,
        pr.additions,
        pr.deletions,
        pr.changed_files,
        pr.body.as_deref().unwrap_or("该 PR 未提供描述。")
    );
    if let Some(report) = &app.report {
        text.push_str(&format!(
            "\n\nAI 审查摘要\n{}\n\n合并建议\n{}",
            report.summary, report.merge_recommendation
        ));
    } else {
        text.push_str("\n\nAI 审查尚未运行。按 A 启动独立审查。");
    }
    text
}

fn gates_detail_cn(app: &App) -> String {
    let mut text = String::from(
        "审查关卡\n\n只有与当前风险等级相关的关卡拥有足够证据时，PR 才适合合并。\n\n",
    );
    for kind in GateKind::ALL {
        let status = if app.report.is_some() {
            gate_status_label(app.gate_status(kind))
        } else {
            pre_review_status(app, kind)
        };
        text.push_str(&format!("[{status}] {}\n", gate_title_cn(kind)));
    }
    text.push_str("\n展开该节点，可以逐项查看每个关卡的证据和结论。");
    text
}

fn gate_detail_cn(app: &App, kind: GateKind) -> String {
    if app.report.is_none() {
        return pre_review_gate_detail_cn(app, kind);
    }

    let status = app.gate_status(kind);
    let mut text = format!(
        "{}审查\n状态: {}\n\n",
        gate_title_cn(kind),
        gate_status_label(status)
    );
    if let Some(review) = app.gate_review(kind) {
        text.push_str(&review.summary);
        if !review.items.is_empty() {
            text.push_str("\n\n证据 / 关注点:\n");
            for item in &review.items {
                text.push_str(&format!("• {item}\n"));
            }
        }
    }
    if kind == GateKind::Evidence {
        text.push_str(&format!("\n\n综合 CI: {}\n", app.data.ci.state));
        for ci in &app.data.ci.statuses {
            text.push_str(&format!(
                "• {}: {} — {}\n",
                ci.context,
                ci.state,
                ci.description.as_deref().unwrap_or("")
            ));
        }
    }
    text
}

fn pre_review_gate_detail_cn(app: &App, kind: GateKind) -> String {
    let status = pre_review_status(app, kind);
    let explanation = match kind {
        GateKind::Scope => "检查实现是否严格停留在任务要求的修改边界内，是否夹带无关改动。",
        GateKind::Code => "检查正确性、错误路径、并发、资源清理、安全性、性能和回归风险。",
        GateKind::Behavior => "检查哪些运行时路径或用户可见行为发生变化，包括失败时的行为。",
        GateKind::Architecture => "检查组件职责、依赖方向和架构边界是否被破坏。",
        GateKind::Evidence => "独立展示 GitHub CI 等确定性证据，不依赖模型主观判断。",
    };

    let mut text = format!(
        "{}审查\n状态: {}\n\n{}",
        gate_title_cn(kind),
        status,
        explanation
    );
    if kind == GateKind::Evidence {
        text.push_str(&format!("\n\n综合 CI: {}", app.data.ci.state));
        for ci in &app.data.ci.statuses {
            text.push_str(&format!(
                "\n• {}: {} — {}",
                ci.context,
                ci.state,
                ci.description.as_deref().unwrap_or("")
            ));
        }
    } else if app.busy {
        text.push_str("\n\n独立审查正在运行。");
    } else {
        text.push_str("\n\n尚未审查。按 A 启动独立审查。");
    }
    text
}

fn components_detail_cn(app: &App) -> String {
    let mut text = String::from("受影响组件\n\n");
    for component in app.components() {
        text.push_str(&format!("• {} — {}\n", component.name, component.impact));
    }
    if app.report.is_some() {
        text.push_str("\n当前列表包含 AI 辅助的影响分析。");
    } else {
        text.push_str("\nAI 审查前，该列表来自确定性的路径推断；审查后会补充模型辅助的影响分析。");
    }
    text
}

fn component_detail_cn(app: &App, idx: usize) -> String {
    app.components()
        .get(idx)
        .map(|component| {
            format!(
                "{}\n\n影响: {}\n\n原因:\n{}",
                component.name, component.impact, component.reason
            )
        })
        .unwrap_or_else(|| "该组件已不可用。".into())
}

fn files_detail_cn(app: &App) -> String {
    format!(
        "修改文件\n\n{} 个文件 · +{} -{}\n\n展开后可以继续查看文件、Hunk 和具体修改行，同时保留 PR 全局上下文。",
        app.data.pr.changed_files, app.data.pr.additions, app.data.pr.deletions
    )
}

fn file_detail_cn(app: &App, idx: usize) -> String {
    let Some(file) = app.data.files.get(idx) else {
        return "该文件已不可用。".into();
    };
    let hunk_count = app.hunks.get(idx).map(Vec::len).unwrap_or(0);
    let mut text = format!(
        "{}\n\n状态: {}\n改动: +{} -{}（共 {} 行）\nHunk: {}\n",
        file.filename, file.status, file.additions, file.deletions, file.changes, hunk_count
    );
    if let Some(report) = &app.report {
        let matching: Vec<_> = report
            .findings
            .iter()
            .filter(|finding| finding.path.as_deref() == Some(file.filename.as_str()))
            .collect();
        if !matching.is_empty() {
            text.push_str("\n该文件的 AI 审查发现:\n");
            for finding in matching {
                text.push_str(&format!("• [{}] {}\n", finding.severity, finding.title));
            }
        }
    }
    if file.patch.is_none() {
        text.push_str("\nGitHub 未提供该文件的 Patch（可能是二进制或文件过大）。应将其视为审查证据缺失。");
    }
    text
}

fn hunk_detail_cn(app: &App, file_idx: usize, hunk_idx: usize) -> String {
    let Some(file) = app.data.files.get(file_idx) else {
        return "该文件已不可用。".into();
    };
    let Some(hunk) = app.hunks.get(file_idx).and_then(|v| v.get(hunk_idx)) else {
        return "该 Hunk 已不可用。".into();
    };
    let mut text = format!("{}\n{}\n\n", file.filename, hunk.header);
    for line in &hunk.lines {
        let old = line.old_line.map(|n| n.to_string()).unwrap_or_default();
        let new = line.new_line.map(|n| n.to_string()).unwrap_or_default();
        text.push_str(&format!("{:>5} {:>5} {}\n", old, new, line.content));
    }
    text
}

fn line_detail_cn(app: &App, file_idx: usize, hunk_idx: usize, line_idx: usize) -> String {
    let Some(file) = app.data.files.get(file_idx) else {
        return "该文件已不可用。".into();
    };
    let Some(line) = app
        .hunks
        .get(file_idx)
        .and_then(|v| v.get(hunk_idx))
        .and_then(|h| h.lines.get(line_idx))
    else {
        return "该修改行已不可用。".into();
    };
    let side = match line.kind {
        DiffLineKind::Add => "新增",
        DiffLineKind::Remove => "删除",
        DiffLineKind::Context => "上下文",
        DiffLineKind::Meta => "元信息",
    };
    let number = line.new_line.or(line.old_line).unwrap_or(0);
    let mut text = format!(
        "{}\n第 {} 行 · {}\n\n{}",
        file.filename, number, side, line.content
    );
    if let Some(report) = &app.report {
        let matching: Vec<_> = report
            .findings
            .iter()
            .filter(|finding| {
                finding.path.as_deref() == Some(file.filename.as_str())
                    && finding.line == Some(number)
            })
            .collect();
        if !matching.is_empty() {
            text.push_str("\n\n定位到此行的 AI 审查发现:\n");
            for finding in matching {
                text.push_str(&format!(
                    "• [{}] {}\n  {}\n",
                    finding.severity, finding.title, finding.explanation
                ));
            }
        }
    }
    text
}

fn findings_detail_cn(app: &App) -> String {
    let Some(report) = &app.report else {
        return "AI 审查发现\n\n尚未运行 AI 审查。按 A 开始。".into();
    };
    let counts = |severity| {
        report
            .findings
            .iter()
            .filter(|f| f.severity == severity)
            .count()
    };
    format!(
        "AI 审查发现\n\n阻断 BLOCKER: {}\n重大 MAJOR: {}\n次要 MINOR: {}\n建议 NIT: {}\n\n这些发现属于审查证据和建议，最终是否合并仍由审查者决定。",
        counts(Severity::Blocker),
        counts(Severity::Major),
        counts(Severity::Minor),
        counts(Severity::Nit)
    )
}

fn finding_detail_cn(app: &App, idx: usize) -> String {
    let Some(finding) = app.report.as_ref().and_then(|r| r.findings.get(idx)) else {
        return "该审查发现已不可用。".into();
    };
    format!(
        "[{}] {}\n\n类别: {}\n位置: {}{}\n\n{}\n\n建议处理方向:\n{}",
        finding.severity,
        finding.title,
        category_cn(&finding.category),
        finding.path.as_deref().unwrap_or("<未定位到具体代码行>"),
        finding.line.map(|n| format!(":{n}")).unwrap_or_default(),
        finding.explanation,
        if finding.suggestion.is_empty() {
            "模型未提供具体修复方案，请人工核对证据后决定。"
        } else {
            &finding.suggestion
        }
    )
}

fn category_cn(category: &str) -> &str {
    match category {
        "scope" => "范围",
        "code" => "代码",
        "behavior" => "行为",
        "architecture" => "架构",
        "evidence" => "证据",
        "security" => "安全",
        "performance" => "性能",
        other => other,
    }
}

fn draw_footer(frame: &mut Frame, app: &App, area: Rect) {
    let text = format!(
        "{}  |  ↑↓ 移动  ←→ 层级  Enter 展开  Tab 切换  A AI审查  R 刷新  Esc PR列表  ? 帮助  Q 退出",
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
    let help = r#"BURNCLOUD REVIEW 快捷键

左侧是分层审查树，不是普通文件列表。

↑ / ↓       在当前可见节点间移动
←            收起当前节点；已收起时返回父层
→            展开当前节点；已展开时进入第一个子节点
Enter        展开 / 收起当前层
Tab          切换焦点：审查树 ↔ 详情面板
PgUp/PgDn   翻页滚动证据 / 详情
A            启动独立 AI / Codex 审查
R            从 GitHub 重新加载 PR、文件和 CI
Esc          返回最近 PR 列表
?            显示 / 隐藏帮助
Q            退出

审查前状态：
未审查       独立审查尚未启动
审查中       独立审查正在运行
CI等待       GitHub CI 确实仍在等待
审查完成后显示：通过 / 警告 / 失败。

建议审查路径：
PR → 风险 → 关卡 → 组件 → 文件 → Hunk → 行 → AI发现

AI 发现只是证据和建议，不是最终权威。缺少 Patch 或上下文时必须标记为证据不足，不能猜测代码事实。"#;
    let block = Block::default()
        .title(" 帮助 ")
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
