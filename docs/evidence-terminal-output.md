# Evidence terminal output safety

Local CI output is untrusted terminal text. A compiler, test binary, or PR-controlled program can emit ANSI/VT escape sequences that change colors, move the cursor, clear the terminal, or set terminal metadata.

BurnCloud Review therefore treats CI logs as data, not terminal commands:

1. Local Cargo commands run with `CARGO_TERM_COLOR=never` and `NO_COLOR=1`.
2. Captured stdout/stderr is sanitized before it is stored as Evidence.
3. ANSI CSI/OSC/string-control sequences and C0 cursor controls are removed while ordinary UTF-8, newlines and tabs are retained.
4. The sanitized text is then truncated and rendered by Ratatui.

This prevents PR-controlled build/test output from corrupting the TUI layout or injecting terminal control behavior.