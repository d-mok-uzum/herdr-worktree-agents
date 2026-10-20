# Multi-forge merge-request support Plan

**Goal:** Display branch-matched GitHub PRs and self-hosted GitLab MRs in every worktree card without exposing GitLab tokens.
**Files touched:** `board/src/data.rs`, `board/src/main.rs`, `board.sh`, `config.env.example`, `README.md`, `CHANGELOG.md`

---

## Tasks

### 1. Add tested forge identification and GitLab response normalization [DONE:1]
- [x] In `board/src/data.rs`, add provider/remote parsing for GitHub SSH/HTTPS URLs and GitLab SSH/HTTPS URLs, preserving host and URL-encoded GitLab project path.
- [x] Add pure helpers that map GitLab open-MR list responses into existing `PrInfo`, including branch, IID, URL, draft/WIP, and head-pipeline state.
- [x] Add unit tests in `board/src/data.rs` for remote parsing, GitLab path encoding, and pipeline-state normalization.
- [x] commit: `git commit -m "feat: identify GitLab review remotes"` (combined in `c534769`)

### 2. Collect and cache GitLab MRs safely [DONE:2]
- [x] In `board/src/data.rs`, retain GitHub `gh` GraphQL collection and add GitLab REST v4 collection for `GET /projects/:project/merge_requests?state=opened&scope=all&per_page=100`, then request MR detail only when its source branch has a local worktree so `head_pipeline` is available.
- [x] Send `PRIVATE-TOKEN` to `curl` through its stdin config, never argv; retain a previously valid cache if request or JSON parsing fails.
- [x] Derive API base as `https://<remote-host>/api/v4`; honor `WORKTREES_GITLAB_API_URL` when set; source authentication from `GITLAB_TOKEN`.
- [x] Cache per forge host and project with the current atomic temp-file rename flow and current refresh TTL.
- [x] commit: `git commit -m "feat: collect GitLab merge requests"` (combined in `c534769`)

### 3. Select collector by repository forge and retain bash fallback [DONE:3]
- [x] In `board/src/main.rs`, replace GitHub-only setup with provider dispatch: GitHub invokes existing `gh` collector; GitLab invokes REST collector; unknown remotes collect no review data.
- [x] Update `board.sh` to perform identical GitHub/GitLab remote selection and populate its shared normalized cache for GitLab, preserving the current shell fallback when Rust is unavailable.
- [x] Ensure provider failures leave panel, git status, agent data, and existing valid review cache usable.
- [x] commit: `git commit -m "feat: show GitLab MRs in worktrees"` (combined in `c534769`)

### 4. Document configuration and validate both paths [DONE:4]
- [x] Update `config.env.example` and `README.md` with GitLab/self-hosted support, `GITLAB_TOKEN`, `WORKTREES_GITLAB_API_URL`, field limits (no approval or unresolved-thread count), and token placement in plugin `config.env`.
- [x] Update `CHANGELOG.md` for multi-forge review support.
- [x] Run `cargo fmt --check`, `cargo test`, `cargo clippy --all-targets`; run shell syntax checks for `board.sh` and `scripts/launch.sh`.
- [x] Opened the panel in `qa-backend-tests`: MRs !2084 and !2092 matched the two checked-out branches and both reported failed latest pipelines. GitHub collector/provider regression is covered by its unchanged GraphQL path and remote-selection unit tests.
- [x] commit: `git commit -m "docs: document GitLab MR integration"`
