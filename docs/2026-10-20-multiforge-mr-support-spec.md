# Multi-forge merge-request support

## Goal

Make Worktree Agents show an open review for the branch in each worktree when the repository is hosted on GitHub or GitLab, including self-hosted GitLab such as `git.uzum.io`.

## Scope

- Fork maintained at `d-mok-uzum/herdr-worktree-agents`.
- GitHub behavior remains the existing `gh` GraphQL collector.
- GitLab repositories use GitLab REST API v4 with the host and project path derived from the selected Git remote.
- GitLab authentication uses `GITLAB_TOKEN`. The plugin also loads it from its persistent `config.env`, so a Herdr service does not need to inherit an interactive shell environment.
- GitLab collector lists open merge requests once per refresh interval, normalizes data into the existing cache/row format, and matches each worktree by source branch.
- GitLab rows provide MR IID, web URL, draft status, and latest head-pipeline status. `o` opens the MR URL.
- GitLab review approvals and unresolved-thread counts are not collected in this change; they require extra calls for every MR.

## Provider selection

1. Resolve the remote used by the repository default branch.
2. A `github.com` remote selects the current GitHub collector.
3. Any other parseable SSH or HTTP(S) Git remote selects GitLab REST, using `https://<remote-host>/api/v4`.
4. If remote parsing, token lookup, or API calls fail, keep the worktree panel functional and show no MR data for that refresh. Never fall back to a different provider.

This deliberately favors self-hosted GitLab. A non-GitHub forge without a GitLab API simply has no review data.

## Security and reliability

- Pass GitLab token to HTTP client through standard input/configuration, not process command-line arguments.
- Cache separately by provider host and project path; use existing atomic temp-file rename behavior.
- Do not log token values, API request headers, or full API failures.
- A failed provider refresh must preserve a valid previous cache rather than replace it with an empty or partial cache.

## Data mapping

Existing normalized review record remains the UI boundary:

| Field | GitHub | GitLab |
|---|---|---|
| source branch | `headRefName` | `source_branch` |
| review number | PR number | MR IID |
| link | PR URL | `web_url` |
| draft | `isDraft` | `draft`/`work_in_progress` |
| CI state | check rollup | `head_pipeline.status` |
| failed checks | check contexts | empty |
| unresolved threads | review threads | empty |
| review decision | review decision | empty |

GitLab pipeline states are mapped to current UI states: `success` → success; `failed`, `canceled`, `skipped`, or `manual` → failure/attention; active states → pending; unavailable pipeline → empty.

## Configuration and documentation

Document `GITLAB_TOKEN`, the default API URL derivation, optional `WORKTREES_GITLAB_API_URL` override for nonstandard deployments, and GitLab’s deliberately limited first-pass review fields. Keep existing GitHub settings unchanged.

## Validation

- Unit-test remote parsing and provider selection for GitHub SSH/HTTPS and GitLab SSH/HTTPS remotes.
- Unit-test GitLab response normalization and pipeline-state mapping.
- Run Rust formatting, tests, and Clippy.
- Manually verify against the existing `git.uzum.io/ecom/platform/qa/qa-backend-tests` worktrees: both feature branches with an open MR show its IID and pipeline status; GitHub behavior still renders PR data.
