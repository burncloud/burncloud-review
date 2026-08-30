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

Risk levels are `R0` through `R4`. Network control-plane, identity, security, billing, and settlement changes are intentionally classified more strictly.

## Run

```bash
cargo run -- --repo burncloud/burncloud --pr 123
```

GitHub authentication:

```bash
export GITHUB_TOKEN=github_pat_xxx
```

AI review uses an OpenAI-compatible endpoint:

```bash
export BCR_AI_BASE_URL=https://api.openai.com/v1
export BCR_AI_API_KEY=...
export BCR_AI_MODEL=gpt-5.6
```

The AI endpoint is configurable so BurnCloud Review can point at BurnCloud itself or any compatible provider later.

## Keyboard

| Key | Action |
|---|---|
| `↑` / `↓` | move selection |
| `←` | collapse selected node / go toward parent |
| `→` | expand selected node |
| `Enter` | expand and inspect the selected layer |
| `Tab` | switch focus between review tree and detail pane |
| `PgUp` / `PgDn` | scroll detail pane |
| `a` | run independent AI review |
| `r` | refresh pull request from GitHub |
| `?` | toggle help |
| `q` | quit |

## Navigation hierarchy

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

## Status

The first implementation focuses on a real interactive review loop. GitHub review submission and policy-as-code enforcement can be layered on after the reviewer experience is stable.
