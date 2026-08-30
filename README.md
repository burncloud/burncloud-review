# BurnCloud Review

BurnCloud Review is an interactive pull-request review console for BurnCloud.

It is built around one rule:

> Review is not “the diff looks fine”. Review is evidence that the change stayed in scope, preserves architecture boundaries, behaves as intended, and is safe to merge.

The application uses a Ratatui terminal UI. A reviewer starts from the recent pull-request list, enters one PR, sees the whole change first, and then drills down through review gates, affected components, files, hunks, changed lines, and AI findings.

## Quick start

For the main BurnCloud repository, the normal workflow is now just:

```bash
cargo run
```

BurnCloud Review opens a Ratatui picker for `burncloud/burncloud`:

```text
Recent Pull Requests

▶ #412  [OPEN]   Add model runtime discovery
  #411  [OPEN]   Fix provider routing
  #410  [MERGED] Improve network status
  ...
```

Use `↑` / `↓` to select a PR and press `Enter` to open it. Press `Esc` inside a review to return to the PR list.

Public repositories can be read without a GitHub token, but GitHub rate limits unauthenticated requests. For regular use, set `GITHUB_TOKEN`.

PowerShell:

```powershell
$env:GITHUB_TOKEN="github_pat_xxx"
cargo run
```

Bash:

```bash
export GITHUB_TOKEN=github_pat_xxx
cargo run
```

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

The local reviewer is launched as an ephemeral, read-only Codex execution. BurnCloud Review supplies the PR metadata, patches, CI evidence, and a strict JSON output schema. Codex is used as an independent reviewer and is not allowed to modify the repository during this review path.

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

If Codex is installed somewhere unusual, specify the executable explicitly:

```bash
cargo run -- --ai-backend codex --codex-bin /path/to/codex
```

## HTTP AI backend

The previous OpenAI-compatible backend remains available. Force it with:

```bash
cargo run -- --ai-backend http
```

By default the HTTP backend expects a local BurnCloud Node:

```text
http://localhost:3000/v1
model = deepseek-v3
```

Configure another OpenAI-compatible endpoint with:

```bash
export BCR_AI_BASE_URL=https://provider.example/v1
export BCR_AI_API_KEY=...
export BCR_AI_MODEL=your-review-model
cargo run -- --ai-backend http
```

## Optional direct launch

The interactive PR picker is the default, but command-line shortcuts are still supported.

Open another repository and choose a PR in the UI:

```bash
cargo run -- --repo owner/repository
```

Open a specific PR directly:

```bash
cargo run -- --repo burncloud/burncloud --pr 123
```

The default repository can also be changed with `BCR_REPO`.

## Review model

Every pull request is inspected through five gates:

1. **Scope** — did the change stay inside the requested boundary?
2. **Code** — correctness, concurrency, error handling, performance, security.
3. **Behavior** — what user-visible or runtime execution paths changed?
4. **Architecture** — did any component cross its responsibility boundary?
5. **Evidence** — build, tests, CI, known limitations, and proof required before merge.

Risk levels are `R0` through `R4`:

| Risk | Typical changes | Minimum review intent |
|---|---|---|
| `R0` | docs / prose | CI evidence |
| `R1` | UI / low-impact tooling | AI review + CI |
| `R2` | Node runtime / router / model / process / hardware | AI + human review |
| `R3` | Network control-plane / auth / security / identity | stronger human review |
| `R4` | billing / settlement / clearing / wallet / ledger | strongest evidence and multi-reviewer policy |

The deterministic path-based classifier runs before the model. A model may escalate risk, but it cannot lower a deterministic risk classification. Likewise, failed or unknown hard CI evidence cannot be promoted to PASS by a model response.

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

The review screen deliberately starts at the PR level. Nothing forces a reviewer to read hundreds of diff lines before understanding scope, risk, and impact.

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
| `a` | run the independent Codex / AI review |
| `r` | refresh PR metadata, patches and CI from GitHub |
| `Esc` | return to the recent PR picker |
| `?` | toggle help |
| `q` | quit BurnCloud Review |

Suggested reviewer path:

```text
Recent PR list
↓
PR overview
↓
Risk + Review Gates
↓
Affected Components
↓
Changed Files
↓
Hunks
↓
Changed Lines
↓
AI Findings + Evidence
↓
Human merge decision
```

## What the current version does

- Opens directly into a recent-PR Ratatui picker when no PR number is supplied.
- Defaults to the `burncloud/burncloud` repository.
- Keeps `--repo` and `--pr` as optional direct-launch shortcuts.
- Detects and uses a local Codex CLI by default when available.
- Runs the Codex review in an ephemeral read-only sandbox and requests structured JSON output.
- Keeps the OpenAI-compatible HTTP reviewer as an explicit backend and automatic fallback.
- Pulls PR metadata, changed files and commit status from GitHub.
- Parses unified patches into file → hunk → changed-line hierarchy.
- Assigns a deterministic `R0`–`R4` preflight risk level.
- Infers initial affected components from changed paths.
- Provides fully keyboard-driven Ratatui navigation.
- Anchors findings to file/line only when the supplied patch supports that location.
- Keeps model findings separate from the reviewer’s final decision.

## Next layers

- GitHub inline review submission / request-changes / approve actions.
- BurnCloud architecture policy-as-code loaded from the documentation repository.
- Before/after E2E request-flow comparison.
- Test/evidence ingestion from GitHub Actions check runs and artifacts.
- Multi-model reviewer roles (code, architecture, tests/security) with consensus and disagreement display.
- Persisted review sessions and reproducible signed review reports.
