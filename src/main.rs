mod ai;
mod app;
mod diff;
mod github;
mod models;
mod review;
mod ui;

use std::{io, time::Duration};

use anyhow::{Context, Result};
use app::App;
use clap::Parser;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

use crate::{ai::AiClient, github::GitHubClient};

#[derive(Debug, Parser)]
#[command(name = "burncloud-review")]
#[command(about = "Interactive evidence-driven pull-request review console")]
struct Args {
    /// Repository in owner/name form.
    #[arg(long)]
    repo: String,

    /// Pull request number.
    #[arg(long)]
    pr: u64,

    /// GitHub token. Public repositories can work without one but are rate limited.
    #[arg(long, env = "GITHUB_TOKEN")]
    github_token: Option<String>,

    /// OpenAI-compatible base URL. BurnCloud Node is the default local AI endpoint.
    #[arg(
        long,
        env = "BCR_AI_BASE_URL",
        default_value = "http://localhost:3000/v1"
    )]
    ai_base_url: String,

    /// Optional bearer token for the AI endpoint.
    #[arg(long, env = "BCR_AI_API_KEY")]
    ai_api_key: Option<String>,

    /// Model used for independent review.
    #[arg(long, env = "BCR_AI_MODEL", default_value = "deepseek-v3")]
    ai_model: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    validate_repo(&args.repo)?;

    let github = GitHubClient::new(args.github_token.as_deref())?;
    eprintln!("Loading {} PR #{} from GitHub...", args.repo, args.pr);
    let data = github
        .load_pull_request(&args.repo, args.pr)
        .await
        .with_context(|| format!("load {} PR #{}", args.repo, args.pr))?;

    let ai = AiClient::new(args.ai_base_url, args.ai_api_key, args.ai_model)?;
    let ai_summary = ai.endpoint_summary();
    let app = App::new(data);
    run_terminal(app, github, ai, ai_summary).await
}

async fn run_terminal(
    mut app: App,
    github: GitHubClient,
    ai: AiClient,
    ai_summary: String,
) -> Result<()> {
    enable_raw_mode().context("enable terminal raw mode")?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)
        .context("enter alternate terminal screen")?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).context("create Ratatui terminal")?;

    let result = event_loop(&mut terminal, &mut app, &github, &ai, &ai_summary).await;

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

async fn event_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    github: &GitHubClient,
    ai: &AiClient,
    ai_summary: &str,
) -> Result<()> {
    loop {
        terminal.draw(|frame| ui::draw(frame, app, ai_summary))?;
        if app.should_quit {
            return Ok(());
        }

        if !event::poll(Duration::from_millis(200)).context("poll terminal event")? {
            continue;
        }
        let Event::Key(key) = event::read().context("read terminal event")? else {
            continue;
        };
        if key.kind == KeyEventKind::Release {
            continue;
        }

        match key.code {
            KeyCode::Char('q') | KeyCode::Char('Q') if !app.busy => app.should_quit = true,
            KeyCode::Char('?') => app.show_help = !app.show_help,
            KeyCode::Esc if app.show_help => app.show_help = false,
            KeyCode::Tab if !app.show_help => app.toggle_focus(),
            KeyCode::Up if !app.show_help => app.move_up(),
            KeyCode::Down if !app.show_help => app.move_down(),
            KeyCode::Left if !app.show_help => app.collapse_selected(),
            KeyCode::Right if !app.show_help => app.expand_selected(),
            KeyCode::Enter if !app.show_help => app.toggle_selected(),
            KeyCode::PageUp if !app.show_help => app.page_up(),
            KeyCode::PageDown if !app.show_help => app.page_down(),
            KeyCode::Char('a') | KeyCode::Char('A') if !app.show_help && !app.busy => {
                app.busy = true;
                app.status = "Running independent AI review across scope/code/behavior/architecture/evidence...".into();
                terminal.draw(|frame| ui::draw(frame, app, ai_summary))?;
                match ai.review(&app.data).await {
                    Ok(report) => app.set_report(report),
                    Err(error) => {
                        app.status = format!("AI review failed: {error:#}");
                    }
                }
                app.busy = false;
            }
            KeyCode::Char('r') | KeyCode::Char('R') if !app.show_help && !app.busy => {
                app.busy = true;
                app.status = "Refreshing pull request, changed files and CI evidence...".into();
                terminal.draw(|frame| ui::draw(frame, app, ai_summary))?;
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

fn validate_repo(repository: &str) -> Result<()> {
    let mut parts = repository.split('/');
    let owner = parts.next().unwrap_or_default();
    let name = parts.next().unwrap_or_default();
    if owner.is_empty() || name.is_empty() || parts.next().is_some() {
        anyhow::bail!("--repo must use owner/name form, for example burncloud/burncloud");
    }
    Ok(())
}
