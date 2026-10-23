# Review Worker Consolidation Plan

**Goal:** Put review-cache refresh lifecycle behind one Rust module interface without changing GitHub, GitLab, cache, or panel UI behavior.
**Files touched:** `board/src/data.rs`, `board/src/ui.rs`, `CONTRIBUTING.md`, `CHANGELOG.md`

---

## Tasks

### 1. Consolidate review refresh lifecycle
- [ ] `board/src/data.rs`: add one internal review-worker loop that owns TTL checks, sidecar locking, cache replacement, cached `Msg::Pr` publishing, and refresh kicks.
- [ ] `board/src/data.rs`: reduce GitHub GraphQL and GitLab REST workers to provider-specific fetch implementations passed to that loop; preserve cache names, normalized `PrInfo` JSON, request behavior, and failure retention.
- [ ] `board/src/data.rs`: add unit coverage that a failed refresh retains an existing cache and removes its lock; retain provider parsing and GitLab normalization coverage.

### 2. Verify panel UI stays stable
- [ ] `board/src/ui.rs`: make no rendering or interaction change unless the review-worker refactor exposes a real defect.
- [ ] Run existing UI rendering tests, including selected-card color, agent labels, branch wrapping, and managed-path display.

### 3. Document and validate
- [ ] `CONTRIBUTING.md`: describe `data.rs` as owning GitHub and GitLab review collectors.
- [ ] `CHANGELOG.md`: record the internal consolidation and preserved review-cache behavior.
- [ ] Run `cargo fmt --check`, `cargo test`, `cargo clippy --all-targets`, and shell syntax checks.
- [ ] Commit: `refactor: consolidate review workers`
