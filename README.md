# BurnCloud Review

BurnCloud Review is an interactive pull-request review console for BurnCloud.

It is built around one rule:

> Review is not “the diff looks fine”. Review is evidence that the change stayed in scope, preserves architecture boundaries, behaves as intended, and is safe to merge.

The application uses a Ratatui terminal UI and an OpenAI-compatible LLM endpoint. Reviewers start from the whole pull request, then drill down through review gates, affected components, files, hunks, lines, and AI findings.

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

The deterministic path-based classifier runs before the LLM. The model may escalate risk, but it is never the only source of truth.

## Run

```bash
cargo run -- --repo burncloud/burncloud --pr 123
```

GitHub authentication:

```bash
export GITHUB_TOKEN=github_pat_xxx
```

### AI connection

By default BurnCloud Review expects a local BurnCloud Node:

```text
http://localhost:3000/v1
model = deepseek-v3
```

So once BurnCloud Node is running, the reviewer can use the same local `/v1` contract:

```bash
export BCR_AI_BASE_URL=http://localhost:3000/v1
export BCR_AI_MODEL=deepseek-v3
cargo run -- --repo burncloud/burncloud --pr 123
```

Any OpenAI-compatible provider can be used instead:

```bash
export BCR_AI_BASE_URL=https://provider.example/v1
export BCR_AI_API_KEY=...
export BCR_AI_MODEL=your-review-model
```

The coding agent and review model should be treated as separate roles. BurnCloud Review prompts the model as an adversarial reviewer and asks it to report missing evidence instead of inventing source facts.

## Interactive navigation

```text
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

The first screen deliberately stays at the PR level. Nothing forces a reviewer to read hundreds of diff lines before understanding risk and scope.

### Keyboard

| Key | Action |
|---|---|
| `↑` / `↓` | move through the visible hierarchy |
| `←` | collapse current node; if already closed, move to its parent |
| `→` | expand current node; if already open, move into its first child |
| `Enter` | toggle the current layer open / closed |
| `Tab` | switch focus between review tree and evidence/detail pane |
| `PgUp` / `PgDn` | scroll evidence/detail |
| `a` | run independent AI review |
| `r` | refresh PR metadata, patches and CI from GitHub |
| `?` | toggle help |
| `q` | quit |

Suggested reviewer path:

```text
PR
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

## What v0.1 already does

- Pulls PR metadata, changed files and commit status from GitHub.
- Parses unified patches into file → hunk → changed-line hierarchy.
- Assigns a deterministic `R0`–`R4` preflight risk level.
- Infers initial affected components from changed paths.
- Provides fully keyboard-driven Ratatui navigation.
- Connects to an OpenAI-compatible LLM for the five-gate independent review.
- Requires the model to return structured JSON findings and gate results.
- Anchors findings to file/line only when the supplied patch supports that location.
- Keeps AI findings separate from the reviewer’s final decision.

## Next layers

The next milestones are deliberately separate from the first usable console:

- GitHub inline review submission / request-changes / approve actions.
- BurnCloud architecture policy-as-code loaded from the documentation repository.
- Before/after E2E request-flow comparison.
- Test/evidence ingestion from GitHub Actions check runs and artifacts.
- Multi-model reviewer roles (code, architecture, tests/security) with consensus and disagreement display.
- Persisted review sessions and reproducible signed review reports.
