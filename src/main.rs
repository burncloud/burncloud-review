mod ai;
mod app;
mod codex;
mod diff;
mod github;
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
    models::AiReviewReport,
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

    run_terminal(args.repo, args.pr, github, reviewer).await
}

async fn run_terminal(
    repository: String,
    initial_pr: Option<u64>,
    github: GitHubClient,
    reviewer: ReviewBackend,
) -> Result<()> {
    enable_raw_mode().context("enable terminal raw mode")?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)
        .context("enter alternate terminal screen")?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).context("create Ratatui terminal")?;

    let result = application_loop(&mut terminal, &repository, initial_pr, &github, &reviewer).await;

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
                &format!("Loading PR #{number}, changed files and CI evidence..."),
            )
        })?;

        let data = github
            .load_pull_request(repository, number)
            .await
            .with_context(|| format!("load {repository} PR #{number}"))?;
        let mut app = App::new(data);

        match review_event_loop(terminal, &mut app, github, reviewer, &reviewer_summary).await? {
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

async fn review_event_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    github: &GitHubClient,
    reviewer: &ReviewBackend,
    reviewer_summary: &str,
) -> Result<ReviewExit> {
    let mut active_review: Option<ActiveReview> = None;

    loop {
        if app.should_quit {
            return Ok(ReviewExit::Quit);
        }

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

        if let Some(active) = &active_review {
            let elapsed = format_elapsed(active.started_at.elapsed());
            app.status = format!(
                "AI 审查正在后台运行 · 已耗时 {elapsed} · 最长 {} 秒 · 按 C 取消",
                reviewer.timeout_secs()
            );
        }

        terminal.draw(|frame| ui::draw(frame, app, reviewer_summary))?;

        let Some(key) = read_key()? else {
            continue;
        };

        match key {
            KeyCode::Char('q') | KeyCode::Char('Q') if !app.busy => app.should_quit = true,
            KeyCode::Esc if app.show_help => app.show_help = false,
            KeyCode::Esc if !app.busy => return Ok(ReviewExit::BackToPicker),
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
                app.status = "Refreshing pull request, changed files and CI evidence...".into();
                terminal.draw(|frame| ui::draw(frame, app, reviewer_summary))?;
                let repository = app.data.repository.clone();
                let number = app.data.pr.number;
                match github.load_pull_request(&repository, number).await {
                    Ok(data) => app.replace_data(data),
                    Err(error) => {
                        app.status = format!("Refresh failed: {error:#}");
                    }
                }
                app.busy = false;
            }
            _ => {}
        }
    }
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
