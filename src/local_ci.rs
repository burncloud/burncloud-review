use std::{
    collections::{HashMap, HashSet, VecDeque},
    fs::{self, File},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender},
        Arc,
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use anyhow::{anyhow, Context, Result};
use serde_json::Value;

use crate::models::{ChangedFile, CommitStatus};

#[derive(Debug, Clone)]
pub struct LocalCiConfig {
    pub repo: PathBuf,
    pub step_timeout: Duration,
}

pub struct LocalCiExecution {
    pub receiver: Receiver<LocalCiEvent>,
    pub cancel: Arc<AtomicBool>,
}

#[derive(Debug)]
pub enum LocalCiEvent {
    Started {
        worktree: String,
        packages: Vec<String>,
    },
    StepStarted {
        context: String,
        command: String,
    },
    StepFinished(CommitStatus),
    Finished {
        success: bool,
    },
    Failed {
        message: String,
    },
}

impl LocalCiConfig {
    pub fn start(
        &self,
        pr_number: u64,
        expected_sha: String,
        files: Vec<ChangedFile>,
    ) -> LocalCiExecution {
        let (tx, rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let task_cancel = Arc::clone(&cancel);
        let config = self.clone();
        thread::spawn(move || {
            if let Err(error) =
                run_local_ci(&config, pr_number, &expected_sha, &files, &tx, &task_cancel)
            {
                let _ = tx.send(LocalCiEvent::Failed {
                    message: format!("{error:#}"),
                });
            }
        });
        LocalCiExecution {
            receiver: rx,
            cancel,
        }
    }
}

fn run_local_ci(
    config: &LocalCiConfig,
    pr_number: u64,
    expected_sha: &str,
    files: &[ChangedFile],
    tx: &Sender<LocalCiEvent>,
    cancel: &Arc<AtomicBool>,
) -> Result<()> {
    let repo = config
        .repo
        .canonicalize()
        .with_context(|| format!("本地 BurnCloud 仓库不存在: {}", config.repo.display()))?;
    verify_git_repo(&repo)?;

    let review_ref = format!("refs/burncloud-review/pr-{pr_number}");
    let fetch_ref = format!("+refs/pull/{pr_number}/head:{review_ref}");
    run_simple(
        Command::new("git")
            .arg("-C")
            .arg(&repo)
            .arg("fetch")
            .arg("--force")
            .arg("origin")
            .arg(fetch_ref),
        "fetch PR branch",
    )?;

    let fetched_sha = command_text(
        Command::new("git")
            .arg("-C")
            .arg(&repo)
            .arg("rev-parse")
            .arg(&review_ref),
    )?;
    if fetched_sha.trim() != expected_sha.trim() {
        return Err(anyhow!(
            "本地 fetch 到的 PR SHA 与 GitHub 元数据不一致: fetched={} expected={}",
            fetched_sha.trim(),
            expected_sha
        ));
    }

    let worktree = worktree_path(pr_number);
    let _ = Command::new("git")
        .arg("-C")
        .arg(&repo)
        .arg("worktree")
        .arg("remove")
        .arg("--force")
        .arg(&worktree)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    if worktree.exists() {
        let _ = fs::remove_dir_all(&worktree);
    }
    if let Some(parent) = worktree.parent() {
        fs::create_dir_all(parent).context("create local CI worktree parent")?;
    }
    run_simple(
        Command::new("git")
            .arg("-C")
            .arg(&repo)
            .arg("worktree")
            .arg("add")
            .arg("--detach")
            .arg(&worktree)
            .arg(&review_ref),
        "create PR worktree",
    )?;

    let packages = affected_packages(&worktree, files).unwrap_or_default();
    let _ = tx.send(LocalCiEvent::Started {
        worktree: worktree.display().to_string(),
        packages: packages.clone(),
    });

    let target_dir = repo.join("target").join("burncloud-review");
    fs::create_dir_all(&target_dir).context("create shared Cargo target directory")?;

    let mut all_success = true;
    for step in build_steps(&packages) {
        if cancel.load(Ordering::Relaxed) {
            return Err(anyhow!("本地 CI 已取消"));
        }
        let _ = tx.send(LocalCiEvent::StepStarted {
            context: step.context.clone(),
            command: step.command_line(),
        });
        let result = run_step(&worktree, &target_dir, &step, config.step_timeout, cancel)?;
        if result.state != "success" {
            all_success = false;
        }
        let _ = tx.send(LocalCiEvent::StepFinished(result));
    }

    let _ = tx.send(LocalCiEvent::Finished {
        success: all_success,
    });
    Ok(())
}

fn verify_git_repo(repo: &Path) -> Result<()> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .arg("rev-parse")
        .arg("--is-inside-work-tree")
        .output()
        .context("run git rev-parse")?;
    if !output.status.success() || String::from_utf8_lossy(&output.stdout).trim() != "true" {
        return Err(anyhow!("{} 不是有效 Git 工作区", repo.display()));
    }
    Ok(())
}

#[derive(Clone)]
struct CiStep {
    context: String,
    args: Vec<String>,
}

impl CiStep {
    fn command_line(&self) -> String {
        format!("cargo {}", self.args.join(" "))
    }
}

fn build_steps(packages: &[String]) -> Vec<CiStep> {
    let mut selectors = Vec::new();
    if packages.is_empty() {
        selectors.push("--workspace".to_string());
    } else {
        for package in packages {
            selectors.push("-p".into());
            selectors.push(package.clone());
        }
    }

    let mut build = vec!["build".into()];
    build.extend(selectors.clone());
    let mut test = vec!["test".into()];
    test.extend(selectors.clone());
    let mut clippy = vec!["clippy".into()];
    clippy.extend(selectors);
    clippy.extend([
        "--all-targets".into(),
        "--".into(),
        "-D".into(),
        "warnings".into(),
    ]);

    vec![
        CiStep {
            context: "local/format".into(),
            args: vec!["fmt".into(), "--all".into(), "--".into(), "--check".into()],
        },
        CiStep {
            context: "local/build".into(),
            args: build,
        },
        CiStep {
            context: "local/test".into(),
            args: test,
        },
        CiStep {
            context: "local/clippy".into(),
            args: clippy,
        },
    ]
}

fn run_step(
    worktree: &Path,
    target_dir: &Path,
    step: &CiStep,
    timeout: Duration,
    cancel: &Arc<AtomicBool>,
) -> Result<CommitStatus> {
    let temp = CommandLogs::new(&step.context)?;
    let stdout = File::create(&temp.stdout).context("create local CI stdout log")?;
    let stderr = File::create(&temp.stderr).context("create local CI stderr log")?;
    let mut command = Command::new("cargo");
    command
        .args(&step.args)
        .current_dir(worktree)
        .env("CARGO_TARGET_DIR", target_dir)
        .env("CARGO_TERM_COLOR", "never")
        .env("NO_COLOR", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr));

    let started = Instant::now();
    let mut child = command
        .spawn()
        .with_context(|| format!("启动 {}", step.command_line()))?;
    let status = loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(anyhow!("本地 CI 已取消"));
        }
        if started.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err(anyhow!(
                "{} 超过 {} 秒，已终止",
                step.command_line(),
                timeout.as_secs()
            ));
        }
        if let Some(status) = child.try_wait().context("poll local CI command")? {
            break status;
        }
        thread::sleep(Duration::from_millis(200));
    };

    let output = read_logs(&temp);
    let success = status.success();
    let elapsed_ms = started.elapsed().as_millis().min(u64::MAX as u128) as u64;
    let mut result = CommitStatus {
        state: if success { "success" } else { "failure" }.into(),
        context: step.context.clone(),
        description: Some(if success { "通过" } else { "失败" }.into()),
        command: Some(step.command_line()),
        duration_ms: Some(elapsed_ms),
        exit_code: status.code(),
        output: Some(output),
    };
    result.description = Some(result.evidence_text());
    Ok(result)
}

fn affected_packages(worktree: &Path, files: &[ChangedFile]) -> Result<Vec<String>> {
    let output = Command::new("cargo")
        .arg("metadata")
        .arg("--no-deps")
        .arg("--format-version")
        .arg("1")
        .current_dir(worktree)
        .output()
        .context("run cargo metadata for local CI planning")?;
    if !output.status.success() {
        return Err(anyhow!(
            "cargo metadata failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let metadata: Value =
        serde_json::from_slice(&output.stdout).context("decode cargo metadata")?;
    let packages = metadata["packages"]
        .as_array()
        .ok_or_else(|| anyhow!("cargo metadata missing packages"))?;

    #[derive(Clone)]
    struct PackageInfo {
        name: String,
        root: PathBuf,
        deps: Vec<String>,
    }

    let mut infos = Vec::new();
    for package in packages {
        let Some(name) = package["name"].as_str() else {
            continue;
        };
        let Some(manifest) = package["manifest_path"].as_str() else {
            continue;
        };
        let root = PathBuf::from(manifest)
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| worktree.to_path_buf());
        let deps = package["dependencies"]
            .as_array()
            .map(|deps| {
                deps.iter()
                    .filter_map(|dep| dep["name"].as_str().map(ToOwned::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        infos.push(PackageInfo {
            name: name.to_string(),
            root,
            deps,
        });
    }

    let mut affected = HashSet::new();
    let mut full_workspace = false;
    for file in files {
        if file.filename == "Cargo.toml"
            || file.filename == "Cargo.lock"
            || file.filename.starts_with(".cargo/")
        {
            full_workspace = true;
            break;
        }
        let path = worktree.join(&file.filename);
        let best = infos
            .iter()
            .filter(|pkg| path.starts_with(&pkg.root))
            .max_by_key(|pkg| pkg.root.components().count());
        if let Some(pkg) = best {
            affected.insert(pkg.name.clone());
        }
    }
    if full_workspace || affected.is_empty() {
        return Ok(Vec::new());
    }

    let mut reverse: HashMap<String, Vec<String>> = HashMap::new();
    for pkg in &infos {
        for dep in &pkg.deps {
            reverse
                .entry(dep.clone())
                .or_default()
                .push(pkg.name.clone());
        }
    }
    let mut queue: VecDeque<String> = affected.iter().cloned().collect();
    while let Some(name) = queue.pop_front() {
        if let Some(users) = reverse.get(&name) {
            for user in users {
                if affected.insert(user.clone()) {
                    queue.push_back(user.clone());
                }
            }
        }
    }

    let mut result: Vec<String> = affected.into_iter().collect();
    result.sort();
    Ok(result)
}

fn worktree_path(pr_number: u64) -> PathBuf {
    std::env::temp_dir()
        .join("burncloud-review")
        .join("worktrees")
        .join(format!("pr-{pr_number}-{}", std::process::id()))
}

fn run_simple(command: &mut Command, label: &str) -> Result<()> {
    let output = command.output().with_context(|| format!("run {label}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(anyhow!(
            "{label} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

fn command_text(command: &mut Command) -> Result<String> {
    let output = command.output().context("run git command")?;
    if !output.status.success() {
        return Err(anyhow!(
            "git command failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

struct CommandLogs {
    stdout: PathBuf,
    stderr: PathBuf,
}

impl CommandLogs {
    fn new(context: &str) -> Result<Self> {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .context("system time before unix epoch")?
            .as_nanos();
        let safe = context.replace(['/', '\\'], "-");
        let dir = std::env::temp_dir().join("burncloud-review").join("logs");
        fs::create_dir_all(&dir).context("create local CI log directory")?;
        Ok(Self {
            stdout: dir.join(format!("{safe}-{stamp}.out.log")),
            stderr: dir.join(format!("{safe}-{stamp}.err.log")),
        })
    }
}

impl Drop for CommandLogs {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.stdout);
        let _ = fs::remove_file(&self.stderr);
    }
}

fn read_logs(logs: &CommandLogs) -> String {
    let stdout = fs::read_to_string(&logs.stdout).unwrap_or_default();
    let stderr = fs::read_to_string(&logs.stderr).unwrap_or_default();
    let combined = if stderr.trim().is_empty() {
        stdout
    } else if stdout.trim().is_empty() {
        stderr
    } else {
        format!("{stdout}\n{stderr}")
    };
    let sanitized = sanitize_terminal_output(&combined);
    truncate_tail(&sanitized, 16_000)
}

fn sanitize_terminal_output(value: &str) -> String {
    #[derive(Clone, Copy)]
    enum State {
        Text,
        Escape,
        Csi,
        Osc,
        OscEscape,
        StringControl,
        StringEscape,
    }

    let mut state = State::Text;
    let mut output = String::with_capacity(value.len());

    for ch in value.chars() {
        state = match state {
            State::Text => match ch {
                '\u{1b}' => State::Escape,
                '\r' => State::Text,
                '\n' | '\t' => {
                    output.push(ch);
                    State::Text
                }
                c if c.is_control() => State::Text,
                c => {
                    output.push(c);
                    State::Text
                }
            },
            State::Escape => match ch {
                '[' => State::Csi,
                ']' => State::Osc,
                'P' | 'X' | '^' | '_' => State::StringControl,
                _ => State::Text,
            },
            State::Csi => {
                if ('@'..='~').contains(&ch) {
                    State::Text
                } else {
                    State::Csi
                }
            }
            State::Osc => match ch {
                '\u{7}' => State::Text,
                '\u{1b}' => State::OscEscape,
                _ => State::Osc,
            },
            State::OscEscape => {
                if ch == '\\' {
                    State::Text
                } else if ch == '\u{1b}' {
                    State::OscEscape
                } else {
                    State::Osc
                }
            }
            State::StringControl => {
                if ch == '\u{1b}' {
                    State::StringEscape
                } else {
                    State::StringControl
                }
            }
            State::StringEscape => {
                if ch == '\\' {
                    State::Text
                } else if ch == '\u{1b}' {
                    State::StringEscape
                } else {
                    State::StringControl
                }
            }
        };
    }

    output
}

fn truncate_tail(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_string();
    }
    let mut start = value.len().saturating_sub(max_bytes);
    while !value.is_char_boundary(start) {
        start += 1;
    }
    format!("...（仅显示最后 {max_bytes} 字节）\n{}", &value[start..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn targeted_steps_use_package_selectors() {
        let steps = build_steps(&["burncloud-client".into(), "burncloud".into()]);
        assert!(steps[1].command_line().contains("-p burncloud-client"));
        assert!(steps[1].command_line().contains("-p burncloud"));
        assert_eq!(steps[2].context, "local/test");
    }

    #[test]
    fn empty_package_plan_uses_workspace() {
        let steps = build_steps(&[]);
        assert!(steps[1].command_line().contains("--workspace"));
    }

    #[test]
    fn terminal_output_strips_ansi_color_and_cursor_sequences() {
        let input = "before \u{1b}[31mred\u{1b}[0m \u{1b}[2J\u{1b}[Hafter";
        assert_eq!(sanitize_terminal_output(input), "before red after");
    }

    #[test]
    fn terminal_output_strips_osc_and_control_characters() {
        let input = "\u{1b}]0;window title\u{7}abc\rdef\u{8}g";
        assert_eq!(sanitize_terminal_output(input), "abcdefg");
    }
}