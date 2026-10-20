#!/usr/bin/env bash
# Usage: panel.sh toggle | ensure | off
#   toggle  re-enable and show the active worktree workspace's panel
#   ensure  (startup / workspace.focused / worktree.created hooks) auto-open in the active
#           worktree workspace when its repo has >= WORKTREES_MIN checkouts and the panel is missing
#   off     hide everywhere (the board's `q` key)
set -euo pipefail

cfg="${HERDR_PLUGIN_CONFIG_DIR:-}/config.env"
if [ -f "$cfg" ]; then set -a; . "$cfg"; set +a; fi

H=${HERDR_BIN_PATH:-herdr}
PLUGIN=${HERDR_PLUGIN_ID:-minhtran.worktrees}
STATE=${HERDR_PLUGIN_STATE_DIR:?run from herdr}
OFF=$STATE/disabled
LAST=$STATE/last-workspace
LEGACY_LAST=$STATE/last-source
LABEL="⎇ worktrees"
WIDTH_FILE=$STATE/width
SUPPRESS_FILE=$STATE/width-suppress-until
COOLDOWN_FILE=$STATE/width-sync-until
WIDTH=${WORKTREES_WIDTH:-38}
[ -r "$WIDTH_FILE" ] && IFS= read -r WIDTH < "$WIDTH_FILE" || true
MIN=${WORKTREES_MIN:-2}

# Herdr has no "list plugin panes" API, so the board labels its own pane.
# Output: "<tab_id> <pane_id> <workspace_id>".
board_panes() {
  "$H" pane list | jq -r --arg l "$LABEL" '.result.panes[] | select(.label == $l) | "\(.tab_id) \(.pane_id) \(.workspace_id)"'
}

# "<workspace_id> <active_tab_id>" of the focused workspace.
active() {
  "$H" workspace list | jq -r '.result.workspaces[] | select(.focused) | "\(.workspace_id) \(.active_tab_id)"'
}

source_workspace() {
  "$H" worktree list --workspace "$1" 2>/dev/null \
    | jq -er '.result.source.source_workspace_id // empty'
}

# Non-git workspaces make worktree list fail; single-checkout repos are not worth a panel.
has_worktrees() {
  local n
  n=$("$H" worktree list --workspace "$1" 2>/dev/null | jq '.result.worktrees | length') || return 1
  (( n >= MIN ))
}

# A just-opened pane may not have renamed itself yet. Remember its id immediately so a queued hook
# cannot open a second right split before `board_panes` can find the label.
board_file() {
  printf '%s/board-%s' "$STATE" "$1"
}

workspace_board_open() {
  local ws=$1 tab=$2 id file
  board_panes | awk '{print $1}' | grep -qxF "$tab" && return 0
  file=$(board_file "$ws")
  [ -r "$file" ] || return 1
  IFS= read -r id < "$file" || { rm -f "$file"; return 1; }
  "$H" pane list | jq -e --arg id "$id" '.result.panes[] | select(.pane_id == $id)' >/dev/null && return 0
  rm -f "$file"
  return 1
}

open_in_tab() {
  local tab=$1 ws=$2 anchor layout target new_id
  anchor=$("$H" pane list | jq -r --arg t "$tab" '[.result.panes[] | select(.tab_id == $t)][0].pane_id // empty')
  [ -n "$anchor" ] || return 0
  layout=$("$H" pane layout --pane "$anchor")
  # Top-right pane: furthest right edge, then smallest y.
  target=$(jq -r '.result.layout.panes | max_by([.rect.x + .rect.width, -.rect.y]) | .pane_id' <<<"$layout")
  # Ignore the launch-time terminal resize: it observes a transient split size, not a user choice.
  printf '%s\n' "$(( $(date +%s) + 15 ))" > "$SUPPRESS_FILE"
  new_id=$("$H" plugin pane open --plugin "$PLUGIN" --entrypoint board --placement split \
    --target-pane "$target" --direction right --no-focus | jq -r '.result.plugin_pane.pane.pane_id')
  printf '%s\n' "$new_id" > "$(board_file "$ws")"
  printf '%s\n' "$ws" > "$LAST"
  "$H" pane rename "$new_id" "$LABEL" >/dev/null 2>&1 || true
  fit_width "$new_id"
}

# Resize amounts are fractions of the parent split (the narrowest right-split holding the board).
fit_width() {
  local id=$1 w parent amount direction
  read -r w parent < <("$H" pane layout --pane "$id" | jq -r --arg id "$id" '.result.layout
    | (.panes[] | select(.pane_id == $id) | .rect) as $r
    | [.splits[] | select(.direction == "right" and .rect.x <= $r.x and .rect.x + .rect.width >= $r.x + $r.width
        and .rect.y <= $r.y and .rect.y + .rect.height >= $r.y + $r.height) | .rect.width] | min as $p
    | "\($r.width) \($p)"')
  (( w == WIDTH )) && return 0
  if (( w > WIDTH )); then direction=right; else direction=left; fi
  amount=$(awk -v w="$w" -v t="$WIDTH" -v p="$parent" 'BEGIN { printf "%.3f", (w > t ? w - t : t - w) / p }')
  "$H" pane resize --pane "$id" --direction "$direction" --amount "$amount" >/dev/null || true
}

sync_board_widths() {
  board_panes | while read -r _ id _ws; do fit_width "$id" || true; done
}

close_all() {
  # Re-linking the plugin drops herdr's ownership record, so fall back to a plain pane close.
  board_panes | while read -r _ id _ws; do
    "$H" plugin pane close "$id" >/dev/null 2>&1 || "$H" pane close "$id" >/dev/null 2>&1 || true
  done
  rm -f "$STATE"/board-*
}

# $1 = force: skip the worktree-count check (explicit toggle on).
ensure() {
  local force=${1:-} ws tab
  [ -f "$OFF" ] && return 0
  read -r ws tab < <(active) || return 0
  source_workspace "$ws" >/dev/null || return 0
  workspace_board_open "$ws" "$tab" && return 0
  [ -n "$force" ] || has_worktrees "$ws" || return 0
  open_in_tab "$tab" "$ws"
}

# Rapid workspace switches fire overlapping hooks; serialize them. macOS has no flock(1):
# mkdir is atomic there, and a stuck lock is ignored after ~5s rather than hanging a hook.
if command -v flock >/dev/null 2>&1; then
  exec 9>"$STATE/lock"
  flock 9
else
  for _ in $(seq 50); do mkdir "$STATE/lock.d" 2>/dev/null && break; sleep 0.1; done
  trap 'rmdir "$STATE/lock.d" 2>/dev/null' EXIT
fi

case ${1:-} in
  width)
    new_width=${2:-}
    [[ "$new_width" =~ ^[1-9][0-9]*$ ]] || { echo "usage: panel.sh width <columns>" >&2; exit 2; }
    if [ -r "$SUPPRESS_FILE" ]; then
      IFS= read -r suppress_until < "$SUPPRESS_FILE" || suppress_until=0
      if (( $(date +%s) < suppress_until )); then exit 0; fi
      rm -f "$SUPPRESS_FILE"
    fi
    [ "$WIDTH" = "$new_width" ] && exit 0
    if [ -r "$COOLDOWN_FILE" ]; then
      IFS= read -r cooldown_until < "$COOLDOWN_FILE" || cooldown_until=0
      (( $(date +%s) < cooldown_until )) && exit 0
    fi
    WIDTH=$new_width
    printf '%s\n' "$WIDTH" > "$WIDTH_FILE"
    # The other boards receive resize events from this sync. Ignore them briefly so they cannot
    # overwrite the width the user just chose in the resized board.
    printf '%s\n' "$(( $(date +%s) + 2 ))" > "$COOLDOWN_FILE"
    sync_board_widths ;;
  toggle)
    # This action is intentionally idempotent: `q` hides boards; `/worktrees` shows this workspace's board.
    read -r ws _ < <(active) || exit 0
    if ! source_workspace "$ws" >/dev/null; then
      # `/worktrees` may run from a non-Git agent workspace; reveal or reopen the last worktree board.
      board_ws=$(board_panes | awk 'NR == 1 { print $3 }')
      [ -n "$board_ws" ] || { [ -r "$LAST" ] && IFS= read -r board_ws < "$LAST" || true; }
      [ -n "${board_ws:-}" ] || { [ -r "$LEGACY_LAST" ] && IFS= read -r board_ws < "$LEGACY_LAST" || true; }
      [ -n "${board_ws:-}" ] || exit 0
      "$H" workspace focus "$board_ws" >/dev/null
      if [ -z "$(board_panes)" ]; then rm -f "$OFF"; ensure force; fi
      exit 0
    fi
    rm -f "$OFF"
    ensure force ;;
  ensure) ensure ;;
  off) touch "$OFF"; close_all ;;
  *) echo "usage: panel.sh width <columns>|toggle|ensure|off" >&2; exit 2 ;;
esac
