# Board Readability Plan

**Goal:** Keep full worktree names visible by continuing them on later lines, and make agent and terminal-session labels readable in light terminal themes.
**Files touched:** `board/src/ui.rs`, `board/src/app.rs`, `board/src/main.rs`, `panel.sh`, `herdr-plugin.toml`, `board/Cargo.toml`, `README.md`, `CONTRIBUTING.md`, `CHANGELOG.md`, `config.env.example`, `.gitignore`

---

## Handoff state

- Source: `/Users/d.mok/VSCodeProjects/herdr-worktree-agents`, currently has the local `fix: improve worktree board readability` commit on `main`.
- Worktree Agents is locally linked with `herdr plugin link /Users/d.mok/VSCodeProjects/herdr-worktree-agents --enabled` because `d-mok-pvt` receives GitHub 403 when pushing to `d-mok-uzum/herdr-worktree-agents`. Do not run `herdr plugin update --yes` before a push is possible.
- `panel.sh` reserves and labels a newly opened worktree-workspace board immediately, including during the interval before the board process labels itself, so overlapping focus hooks cannot create duplicate right splits. Its `toggle` action is idempotent: it shows the current workspace board; `q` hides all boards.
- `board/src/ui.rs` wraps branch labels in returned card lines, uses dark-gray agent/session and expanded commit-subject labels, and abbreviates managed checkout paths at `.herdr/worktrees/`. `board/src/app.rs` keeps the origin checkout first and ranks linked worktrees below it by attention. A manual board resize persists its width and synchronizes every open workspace board; launch-time resize events are ignored.

## Tasks

### 1. Cover branch continuations and light-theme label contrast [DONE:1]
- [x] Added a narrow `TestBackend` branch fixture that checks the complete wrapped branch and no ellipsis.
- [x] Added buffer-style assertions for dark-gray agent/session labels and the yellow working glyph.
- [x] Confirmed both new tests fail before the renderer changes.

### 2. Render complete branch names without crossing the card border [DONE:2]
- [x] `card()` reserves the first-line gap, then renders long branches on indented continuation lines.
- [x] `wrap()` preserves every character and does not append an ellipsis.
- [x] Continuations remain in `card()`'s returned `Vec<Line>`.

### 3. Use contrast-safe agent and session label styles [DONE:3]
- [x] `agent_line()` uses bold `Color::DarkGray` names and dark-gray terminal titles.
- [x] Glyphs and explicit state text retain their status colors.

### 4. Release and deploy [DONE:4]
- [x] Bumped the manifest and board crate to `0.1.4`; updated README and CHANGELOG.
- [x] Ran shell checks, formatting, tests, clippy with warnings denied, and a release build.
- [x] Commit: `git commit -m "fix: improve worktree board readability"`.
- [BLOCKED] Push remains blocked by the documented GitHub 403; the local linked plugin stays deployed and `herdr plugin update --yes` remains deferred.
