use crate::models::{ChangedFile, ExecutionRisk, SandboxKind};

#[derive(Debug, Clone)]
pub struct StaticExecutionAssessment {
    pub risk: ExecutionRisk,
    pub recommended_sandbox: SandboxKind,
    pub reasons: Vec<String>,
    pub dangerous_files: Vec<String>,
}

impl StaticExecutionAssessment {
    pub fn safe_for_host(&self) -> bool {
        self.risk == ExecutionRisk::Low && self.recommended_sandbox == SandboxKind::Host
    }
}

pub fn assess(files: &[ChangedFile]) -> StaticExecutionAssessment {
    let mut risk = ExecutionRisk::Low;
    let mut recommended_sandbox = SandboxKind::Host;
    let mut reasons = Vec::new();
    let mut dangerous_files = Vec::new();

    for file in files {
        let path = file.filename.replace('\\', "/").to_ascii_lowercase();
        let patch = file.patch.as_deref().unwrap_or_default().to_ascii_lowercase();

        if file.patch.is_none() {
            escalate(&mut risk, ExecutionRisk::Block);
            recommended_sandbox = SandboxKind::None;
            reasons.push(format!("{} 缺少可审查 Patch，无法证明执行安全", file.filename));
            dangerous_files.push(file.filename.clone());
            continue;
        }

        if is_execution_hook(&path) {
            escalate(&mut risk, ExecutionRisk::High);
            recommended_sandbox = SandboxKind::DisposableVm;
            reasons.push(format!("{} 属于构建/执行钩子，可能在 CI 阶段自动执行", file.filename));
            dangerous_files.push(file.filename.clone());
        }

        if is_ci_or_script(&path) {
            escalate(&mut risk, ExecutionRisk::High);
            recommended_sandbox = SandboxKind::DisposableVm;
            reasons.push(format!("{} 可改变 CI/脚本执行行为", file.filename));
            dangerous_files.push(file.filename.clone());
        }

        if path.ends_with("cargo.toml") || path.ends_with("cargo.lock") {
            escalate(&mut risk, ExecutionRisk::Medium);
            recommended_sandbox = recommended_sandbox.max(SandboxKind::Container);
            reasons.push(format!("{} 修改依赖/构建图，需要隔离执行", file.filename));
        }

        for (needle, message, level) in DANGEROUS_PATTERNS {
            if patch.contains(needle) {
                escalate(&mut risk, *level);
                recommended_sandbox = match level {
                    ExecutionRisk::Block => SandboxKind::None,
                    ExecutionRisk::High => SandboxKind::DisposableVm,
                    ExecutionRisk::Medium => recommended_sandbox.max(SandboxKind::Container),
                    ExecutionRisk::Low => recommended_sandbox,
                };
                reasons.push(format!("{}: {}", file.filename, message));
                if *level >= ExecutionRisk::High {
                    dangerous_files.push(file.filename.clone());
                }
            }
        }
    }

    dangerous_files.sort();
    dangerous_files.dedup();
    reasons.sort();
    reasons.dedup();

    StaticExecutionAssessment {
        risk,
        recommended_sandbox,
        reasons,
        dangerous_files,
    }
}

fn escalate(current: &mut ExecutionRisk, next: ExecutionRisk) {
    if next > *current {
        *current = next;
    }
}

fn is_execution_hook(path: &str) -> bool {
    path.ends_with("build.rs")
        || path.contains("/.cargo/config")
        || path.ends_with(".cargo/config")
        || path.ends_with(".cargo/config.toml")
        || path.contains("proc-macro")
        || path.contains("proc_macro")
}

fn is_ci_or_script(path: &str) -> bool {
    path.starts_with(".github/workflows/")
        || path.starts_with("scripts/")
        || path.ends_with(".ps1")
        || path.ends_with(".bat")
        || path.ends_with(".cmd")
        || path.ends_with(".sh")
}

const DANGEROUS_PATTERNS: &[(&str, &str, ExecutionRisk)] = &[
    ("std::process::command", "新增或修改子进程执行", ExecutionRisk::High),
    ("command::new", "新增或修改命令执行", ExecutionRisk::High),
    ("powershell", "涉及 PowerShell 执行", ExecutionRisk::High),
    ("cmd.exe", "涉及 Windows shell 执行", ExecutionRisk::High),
    ("bash -c", "涉及 shell 命令执行", ExecutionRisk::High),
    ("sh -c", "涉及 shell 命令执行", ExecutionRisk::High),
    ("invoke-webrequest", "脚本可从网络下载内容", ExecutionRisk::High),
    ("curl ", "脚本/代码可能从网络下载或发送数据", ExecutionRisk::High),
    ("wget ", "脚本可能从网络下载内容", ExecutionRisk::High),
    ("remove_dir_all", "代码可递归删除目录", ExecutionRisk::High),
    ("remove-item -recurse", "脚本可递归删除文件", ExecutionRisk::High),
    ("reg delete", "脚本可修改 Windows 注册表", ExecutionRisk::High),
    ("setx ", "脚本可修改宿主环境变量", ExecutionRisk::High),
    ("libloading", "涉及动态库加载", ExecutionRisk::High),
    ("extern \"c\"", "涉及 FFI 边界", ExecutionRisk::Medium),
    ("unsafe {", "新增 unsafe 代码路径", ExecutionRisk::Medium),
    ("reqwest::", "代码可主动发起网络请求", ExecutionRisk::Medium),
    ("tcpstream", "代码可建立原始网络连接", ExecutionRisk::Medium),
    ("std::env::var", "代码读取环境变量，需防止凭据泄露", ExecutionRisk::Medium),
    ("github_token", "涉及 GitHub 凭据名称", ExecutionRisk::High),
    ("api_key", "涉及 API Key/凭据名称", ExecutionRisk::High),
    ("secret", "涉及 secret/凭据处理", ExecutionRisk::Medium),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str, patch: Option<&str>) -> ChangedFile {
        ChangedFile {
            filename: path.into(),
            status: "modified".into(),
            additions: 1,
            deletions: 0,
            changes: 1,
            patch: patch.map(str::to_string),
        }
    }

    #[test]
    fn ordinary_ui_patch_can_use_host() {
        let result = assess(&[file("crates/client/src/app.rs", Some("+let title = value;"))]);
        assert_eq!(result.risk, ExecutionRisk::Low);
        assert!(result.safe_for_host());
    }

    #[test]
    fn build_script_requires_disposable_vm() {
        let result = assess(&[file("build.rs", Some("+fn main() {}"))]);
        assert_eq!(result.risk, ExecutionRisk::High);
        assert_eq!(result.recommended_sandbox, SandboxKind::DisposableVm);
        assert!(!result.safe_for_host());
    }

    #[test]
    fn command_execution_escalates_to_high() {
        let result = assess(&[file(
            "src/lib.rs",
            Some("+std::process::Command::new(\"powershell\")"),
        )]);
        assert_eq!(result.risk, ExecutionRisk::High);
        assert!(!result.safe_for_host());
    }

    #[test]
    fn missing_patch_blocks_execution() {
        let result = assess(&[file("src/lib.rs", None)]);
        assert_eq!(result.risk, ExecutionRisk::Block);
        assert_eq!(result.recommended_sandbox, SandboxKind::None);
    }
}
