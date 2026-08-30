mod ai;
mod app;
mod codex;
mod diff;
mod github;
mod local_ci;
mod models;
#[cfg(test)]
mod navigation_tests;
mod picker;
mod picker_ui;
mod review;
mod reviewer;
mod ui;

use std::{
    io,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use app::App;
use clap::Parser;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use picker::PrPicker;
use ratatui::{backend::CrosstermBackend, Terminal};
use tokio::task::JoinHandle;

use crate::{
    github::GitHubClient,
    local_ci::{LocalCiConfig, LocalCiEvent, LocalCiExecution},
    models::{AiReviewReport, CommitStatus, GateStatus},
    reviewer::{ReviewBackend, ReviewBackendOptions},
};

#[derive(Debug, Parser)]
#[command(name = "burncloud-review")]
#[command(about = "Interactive evidence-driven pull-request review console")]
struct Args {
    /// Repository in owner/name form. Defaults to the main BurnCloud repository.
    #[arg(long, env = "BCR_REPO", default_value = "burncloud/burncloud")]
    repo: String,

    /// Optional pull request number. Omit it to choose from the Ratatui PR picker.
    #[arg(long)]
    pr: Option<u64>,

    /// Local BurnCloud checkout used as the Git object store and Cargo build source.
    #[arg(long, env = "BCR_LOCAL_REPO", default_value = "../burncloud")]
    local_repo: PathBuf,

    /// Maximum time allowed for each local CI command.
    #[arg(long, env = "BCR_LOCAL_CI_TIMEOUT_SECS", default_value_t = 1800)]
    local_ci_timeout_secs: u64,

    /// GitHub token. Public repositories can work without one but are rate limited.
    #[arg(long, env = "GITHUB_TOKEN")]
    github_token: Option<String>,

    /// Review backend: auto, codex, or http. Auto prefers a local Codex CLI.
    #[arg(long, env = "BCR_AI_BACKEND", default_value = "auto")]
    ai_backend: String,

    /// Optional explicit local Codex executable path/name.
    #[arg(long, env = "BCR_CODEX_BIN")]
    codex_bin: Option<String>,

    /// Optional model override for local Codex. Omit it to use Codex configuration.
    #[arg(long, env = "BCR_CODEX_MODEL")]
    codex_model: Option<String>,

    /// Maximum time for one AI review before it is terminated.
    #[arg(long, env = "BCR_REVIEW_TIMEOUT_SECS", default_value_t = 300)]
    review_timeout_secs: u64,

    /// OpenAI-compatible base URL used by the HTTP fallback/backend.
    #[arg(
        long,
        env = "BCR_AI_BASE_URL",
        default_value = "http://localhost:3000/v1"
    )]
    ai_base_url: String,

    /// Optional bearer token for the HTTP AI endpoint.
    #[arg(long, env = "BCR_AI_API_KEY")]
    ai_api_key: Option<String>,

    /// Model used by the HTTP AI backend.
    #[arg(long, env = "BCR_AI_MODEL", default_value = "deepseek-v3")]
    ai_model: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    validate_repo(&args.repo)?;

    let github = GitHubClient::new(args.github_token.as_deref())?;
    let reviewer = ReviewBackend::from_options(ReviewBackendOptions {
        backend: args.ai_backend,
        codex_bin: args.codex_bin,
        codex_model: args.codex_model,
        http_base_url: args.ai_base_url,
        http_api_key: args.ai_api_key,
        http_model: args.ai_model,
        review_timeout_secs: args.review_timeout_secs,
    })?;
    let local_ci = LocalCiConfig {
        repo: args.local_repo,
        step_timeout: Duration::from_secs(args.local_ci_timeout_secs.max(1)),
    };

    run_terminal(args.repo, args.pr, github, reviewer, local_ci).await
}

async fn run_terminal(
    repository: String,
    initial_pr: Option<u64>,
    github: GitHubClient,
    reviewer: ReviewBackend,
    local_ci: LocalCiConfig,
) -> Result<()> {
    enable_raw_mode().context("enable terminal raw mode")?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)
        .context("enter alternate terminal screen")?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).context("create Ratatui terminal")?;

    let result = application_loop(
        &mut terminal,
        &repository,
        initial_pr,
        &github,
        &reviewer,
        &local_ci,
    )
    .await;

    disable_raw_mode().ok();
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )
    .ok();
    terminal.show_cursor().ok();
    result
}

async fn application_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    repository: &str,
    initial_pr: Option<u64>,
    github: &GitHubClient,
    reviewer: &ReviewBackend,
    local_ci: &LocalCiConfig,
) -> Result<()> {
    let reviewer_summary = reviewer.summary();
    let mut next_pr = initial_pr;

    loop {
        let number = match next_pr.take() {
            Some(number) => number,
            None => match pick_pull_request(terminal, repository, github, &reviewer_summary).await?
            {
                Some(number) => number,
                None => return Ok(()),
            },
        };

        terminal.draw(|frame| {
            picker_ui::draw_loading(
                frame,
                repository,
                &reviewer_summary,
                &format!("Loading PR #{number} and changed files..."),
            )
        })?;

        let data = github
            .load_pull_request(repository, number)
            .await
            .with_context(|| format!("load {repository} PR #{number}"))?;
        let mut app = App::new(data);
        app.status = format!(
            "本地 CI 尚未运行。按 T 使用 {} 创建隔离 worktree 并执行真实 build/test。",
            local_ci.repo.display()
        );

        match review_event_loop(
            terminal,
            &mut app,
            github,
            reviewer,
            &reviewer_summary,
            local_ci,
            None,
        )
        .await?
        {
            ReviewExit::BackToPicker => {}
            ReviewExit::Quit => return Ok(()),
        }
    }
}

async fn pick_pull_request(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    repository: &str,
    github: &GitHubClient,
    reviewer_summary: &str,
) -> Result<Option<u64>> {
    terminal.draw(|frame| {
        picker_ui::draw_loading(
            frame,
            repository,
            reviewer_summary,
            "Loading recent pull requests from GitHub...",
        )
    })?;

    let prs = github
        .load_recent_pull_requests(repository, 30)
        .await
        .with_context(|| format!("load recent pull requests for {repository}"))?;
    let mut picker = PrPicker::new(repository.to_string(), prs);

    loop {
        terminal.draw(|frame| picker_ui::draw(frame, &picker, reviewer_summary))?;

        let Some(key) = read_key()? else {
            continue;
        };
        match key {
            KeyCode::Up => picker.move_up(),
            KeyCode::Down => picker.move_down(),
            KeyCode::Enter => {
                if let Some(number) = picker.selected_number() {
                    return Ok(Some(number));
                }
            }
            KeyCode::Char('r') | KeyCode::Char('R') => {
                picker.status = "Refreshing recent pull requests...".into();
                terminal.draw(|frame| picker_ui::draw(frame, &picker, reviewer_summary))?;
                match github.load_recent_pull_requests(repository, 30).await {
                    Ok(prs) => picker.replace(prs),
                    Err(error) => picker.status = format!("Refresh failed: {error:#}"),
                }
            }
            KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => return Ok(None),
            _ => {}
        }
    }
}

enum ReviewExit {
    BackToPicker,
    Quit,
}

struct ActiveReview {
    handle: JoinHandle<Result<AiReviewReport>>,
    cancel: Arc<AtomicBool>,
    started_at: Instant,
}

struct ActiveLocalCi {
    execution: LocalCiExecution,
    started_at: Instant,
}

async fn review_event_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    github: &GitHubClient,
    reviewer: &ReviewBackend,
    reviewer_summary: &str,
    local_ci: &LocalCiConfig,
    mut active_ci: Option<ActiveLocalCi>,
) -> Result<ReviewExit> {
    let mut active_review: Option<ActiveReview> = None;

    loop {
        if app.should_quit {
            cancel_local_ci(&active_ci);
            return Ok(ReviewExit::Quit);
        }

        poll_local_ci(app, &mut active_ci);

        if active_review
            .as_ref()
            .is_some_and(|active| active.handle.is_finished())
        {
            let active = active_review.take().expect("finished review exists");
            match active.handle.await {
                Ok(Ok(report)) => {
                    app.set_report(report);
                    app.status = "AI 审查完成。可以逐层查看关卡、文件和审查发现。".into();
                }
                Ok(Err(error)) => {
                    app.status = format!("AI 审查结束：{error:#}");
                }
                Err(error) => {
                    app.status = format!("AI 审查任务异常结束：{error}");
                }
            }
            app.busy = false;
        }

        update_running_status(app, active_review.as_ref(), active_ci.as_ref(), reviewer);

        terminal.draw(|frame| ui::draw(frame, app, reviewer_summary))?;

        let Some(key) = read_key()? else {
            continue;
        };

        match key {
            KeyCode::Char('q') | KeyCode::Char('Q') if !app.busy => {
                cancel_local_ci(&active_ci);
                app.should_quit = true;
            }
            KeyCode::Esc if app.show_help => app.show_help = false,
            KeyCode::Esc if !app.busy => {
                cancel_local_ci(&active_ci);
                return Ok(ReviewExit::BackToPicker);
            }
            KeyCode::Char('?') => app.show_help = !app.show_help,
            KeyCode::Tab if !app.show_help => app.toggle_focus(),
            KeyCode::Up if !app.show_help => app.move_up(),
            KeyCode::Down if !app.show_help => app.move_down(),
            KeyCode::Left if !app.show_help => app.collapse_selected(),
            KeyCode::Right if !app.show_help => app.expand_selected(),
            KeyCode::Enter if !app.show_help => app.toggle_selected(),
            KeyCode::PageUp if !app.show_help => app.page_up(),
            KeyCode::PageDown if !app.show_help => app.page_down(),
            KeyCode::Char('c') | KeyCode::Char('C') if active_review.is_some() => {
                if let Some(active) = &active_review {
                    active.cancel.store(true, Ordering::Relaxed);
                    app.status = "正在取消 AI 审查并终止本地 Codex 子进程...".into();
                }
            }
            KeyCode::Char('x') | KeyCode::Char('X') if active_ci.is_some() => {
                cancel_local_ci(&active_ci);
                app.status = "正在取消本地 CI...".into();
            }
            KeyCode::Char('t') | KeyCode::Char('T') if !app.show_help && active_ci.is_none() => {
                active_ci = start_local_ci(app, local_ci);
            }
            KeyCode::Char('a') | KeyCode::Char('A') if !app.show_help && !app.busy => {
                app.busy = true;
                let cancel = Arc::new(AtomicBool::new(false));
                let task_cancel = Arc::clone(&cancel);
                let task_reviewer = reviewer.clone();
                let data = app.data.clone();
                let handle =
                    tokio::spawn(async move { task_reviewer.review(&data, task_cancel).await });
                active_review = Some(ActiveReview {
                    handle,
                    cancel,
                    started_at: Instant::now(),
                });
                app.status = format!(
                    "AI 审查已在后台启动 · 最长 {} 秒 · 按 C 取消",
                    reviewer.timeout_secs()
                );
            }
            KeyCode::Char('r') | KeyCode::Char('R') if !app.show_help && !app.busy => {
                app.busy = true;
                app.status = "正在刷新 Pull Request 和修改文件...".into();
                terminal.draw(|frame| ui::draw(frame, app, reviewer_summary))?;
                let repository = app.data.repository.clone();
                let number = app.data.pr.number;
                match github.load_pull_request(&repository, number).await {
                    Ok(data) => {
                        cancel_local_ci(&active_ci);
                        active_ci = None;
                        app.replace_data(data);
                        app.status = format!(
                            "PR 已刷新。本地 CI 尚未运行；按 T 使用 {} 重新验证。",
                            local_ci.repo.display()
                        );
                    }
                    Err(error) => {
                        app.status = format!("刷新失败: {error:#}");
                    }
                }
                app.busy = false;
            }
            _ => {}
        }
    }
}

fn start_local_ci(app: &mut App, config: &LocalCiConfig) -> Option<ActiveLocalCi> {
    app.data.ci.state = "pending".into();
    app.data.ci.statuses.clear();
    app.status = format!(
        "本地 CI 已启动：使用 {} 创建 PR worktree 并执行真实 build/test",
        config.repo.display()
    );
    Some(ActiveLocalCi {
        execution: config.start(
            app.data.pr.number,
            app.data.pr.head.sha.clone(),
            app.data.files.clone(),
        ),
        started_at: Instant::now(),
    })
}

fn poll_local_ci(app: &mut App, active_ci: &mut Option<ActiveLocalCi>) {
    let mut finished = false;
    if let Some(active) = active_ci.as_mut() {
        while let Ok(event) = active.execution.receiver.try_recv() {
            match event {
                LocalCiEvent::Started { worktree, packages } => {
                    let package_text = if packages.is_empty() {
                        "workspace".to_string()
                    } else {
                        packages.join(", ")
                    };
                    app.data.ci.statuses.push(CommitStatus {
                        state: "success".into(),
                        context: "local/setup".into(),
                        description: Some(format!(
                            "PR worktree 已准备 · packages: {package_text} · {worktree}"
                        )),
                        command: None,
                        duration_ms: None,
                        exit_code: Some(0),
                        output: None,
                    });
                }
                LocalCiEvent::StepStarted { context, command } => {
                    upsert_ci_status(
                        app,
                        CommitStatus {
                            state: "running".into(),
                            context,
                            description: Some("正在本机执行".into()),
                            command: Some(command),
                            duration_ms: None,
                            exit_code: None,
                            output: None,
                        },
                    );
                }
                LocalCiEvent::StepFinished(status) => upsert_ci_status(app, status),
                LocalCiEvent::Finished { success } => {
                    app.data.ci.state = if success { "success" } else { "failure" }.into();
                    if !success {
                        hard_fail_evidence(app);
                    }
                    app.status = if success {
                        "本地 CI 完成：format / build / test / clippy 全部通过。".into()
                    } else {
                        "本地 CI 失败：请展开证据关卡查看真实命令和输出。".into()
                    };
                    finished = true;
                }
                LocalCiEvent::Failed { message } => {
                    app.data.ci.state = "failure".into();
                    upsert_ci_status(
                        app,
                        CommitStatus {
                            state: "failure".into(),
                            context: "local/setup".into(),
                            description: Some(message),
                            command: None,
                            duration_ms: None,
                            exit_code: None,
                            output: None,
                        },
                    );
                    hard_fail_evidence(app);
                    app.status = "本地 CI 无法完成；请展开证据关卡查看原因。".into();
                    finished = true;
                }
            }
        }
    }
    if finished {
        *active_ci = None;
    }
}

fn hard_fail_evidence(app: &mut App) {
    if let Some(report) = app.report.as_mut() {
        report.gates.evidence.status = GateStatus::Fail;
    }
}

fn upsert_ci_status(app: &mut App, status: CommitStatus) {
    if let Some(existing) = app
        .data
        .ci
        .statuses
        .iter_mut()
        .find(|item| item.context == status.context)
    {
        *existing = status;
    } else {
        app.data.ci.statuses.push(status);
    }
}

fn cancel_local_ci(active_ci: &Option<ActiveLocalCi>) {
    if let Some(active) = active_ci {
        active.execution.cancel.store(true, Ordering::Relaxed);
    }
}

fn update_running_status(
    app: &mut App,
    active_review: Option<&ActiveReview>,
    active_ci: Option<&ActiveLocalCi>,
    reviewer: &ReviewBackend,
) {
    let ai = active_review.map(|active| {
        format!(
            "AI 审查 {} / 最长 {} 秒",
            format_elapsed(active.started_at.elapsed()),
            reviewer.timeout_secs()
        )
    });
    let ci = active_ci.map(|active| {
        format!(
            "本地 CI {} / {}",
            format_elapsed(active.started_at.elapsed()),
            current_ci_step(app)
        )
    });
    app.status = match (ai, ci) {
        (Some(ai), Some(ci)) => format!("{ai} · {ci} · C取消AI / X取消CI"),
        (Some(ai), None) => format!("{ai} · 按 C 取消"),
        (None, Some(ci)) => format!("{ci} · 按 X 取消"),
        (None, None) => return,
    };
}

fn current_ci_step(app: &App) -> &str {
    app.data
        .ci
        .statuses
        .iter()
        .rev()
        .find(|status| status.state == "running")
        .map(|status| status.context.as_str())
        .unwrap_or("准备 worktree")
}

fn format_elapsed(duration: Duration) -> String {
    let seconds = duration.as_secs();
    format!("{:02}:{:02}", seconds / 60, seconds % 60)
}

fn read_key() -> Result<Option<KeyCode>> {
    if !event::poll(Duration::from_millis(200)).context("poll terminal event")? {
        return Ok(None);
    }
    let Event::Key(key) = event::read().context("read terminal event")? else {
        return Ok(None);
    };
    if key.kind == KeyEventKind::Release {
        return Ok(None);
    }
    Ok(Some(key.code))
}

fn validate_repo(repository: &str) -> Result<()> {
    let mut parts = repository.split('/');
    let owner = parts.next().unwrap_or_default();
    let name = parts.next().unwrap_or_default();
    if owner.is_empty() || name.is_empty() || parts.next().is_some() {
        anyhow::bail!("--repo must use owner/name form, for example burncloud/burncloud");
    }
    Ok(())
}
