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

    let focused = app.focus == Focus::Tree;
    let border = if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default()
    };
    let title = if focused {
        " 审查树 [当前焦点] "
    } else {
        " 审查树 [← 返回] "
    };
    let block = Block::default()
        .title(title)
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
            "success" => "本地CI通过",
            "failure" | "error" => "本地CI失败",
            "pending" | "expected" => "本地CI运行中",
            "not_run" => "本地CI未运行",
            _ => "本地CI未知",
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
    let focused = app.focus == Focus::Detail;
    let border = if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default()
    };
    let title = if focused {
        format!(
            " 证据 / 详情 [阅读模式] · ↑↓滚动 PgUp/PgDn翻页 ←返回 · offset {} ",
            app.detail_scroll
        )
    } else {
        format!(
            " 证据 / 详情 [按 → 或 Tab 进入] · offset {} ",
            app.detail_scroll
        )
    };
    let block = Block::default()
        .title(title)
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
        NodeId::Hunk(_, _) => app.detail_text(),
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
        "PR #{} — {}\n\n仓库: {}\n作者: {}\n状态: {}{}\n目标分支: {}\n来源分支: {} @ {}\n风险等级: {}\n本地 CI: {}\n改动: +{} -{}，共 {} 个文件\n\nPR 描述\n{}",
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
            "\n\n━━ AI 审查总览 ━━\n{}\n\n━━ 合并建议 ━━\n{}",
            report.summary, report.merge_recommendation
        ));
    } else {
        text.push_str("\n\nAI 审查尚未运行。按 A 启动独立审查。");
    }
    text
}

fn gates_detail_cn(app: &App) -> String {
    let mut text = String::from(
        "审查关卡\n\n这里不是五个简单标签，而是五套独立证据链。每个 Gate 都必须说明：检查了什么、看到了什么、依据是什么、还缺什么，以及为什么得到当前结论。\n\n",
    );
    for kind in GateKind::ALL {
        let status = if app.report.is_some() {
            gate_status_label(app.gate_status(kind))
        } else {
            pre_review_status(app, kind)
        };
        text.push_str(&format!("[{status}] {}\n", gate_title_cn(kind)));
    }
    text.push_str(
        "\n推荐阅读顺序：范围 → 代码 → 行为 → 架构 → 证据。\n在左侧选中具体关卡后按 → 进入右侧，使用 ↑↓ 或 PgUp/PgDn 阅读完整审查档案。",
    );
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
        let detailed = review.detailed_text_cn();
        if detailed.trim().is_empty() {
            text.push_str("AI 没有返回这一关卡的详细结构化内容，应视为审查证据不足。");
        } else {
            text.push_str(&detailed);
        }
    }
    if kind == GateKind::Evidence {
        append_local_ci_evidence(app, &mut text);
    }
    text
}

fn pre_review_gate_detail_cn(app: &App, kind: GateKind) -> String {
    let status = pre_review_status(app, kind);
    let mut text = format!(
        "{}审查\n状态: {}\n\n审查标准\n{}",
        gate_title_cn(kind),
        status,
        gate_checklist_cn(kind)
    );
    if kind == GateKind::Evidence {
        append_local_ci_evidence(app, &mut text);
        if app.data.ci.state == "not_run" {
            text.push_str("\n\n按 T 后才会执行 PR 中的本地代码。浏览 PR 本身不会自动执行代码。");
        }
    } else if app.busy {
        text.push_str("\n\n独立审查正在运行。完成后这里会变成逐章节审查档案，而不是一句摘要。");
    } else {
        text.push_str("\n\n尚未审查。按 A 启动独立审查。");
    }
    text
}

fn gate_checklist_cn(kind: GateKind) -> &'static str {
    match kind {
        GateKind::Scope => {
            "1. 任务目标与验收条件：PR 到底要求解决什么。\n2. 允许修改边界：哪些模块、文件、行为属于本次任务。\n3. 实际修改范围：Patch 实际改到了哪里。\n4. 无关或越界改动：是否夹带重构、功能扩张或额外行为。\n5. Scope 判定：实际实现是否严格停留在任务边界内。"
        }
        GateKind::Code => {
            "1. 核心正确性：条件、状态、数据流、返回值和不变量。\n2. 错误与异常路径：失败、部分失败、重试、错误传播和清理。\n3. 并发 / 状态 / 资源生命周期：竞态、取消、进程、文件、连接和资源所有权。\n4. 安全边界：输入、命令执行、权限、认证和信任边界。\n5. 性能与兼容性：阻塞、热点、平台差异、API / 行为兼容。\n6. 回归风险与测试点：哪里最可能被这次修改带坏，应该用什么测试证明。"
        }
        GateKind::Behavior => {
            "1. 修改前执行路径：旧逻辑如何运行。\n2. 修改后执行路径：新逻辑逐步经过哪些分支和组件。\n3. 用户 / 调用方可见变化：输出、状态、UI、API、时序和语义。\n4. 失败路径：每个重要步骤失败时系统如何表现。\n5. 状态与副作用：文件、进程、网络、缓存、持久化等变化。\n6. 兼容性判定：旧调用方和既有流程是否保持可用。"
        }
        GateKind::Architecture => {
            "1. 组件职责：新逻辑应该由谁负责，现在由谁负责。\n2. 依赖方向：依赖有没有反向、穿层或形成循环。\n3. 跨层调用与边界：UI / domain / runtime / network / storage 是否越界。\n4. 耦合与职责泄漏：策略是否重复、全局状态是否扩散、编排是否放错层。\n5. 可扩展性与维护成本：以后替换组件、测试或扩展功能是否更困难。\n6. Architecture 判定：本次修改是在强化还是削弱架构边界。"
        }
        GateKind::Evidence => {
            "1. 本地 CI：只认本机隔离 worktree 中真实执行的 format / build / test / clippy。\n2. Patch 覆盖度：是否有二进制、超大文件、截断内容或缺失上下文。\n3. 测试充分性：编译通过不等于行为正确，必须区分 build / lint 与行为测试。\n4. 尚缺验证：当前风险等级还需要哪些确定性证据。\n5. Evidence 判定：现有证据是否足以支持合并。"
        }
    }
}

fn append_local_ci_evidence(app: &App, text: &mut String) {
    text.push_str(&format!(
        "\n\n━━ 本地 CI 硬证据 ━━\n状态: {}",
        app.data.ci.state
    ));
    if app.data.ci.statuses.is_empty() {
        text.push_str("\n• 尚无本地执行证据。");
        return;
    }
    for ci in &app.data.ci.statuses {
        text.push_str(&format!(
            "\n\n• {}: {}\n{}",
            ci.context,
            ci.state,
            ci.evidence_text()
        ));
    }
}

fn components_detail_cn(app: &App) -> String {
    let mut text = String::from("受影响组件\n\n");
    for component in app.components() {
        text.push_str(&format!("• {} — {}\n", component.name, component.impact));
    }
    if app.report.is_some() {
        text.push_str("\n当前列表包含 AI 辅助的影响分析。选中具体组件后可查看影响原因。");
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
                "{}\n\n影响\n{}\n\n原因 / 证据\n{}",
                component.name, component.impact, component.reason
            )
        })
        .unwrap_or_else(|| "该组件已不可用。".into())
}

fn files_detail_cn(app: &App) -> String {
    format!(
        "修改文件\n\n{} 个文件 · +{} -{}\n\n展开后可以继续查看文件、Hunk 和具体修改行，同时保留 PR 全局上下文。选中叶子节点后按 → 进入右侧阅读。",
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
        text.push_str(
            "\nGitHub 未提供该文件的 Patch（可能是二进制或文件过大）。应将其视为审查证据缺失。",
        );
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
        "AI 审查发现\n\n阻断 BLOCKER: {}\n重大 MAJOR: {}\n次要 MINOR: {}\n建议 NIT: {}\n\n这些发现是可定位的问题列表；完整的思考过程和证据链应优先在五个 Gate 的详细审查档案中阅读。",
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
        "[{}] {}\n\n类别: {}\n位置: {}{}\n\n问题解释\n{}\n\n建议处理方向\n{}",
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
    let controls = if app.focus == Focus::Detail {
        "右侧阅读: ↑↓ 滚动  PgUp/PgDn 翻页  ← 返回左侧  Tab 切换"
    } else {
        "左侧导航: ↑↓ 移动  ← 收起/返回  → 展开/进入右侧  Enter 展开/进入详情  Tab 切换"
    };
    let text = format!(
        "{}  |  {}  |  T 本地CI  X取消CI  A AI审查  C取消AI  R刷新  Esc PR列表  ?帮助  Q退出",
        app.status, controls
    );
    frame.render_widget(
        Paragraph::new(text)
            .style(Style::default().fg(Color::DarkGray))
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn draw_help(frame: &mut Frame) {
    let area = centered_rect(76, 76, frame.area());
    frame.render_widget(Clear, area);
    let help = r#"BURNCLOUD REVIEW 快捷键

左侧是审查树，右侧是可滚动的完整审查档案。

左侧焦点：
↑ / ↓       在当前可见节点间移动
←            收起当前节点；已收起时返回父层
→            展开节点；如果是具体叶子节点则进入右侧详情
Enter        展开节点；如果是叶子节点则进入右侧详情
Tab          直接切换：审查树 ↔ 详情面板

右侧焦点：
↑ / ↓       逐行向上 / 向下滚动
PgUp/PgDn   大步翻页
←            返回左侧审查树
→            向下滚动几行
Tab          返回左侧审查树

T            使用 ../burncloud 创建隔离 worktree 并运行本地 CI
X            取消当前本地 CI
A            启动独立 AI / Codex 深度审查
C            取消当前 AI 审查
R            重新加载 GitHub PR 元数据和 Patch；本地 CI 恢复未运行
Esc          返回最近 PR 列表
?            显示 / 隐藏帮助
Q            退出

五个 Gate 不再是一句摘要：
范围         目标、边界、实际范围、越界改动、Scope 判定
代码         正确性、错误路径、并发/资源、安全、性能/兼容、回归测试
行为         修改前路径、修改后路径、可见变化、失败路径、副作用、兼容性
架构         职责、依赖方向、跨层调用、耦合、维护成本、Architecture 判定
证据         本地 CI、Patch 覆盖、测试充分性、缺失验证、Evidence 判定

本地 CI 状态：
未运行       浏览 PR 不会自动执行 PR 代码；按 T 后才运行
运行中       正在本机执行 format / build / test / clippy
通过         本地命令真实返回成功退出码
失败         至少一项本地命令失败，Evidence 不能被 AI 提升为通过

GitHub 只用于读取 PR 身份、SHA、元数据和 Patch；编译与测试硬证据来自本机。
AI 发现只是证据和建议，不是最终权威；缺少 Patch 或上下文时必须明确标记为缺失证据。"#;
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
