# BurnCloud Review

BurnCloud Review is an interactive pull-request review console for BurnCloud.

It is built around one rule:

> Review is not “the diff looks fine”. Review is evidence that the change stayed in scope, preserves architecture boundaries, behaves as intended, and is safe to merge.

The application uses a Ratatui terminal UI. A reviewer starts from the recent pull-request list, enters one PR, sees the whole change first, and then drills down through review gates, affected components, files, hunks, changed lines, local CI evidence, and AI findings.

## Quick start

Keep the repositories next to each other:

```text
Work/
├── burncloud-review/
└── burncloud/
```

Then run:

```bash
cargo run
```

BurnCloud Review opens a Ratatui picker for `burncloud/burncloud`.

Use `↑` / `↓` to select a PR and press `Enter` to open it. Press `Esc` inside a review to return to the PR list.

GitHub is used to read PR metadata and changed-file patches. **GitHub Actions is not the authoritative CI source.** Build and test evidence is produced directly on the reviewer's machine from `../burncloud`.

Public repositories can be read without a GitHub token, but GitHub rate limits unauthenticated requests. For regular use, set `GITHUB_TOKEN`.

PowerShell:

```powershell
$env:GITHUB_TOKEN="github_pat_xxx"
cargo run
```

## Local CI from `../burncloud`

Opening a PR does **not** execute PR code automatically. This matters because a pull request can contain arbitrary `build.rs`, proc macros, tests, or other code that would execute on the reviewer's machine.

After inspecting the PR, press `T` to explicitly run local CI.

BurnCloud Review then performs this flow:

```text
../burncloud
    │
    ├── git fetch refs/pull/<PR>/head
    │
    ├── verify fetched SHA == GitHub PR head SHA
    │
    └── temporary detached git worktree
            │
            ├── cargo fmt --all -- --check
            ├── cargo build <affected packages>
            ├── cargo test <affected packages>
            └── cargo clippy <affected packages> --all-targets -- -D warnings
```

The existing `../burncloud` branch, index, and uncommitted files are not switched or overwritten. The PR is tested in an isolated temporary worktree.

BurnCloud Review uses `cargo metadata` to map changed paths to workspace packages and then includes reverse workspace dependents. For example, a change in `burncloud-client` may also cause the top-level `burncloud` package to be built and tested. Changes to root workspace files fall back to the whole workspace.

Each CI step records:

- exact command
- running/completed state
- exit code
- elapsed time
- captured command output

A build or test is only marked `PASS` when the actual local process exits successfully.

Keyboard controls:

```text
T   run / rerun local CI
X   cancel active local CI
```

The default source repository is `../burncloud`. Override it with:

```bash
cargo run -- --local-repo /path/to/burncloud
```

or:

```bash
BCR_LOCAL_REPO=/path/to/burncloud cargo run
```

Each local CI command has a 30-minute default timeout. Override it with `--local-ci-timeout-secs` or `BCR_LOCAL_CI_TIMEOUT_SECS`.

## Local Codex reviewer

BurnCloud Review can directly use an installed and already-authenticated local Codex CLI. No OpenAI-compatible HTTP server is required when Codex is available.

Check that Codex works locally:

```bash
codex --version
```

The default AI backend is `auto`:

```text
auto
├── local Codex found  -> use Codex CLI
└── Codex not found    -> use OpenAI-compatible HTTP backend
```

To require local Codex and fail instead of falling back:

```bash
cargo run -- --ai-backend codex
```

The local reviewer is launched as an ephemeral, read-only Codex execution. BurnCloud Review supplies the PR metadata, patches, local CI evidence, and a strict JSON output schema. Codex is used as an independent reviewer and is not allowed to modify the repository during this review path.

Normally Codex uses the model already configured by the local CLI. To override it:

PowerShell:

```powershell
$env:BCR_CODEX_MODEL="your-model"
cargo run
```

Bash:

```bash
export BCR_CODEX_MODEL=your-model
cargo run
```

## HTTP AI backend

The OpenAI-compatible backend remains available. Force it with:

```bash
cargo run -- --ai-backend http
```

By default the HTTP backend expects a local BurnCloud Node:

```text
http://localhost:3000/v1
model = deepseek-v3
```

## Optional direct launch

Open another repository and choose a PR in the UI:

```bash
cargo run -- --repo owner/repository
```

Open a specific PR directly:

```bash
cargo run -- --repo burncloud/burncloud --pr 123
```

## Review model

Every pull request is inspected through five gates:

1. **Scope** — did the change stay inside the requested boundary?
2. **Code** — correctness, concurrency, error handling, performance, security.
3. **Behavior** — what user-visible or runtime execution paths changed?
4. **Architecture** — did any component cross its responsibility boundary?
5. **Evidence** — local build/test/lint execution, known limitations, and proof required before merge.

Risk levels are `R0` through `R4`:

| Risk | Typical changes | Minimum review intent |
|---|---|---|
| `R0` | docs / prose | deterministic evidence |
| `R1` | UI / low-impact tooling | AI review + local CI |
| `R2` | Node runtime / router / model / process / hardware | AI + human review + local CI |
| `R3` | Network control-plane / auth / security / identity | stronger human review |
| `R4` | billing / settlement / clearing / wallet / ledger | strongest evidence and multi-reviewer policy |

The deterministic path-based classifier runs before the model. A model may escalate risk, but it cannot lower a deterministic risk classification. Likewise, a failed local CI result cannot be promoted to PASS by a model response.

## Interactive flow

```text
Recent Pull Requests
        │
        │ Enter
        ▼
Pull Request
├── Review Gates
│   ├── Scope
│   ├── Code
│   ├── Behavior
│   ├── Architecture
│   └── Evidence
├── Affected Components
├── Changed Files
│   └── file
│       └── hunk
│           └── changed line
└── AI Findings
    └── BLOCKER / MAJOR / MINOR / NIT
```

## Keyboard

### PR picker

| Key | Action |
|---|---|
| `↑` / `↓` | select a recent pull request |
| `Enter` | open the selected PR |
| `r` | refresh the recent PR list |
| `q` / `Esc` | quit |

### PR review

| Key | Action |
|---|---|
| `↑` / `↓` | move through the visible hierarchy |
| `←` | collapse current node; if already closed, move to its parent |
| `→` | expand current node; if already open, move into its first child |
| `Enter` | toggle the current layer open / closed |
| `Tab` | switch focus between review tree and evidence/detail pane |
| `PgUp` / `PgDn` | scroll evidence/detail |
| `T` | run local CI from `../burncloud` in an isolated worktree |
| `X` | cancel active local CI |
| `A` | run the independent Codex / AI review |
| `C` | cancel active AI review |
| `R` | refresh PR metadata and patches from GitHub; local CI returns to not-run |
| `Esc` | return to the recent PR picker |
| `?` | toggle help |
| `q` | quit BurnCloud Review |

## Evidence policy

GitHub supplies the identity of the change: PR number, head SHA, metadata, changed files, and patches.

The local machine supplies execution evidence:

```text
GitHub PR metadata
        +
verified local fetched SHA
        +
local worktree
        +
real Cargo process exit codes
        =
BurnCloud Review CI evidence
```

This separation avoids treating a GitHub badge as proof that code was actually compiled or tested in the reviewer's environment.
