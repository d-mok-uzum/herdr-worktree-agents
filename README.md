# Worktree Agents

A [herdr](https://herdr.dev) plugin that keeps a panel in the top-right corner listing every git
worktree of the current project, with the state of the agents running in each one, its PR and CI,
and its git status. The origin checkout stays first; linked worktrees that need you sort beneath it.

![The Worktrees panel in a repo with three worktrees](docs/screenshots/panel.png)

```text
  edgeful-app         vs github/main
 4 worktrees  ⠙1                ⟳ now
──────────────────────────────────────
╭────────────────────────────────────╮
│▌ ◆ main                        ✎4  │
│▌   ⠙ claude · Herdr plugin wor… 7h │
│▌   ● claude · New comment explana… │
│▌   ● codex · Edgeful-app simulate… │
│▌   no PR                           │
╰────────────────────────────────────╯
▌   chore/report-size-bounds… ✎2 ↑3↓54
▌   · no agent                    15d
▌   no PR

▌   fix/tradingview-username… ✎4 ↑5↓15
▌   ◉ codex · needs you · Review…  2d
▌    1309 ✓  5
──────────────────────────────────────
 ↵ open  ⇥ info  n new  d del
 o PR  y copy  / find
 q hide
```

## What each card shows

| Line | Content |
|---|---|
| First | `◆` this workspace's checkout, `◇` open in another workspace, `✗` prunable; branch; `✎` changed files, `↑`/`↓` commits ahead/behind the remote default branch. Branch names continue on indented lines when necessary; no branch characters are truncated at the card border. |
| Agents | One line per agent running in that checkout, most urgent first: state glyph (`◉` needs you, braille spinner working, `✓` done, `●` idle), **which coding agent** (`claude`, `codex`, `pi`, `opencode`, … or the pane's reported display name), and its terminal title. Labels use a theme-safe muted color while state glyphs and `needs you` retain their status color. Up to 3 lines, then `+N more`. The first line also shows the age of the last commit |
| Last | Open PR/MR number, CI (`✓` / `✗N` failing checks / spinner running), unresolved review threads, `✔` approved or `±` changes requested, draft. GitLab rows currently omit review-thread and approval data. |

`Tab` expands the selected card with the last commit subject, a diffstat bar, failing check names and
the checkout path. Managed worktree paths omit everything before `.herdr/worktrees/`. Selection uses only
a muted background, so moving it never resizes or shifts cards. The card's left bar is red when an agent is
waiting on you or CI failed, yellow while working, green when done. A card pulses when its agent starts
waiting on you, and cards slide into place when the order changes.

Agents are matched to the deepest worktree containing their working directory, so an agent in
`.worktrees/feat` is not counted under the main checkout.

## Platforms

| OS | Status |
|---|---|
| Linux | Supported. Tested on Ubuntu with herdr 0.9.1 and Ghostty |
| macOS | Supported. The scripts avoid GNU-only tools, but this setup is less tested than Linux |
| Windows | Not supported. The hooks and actions are shell scripts, and herdr's Windows plugin support is still a preview |

The manifest declares `platforms = ["linux", "macos"]`, so on Windows herdr answers its actions and
panes with `platform_unsupported`.

## Requirements

- herdr 0.9.1 or newer
- `git`, `jq`
- `gh`, signed in, for GitHub PR and CI data (optional for GitLab projects)
- `curl` for GitLab and self-hosted GitLab MR data
- Rust (`cargo`) for the full panel. Without it the plugin installs a simpler bash panel, which needs
  bash 4+ (macOS: `brew install bash`)
- A Nerd Font for the PR and comment icons, or set `WORKTREES_ICONS=plain`. Ghostty bundles the glyphs

## Install

From GitHub (runs `scripts/build.sh`, which builds the Rust panel when `cargo` is available):

```bash
herdr plugin install <owner>/<repo>
```

From a local checkout (`link` does not run build steps, so build first):

```bash
cargo build --release --manifest-path board/Cargo.toml
herdr plugin link "$PWD"
```

The panel opens by itself in each focused Git worktree workspace when its repository has two or more checkouts. Each workspace gets its own right-side panel.

### Keybinding

Plugins cannot declare keys. Add this block (without replacing existing configuration) to
`~/.config/herdr/config.toml`, then run `herdr server reload-config`:

```toml
[[keys.command]]
key = "ctrl+alt+w"
type = "plugin_action"
command = "minhtran.worktrees.toggle"
description = "show worktrees"
```

`prefix+w` is intentionally not used: Herdr reserves it for workspace navigation. The default
prefix is `ctrl+b` (press and release it, then the next key).

### Tab-bar summary and sidebar PR row (optional)

When the panel is hidden, a one-line summary such as `⎇ 3 ◉1 ✗1 ⠿2` can sit at the right of the tab
bar, and each worktree workspace can show `#1309 ✓ 💬5` in herdr's sidebar:

```bash
herdr plugin action invoke minhtran.worktrees.statusbar-install
```

This appends a marked block to `config.toml`, checks it with `herdr config check`, reloads, and puts
the old file back if herdr rejects it. A copy of the config from before the plugin first touched it is kept as `config.toml.bak-worktrees`. If your config
already has its own `[ui]` section the action stops and prints the snippet to merge by hand
(`herdr plugin log list --plugin minhtran.worktrees`). Note that the block sets
`[ui.sidebar.spaces] rows`, replacing a custom Space row layout.

## Using the panel

| Key | Action |
|---|---|
| `j` `k` / arrows / mouse wheel | Move |
| `Enter` / double-click | Open or focus the worktree |
| `Tab` / `Space` | Expand details |
| `n` | New worktree from the remote default branch (`herdr worktree create`) |
| `d` | Remove the checkout, after `y` to confirm (never the main checkout; never forced) |
| `o` | Open the PR in the browser |
| `y` | Copy the checkout path (OSC 52) |
| `/` | Filter by branch (fuzzy); `Esc` clears |
| `r` | Refresh now |
| `q` | Hide the panel everywhere and stop auto-opening |

The `toggle` action is idempotent: it enables and shows the current workspace's board, even in a single-checkout repo. From a non-Git workspace, it reveals an already-open board or reopens the last worktree board. Use `q` in a board to hide every board and disable auto-open.

## Configuration

Copy `config.env.example` into the plugin's config directory and edit it:

```bash
cp config.env.example "$(herdr plugin config-dir minhtran.worktrees)/config.env"
```

| Setting | Default | Meaning |
|---|---|---|
| `WORKTREES_WIDTH` | `38` | Initial panel width in columns; a manual resize is saved and synchronized to every board |
| `WORKTREES_MIN` | `2` | Auto-open only in repos with at least this many checkouts |
| `WORKTREES_PR_TTL` | `60` | Seconds between PR/CI refreshes |
| `WORKTREES_GIT_EVERY` | `10` | Seconds between git scans |
| `WORKTREES_GH` | `/usr/bin/gh`, else `gh` | GitHub CLI to use |
| `GITLAB_TOKEN` | unset | Token for GitLab/self-hosted GitLab MR API; set it in `config.env` when Herdr service does not inherit shell variables |
| `WORKTREES_GITLAB_API_URL` | `https://<remote-host>/api/v4` | Override GitLab API root for nonstandard deployments |
| `WORKTREES_ICONS` | Nerd Font | `plain` for ASCII icons |

`WORKTREES_WIDTH` supplies the initial width for new boards. A manual panel resize becomes the saved width for every open board.

## How it works

- `panel.sh` runs on the `startup`, `workspace.focused` and `worktree.created` hooks. It opens one
  `board` pane in each focused Git worktree workspace as a right split of its top-right pane. A manual
  resize is saved and propagated to every open board. Herdr has no API that lists plugin panes, so each board names its pane
  `⎇ worktrees` and `panel.sh` finds boards by that label.
- The board (`board/`, Rust + ratatui) polls three sources on separate threads: herdr (worktrees and
  agent panes, every 1.5s), git (every `WORKTREES_GIT_EVERY`) and review data (every
  `WORKTREES_PR_TTL`). GitHub uses one `gh` GraphQL query for open PRs. GitLab derives its API root
  from Git SSH or HTTP(S) remotes, including `ssh://` URLs, lists open MRs, then requests pipeline
  detail only for branches checked out locally. For review detection, a parseable `origin` always
  wins; when `origin` is absent or unparseable, the first parseable remote is used. The board and
  tab-bar status use this same rule. Both results use a
  provider-specific cache in the plugin state directory shared by every panel.
- `wt-board --status` is the one-shot command behind the tab-bar entry. It reads the same cache, prints
  nothing while the active tab shows the panel, and publishes `$wt_pr` workspace tokens.

State (the `disabled` flag, saved panel width, PR cache, published sidebar tokens) lives in the plugin
state directory, `$HERDR_PLUGIN_STATE_DIR` (on Linux `~/.local/state/herdr/plugins/minhtran.worktrees`).
Nothing is written to the plugin checkout.

## Uninstall

```bash
herdr plugin action invoke minhtran.worktrees.statusbar-remove   # if you installed the summary
herdr plugin uninstall minhtran.worktrees
```

Remove the keybinding from `config.toml` if you added one.

## Troubleshooting

- Hook and action output: `herdr plugin log list --plugin minhtran.worktrees`
- The panel never opens: the repo needs `WORKTREES_MIN` checkouts, or you hid it with `q`. Run the
  toggle action to re-enable it.
- A panel still runs old code after a Rust update: press `q` in any board to close all boards, then
  focus each worktree workspace (or invoke the toggle action there) to open fresh boards. After
  `herdr plugin unlink` + `link`, close any stale panes the same way before reopening them.
- PR/MR line always empty: for GitHub, check `gh auth status` or `WORKTREES_GH`; for GitLab, set
  `GITLAB_TOKEN` in the plugin `config.env`, then press `q` and reopen the board. GitLab supports MR,
  draft, and latest pipeline status; approval and unresolved-thread counts are intentionally not fetched.

## Development

See [CONTRIBUTING.md](CONTRIBUTING.md) for setup, checks and guidelines. In short:

```bash
cd board
cargo test
cargo fmt --check
cargo clippy --all-targets
# Render one frame as text, without a pane:
HERDR_WORKSPACE_ID=<id> HERDR_PLUGIN_STATE_DIR="$HOME/.local/state/herdr/plugins/minhtran.worktrees" \
  cargo run --release -- --snapshot 38x30
```

A running board can be driven with `herdr pane send-keys <pane> j` and raw SGR mouse input through
`herdr pane send-text <pane> $'\e[<0;6;5M'`.

To list the plugin in the [herdr marketplace](https://herdr.dev/plugins/), push it to a public GitHub
repository and add the `herdr-plugin` topic.

## License

MIT. See [LICENSE](LICENSE).
