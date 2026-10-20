# Changelog

All notable changes to this plugin are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Fixed

- Worktree Agents now keeps one panel in every Git worktree workspace. Opening reserves and labels
  the new pane immediately, so overlapping hooks cannot create duplicate right splits in one workspace.
- The `toggle` action is idempotent: it always shows the current workspace board rather than closing
  one opened by another invocation; from a non-Git workspace it reveals or reopens the last board.
  Use `q` in the board to hide all boards.
- Long branch names wrap inside their card instead of truncating at the border.
- A manual panel resize is now saved and synchronized to every open worktree board; `WORKTREES_WIDTH`
  remains the initial width.
- The origin checkout remains first; linked worktrees are ordered by attention beneath it.
- Expanded cards dynamically omit everything before `.herdr/worktrees/` in managed checkout paths.
- Agent, terminal-session, and expanded-card commit-subject labels use a light-theme-safe dark gray
  while status glyphs retain their semantic colors.

### Added

- GitLab and self-hosted GitLab merge-request cards. GitLab remotes use `GITLAB_TOKEN`, show MR IID,
  draft state, and latest pipeline status for checked-out branches, and share the existing review cache.

### Removed

- `c` shortcut that opened a worktree and started Claude in a new pane.

## [0.1.2] - 2026-09-30

### Added

- Panel screenshot in the README.

## [0.1.1] - 2026-09-30

### Fixed

- The pane launcher now falls back to the bash board when the Rust binary cannot run (wrong
  architecture or a stale build) instead of exiting and closing the panel immediately.

## [0.1.0] - 2026-09-30

First release.

### Added

- Panel in the top-right corner listing every git worktree of the current project. It opens by itself
  on `startup`, `workspace.focused` and `worktree.created` in repos with two or more checkouts;
  `q` or the `toggle` action hides it everywhere.
- Per worktree card:
  - checkout marker (this workspace, open elsewhere, prunable), branch, changed files, commits
    ahead/behind the remote default branch;
  - one line per agent running in that checkout, most urgent first, naming which coding agent it is
    (`claude`, `codex`, `pi`, ... or herdr's `display_agent`) with its state glyph (`◉` needs you,
    braille spinner working, `✓` done, `●` idle) and terminal title; up to three, then `+N more`;
  - the last commit age;
  - open PR number, CI state and failing check count, unresolved review threads, review decision, draft.
- Agents are matched to the deepest worktree containing their working directory.
- Worktrees that need you sort first: waiting agent, failed CI, done, working, idle. Cards slide to their
  new slot on re-sort and pulse when an agent starts waiting on you.
- `Tab` details: last commit subject, diffstat bar, failing check names, checkout path.
- Keys: `Enter` open, `n` new worktree, `d` remove (confirmed, never forced, never the main checkout),
  `c` start `claude` in the worktree, `o` open the PR, `y` copy the path (OSC 52), `/` fuzzy filter,
  `r` refresh. Mouse: click to select, double-click to open, wheel to move.
- ANSI palette colors, so the panel follows the terminal theme.
- Tab-bar summary when the panel is hidden (`⎇ 3 ◉1 ✗1 ⠿2`) and a `$wt_pr` row in herdr's sidebar,
  added and removed by the `statusbar-install` / `statusbar-remove` actions. They validate the config
  with `herdr config check`, reload, restore the previous file if herdr rejects it, keep a one-time
  backup, and never merge into a `[ui]` table they did not write.
- One shared GitHub GraphQL query per interval for all open PRs, cached in the plugin state directory.
- Settings in `config.env` in the plugin config directory (see `config.env.example`).
- Rust board (ratatui) with a bash fallback: `scripts/build.sh` builds the Rust board when `cargo` is
  present and otherwise lets the install succeed with the bash board.
- Linux and macOS support. Windows is not supported.
