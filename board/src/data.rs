//! Background collectors. Each worker owns one data source and refresh cadence, so a slow
//! review request never delays agent-state updates.

use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Default)]
pub struct Agent {
    pub status: String,
    /// Which coding agent: herdr's `display_agent` when a pane reports one, else its canonical id
    /// (`claude`, `codex`, `pi`, `opencode`, ...).
    pub name: String,
    pub title: String,
}

#[derive(Clone, Debug)]
pub struct WtRaw {
    pub path: String,
    pub branch: String,
    pub prunable: bool,
    pub linked: bool,
    pub open_ws: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct HerdrSnap {
    pub worktrees: Vec<WtRaw>,
    /// (cwd, agent) for every pane running a detected agent.
    pub agents: Vec<(String, Agent)>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct GitInfo {
    pub dirty: u32,
    pub ahead: u32,
    pub behind: u32,
    pub last_ct: i64,
    pub subject: String,
    pub files: u32,
    pub ins: u32,
    pub del: u32,
}

/// Cache rows are a superset of the bash board's `{b,n,ci,t}` so both boards can share one file.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct PrInfo {
    pub b: String,
    pub n: u64,
    #[serde(default)]
    pub ci: String,
    #[serde(default)]
    pub t: u32,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub draft: bool,
    #[serde(default)]
    pub review: String,
    #[serde(default)]
    pub fails: Vec<String>,
    #[serde(default)]
    pub pending: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Forge {
    Github { slug: String },
    Gitlab { host: String, project: String },
}

pub enum Msg {
    Herdr(HerdrSnap),
    Git(HashMap<String, GitInfo>),
    Pr(Vec<PrInfo>, i64),
    Status(String, bool),
}

/// Seconds from a `WORKTREES_*` setting (config.env), falling back to `default`.
pub fn env_secs(name: &str, default: u64) -> u64 {
    std::env::var(name).ok().and_then(|v| v.parse().ok()).filter(|&n| n > 0).unwrap_or(default)
}

pub fn herdr_bin() -> String {
    std::env::var("HERDR_BIN_PATH").unwrap_or_else(|_| "herdr".into())
}

pub fn run(cmd: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(cmd).args(args).stdin(Stdio::null()).stderr(Stdio::null()).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

fn run_with_stdin(cmd: &str, args: &[&str], input: &str) -> Option<String> {
    let mut child =
        Command::new(cmd).args(args).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().ok()?;
    child.stdin.as_mut()?.write_all(input.as_bytes()).ok()?;
    let out = child.wait_with_output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

pub fn herdr_json(args: &[&str]) -> Option<Value> {
    serde_json::from_str(&run(&herdr_bin(), args)?).ok()
}

pub fn now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

fn s(v: &Value, k: &str) -> String {
    v.get(k).and_then(Value::as_str).unwrap_or_default().to_string()
}

pub fn herdr_snapshot(ws: &str) -> HerdrSnap {
    let Some(wt) = herdr_json(&["worktree", "list", "--workspace", ws]) else {
        return HerdrSnap { error: Some("not a git workspace".into()), ..Default::default() };
    };
    let worktrees = wt["result"]["worktrees"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|w| WtRaw {
            path: s(w, "path"),
            branch: if w["is_detached"].as_bool().unwrap_or(false) { "(detached)".into() } else { s(w, "branch") },
            prunable: w["is_prunable"].as_bool().unwrap_or(false),
            linked: w["is_linked_worktree"].as_bool().unwrap_or(false),
            open_ws: w["open_workspace_id"].as_str().map(String::from),
        })
        .collect();
    let agents = herdr_json(&["pane", "list"])
        .and_then(|p| p["result"]["panes"].as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter(|p| p["agent"].is_string())
        .map(|p| {
            let cwd = p["foreground_cwd"].as_str().or(p["cwd"].as_str()).unwrap_or_default().to_string();
            let agent = Agent {
                status: p["agent_status"].as_str().unwrap_or("unknown").into(),
                name: p["display_agent"]
                    .as_str()
                    .filter(|d| !d.is_empty())
                    .map(String::from)
                    .unwrap_or_else(|| s(p, "agent")),
                title: s(p, "terminal_title_stripped"),
            };
            (cwd, agent)
        })
        .collect();
    HerdrSnap { worktrees, agents, error: None }
}

pub fn spawn_herdr(ws: String, tx: Sender<Msg>, kick: Receiver<()>) {
    std::thread::spawn(move || loop {
        if tx.send(Msg::Herdr(herdr_snapshot(&ws))).is_err() {
            return;
        }
        let _ = kick.recv_timeout(Duration::from_millis(1500));
    });
}

fn git(path: &str, args: &[&str]) -> Option<String> {
    let mut full = vec!["-C", path];
    full.extend_from_slice(args);
    run("git", &full)
}

pub fn git_info(path: &str, base: &str) -> GitInfo {
    let mut g = GitInfo {
        dirty: git(path, &["status", "--porcelain"]).map(|o| o.lines().count() as u32).unwrap_or(0),
        ..Default::default()
    };
    if let Some(o) = git(path, &["rev-list", "--left-right", "--count", &format!("{base}...HEAD")]) {
        let mut it = o.split_whitespace().map(|n| n.parse().unwrap_or(0));
        g.behind = it.next().unwrap_or(0);
        g.ahead = it.next().unwrap_or(0);
    }
    if let Some(o) = git(path, &["log", "-1", "--format=%ct%x1f%s"]) {
        let (ct, subj) = o.trim_end().split_once('\x1f').unwrap_or(("0", ""));
        g.last_ct = ct.parse().unwrap_or(0);
        g.subject = subj.to_string();
    }
    if g.ahead > 0 {
        if let Some(o) = git(path, &["diff", "--shortstat", &format!("{base}...HEAD")]) {
            for part in o.split(',') {
                let n: u32 = part.split_whitespace().next().and_then(|n| n.parse().ok()).unwrap_or(0);
                if part.contains("file") {
                    g.files = n;
                } else if part.contains("insertion") {
                    g.ins = n;
                } else if part.contains("deletion") {
                    g.del = n;
                }
            }
        }
    }
    g
}

pub fn spawn_git(paths: Arc<Mutex<Vec<String>>>, base: String, tx: Sender<Msg>, kick: Receiver<()>) {
    let every = Duration::from_secs(env_secs("WORKTREES_GIT_EVERY", 10));
    std::thread::spawn(move || loop {
        let list = paths.lock().map(|p| p.clone()).unwrap_or_default();
        let infos =
            list.iter().filter(|p| std::path::Path::new(p).is_dir()).map(|p| (p.clone(), git_info(p, &base))).collect();
        if tx.send(Msg::Git(infos)).is_err() {
            return;
        }
        let _ = kick.recv_timeout(every);
    });
}

const PR_QUERY: &str = r#"query($owner:String!,$name:String!){repository(owner:$owner,name:$name){
  pullRequests(states:OPEN,first:100,orderBy:{field:UPDATED_AT,direction:DESC}){nodes{
    number headRefName url isDraft reviewDecision
    reviewThreads(first:100){nodes{isResolved}}
    commits(last:1){nodes{commit{statusCheckRollup{state contexts(first:100){nodes{
      __typename ... on CheckRun{name status conclusion} ... on StatusContext{context state}}}}}}}}}}}"#;

const PR_JQ: &str = r#"[.data.repository.pullRequests.nodes[]
  | (.commits.nodes[0].commit.statusCheckRollup // {}) as $r
  | ($r.contexts.nodes // []) as $c
  | {b: .headRefName, n: .number, url: .url, draft: .isDraft, review: (.reviewDecision // ""),
     ci: ($r.state // ""),
     t: ([.reviewThreads.nodes[] | select(.isResolved | not)] | length),
     fails: [$c[] | select((.conclusion // .state // "") as $s
               | ["FAILURE","TIMED_OUT","CANCELLED","ACTION_REQUIRED","STARTUP_FAILURE","ERROR"] | index($s))
             | (.name // .context)],
     pending: ([$c[] | select(.__typename == "CheckRun" and .status != "COMPLETED")] | length)}]"#;

/// Parse a Git SSH or HTTP(S) remote. GitHub uses its existing collector; every other
/// parseable host is treated as GitLab so self-hosted instances need no host allowlist.
pub fn forge(remote: &str) -> Option<Forge> {
    let remote = remote.trim().trim_end_matches('/');
    let (host, path) = if let Some(rest) = remote.strip_prefix("git@") {
        rest.split_once(':')?
    } else {
        let rest = remote.split_once("://")?.1;
        rest.split_once('/')?
    };
    let host = host.rsplit('@').next()?.to_string();
    let project = path.trim_start_matches('/').trim_end_matches(".git");
    (project.split('/').count() >= 2).then_some(())?;
    if host == "github.com" && project.split('/').count() == 2 {
        Some(Forge::Github { slug: project.to_string() })
    } else {
        Some(Forge::Gitlab { host, project: project.to_string() })
    }
}

/// Choose the canonical review remote. `origin` wins whenever its URL identifies a supported
/// forge; otherwise retain Git's remote order and use the first parseable URL.
pub fn select_remote(remotes: &[(String, String)]) -> Option<(String, Forge)> {
    remotes
        .iter()
        .find(|(name, url)| name == "origin" && forge(url).is_some())
        .or_else(|| remotes.iter().find(|(_, url)| forge(url).is_some()))
        .and_then(|(name, url)| forge(url).map(|forge| (name.clone(), forge)))
}

/// Read repository remotes once, then apply the canonical remote-selection rule shared by board
/// startup and the statusbar command.
pub fn remote_forge(root: &str) -> Option<(String, Forge)> {
    let remotes = run("git", &["-C", root, "remote"])
        .unwrap_or_default()
        .lines()
        .filter_map(|name| run("git", &["-C", root, "remote", "get-url", name]).map(|url| (name.to_string(), url)))
        .collect::<Vec<_>>();
    select_remote(&remotes)
}

pub fn cache_name(forge: &Forge) -> String {
    let id = match forge {
        Forge::Github { slug } => format!("github-{slug}"),
        Forge::Gitlab { host, project } => format!("gitlab-{host}-{project}"),
    };
    format!(
        "reviews-{}.json",
        id.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' }).collect::<String>()
    )
}

fn url_encode(value: &str) -> String {
    value
        .bytes()
        .flat_map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => vec![b as char],
            _ => format!("%{b:02X}").chars().collect(),
        })
        .collect()
}

#[derive(Deserialize)]
struct GitlabPipeline {
    #[serde(default)]
    status: String,
}

#[derive(Deserialize)]
struct GitlabMr {
    source_branch: String,
    iid: u64,
    web_url: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    work_in_progress: bool,
    #[serde(default)]
    head_pipeline: Option<GitlabPipeline>,
}

fn gitlab_ci(status: &str) -> String {
    match status {
        "success" => "SUCCESS",
        "failed" | "canceled" | "cancelled" | "skipped" | "manual" => "FAILURE",
        "created" | "waiting_for_resource" | "preparing" | "pending" | "running" | "scheduled" => "PENDING",
        _ => "",
    }
    .into()
}

fn normalize_gitlab_mrs(mrs: Vec<GitlabMr>) -> Vec<PrInfo> {
    mrs.into_iter()
        .map(|mr| PrInfo {
            b: mr.source_branch,
            n: mr.iid,
            url: mr.web_url,
            draft: mr.draft || mr.work_in_progress,
            ci: mr.head_pipeline.map(|p| gitlab_ci(&p.status)).unwrap_or_default(),
            ..Default::default()
        })
        .collect()
}

fn parse_gitlab_mrs(json: &str) -> Option<Vec<GitlabMr>> {
    serde_json::from_str(json).ok()
}

fn gitlab_request(endpoint: &str, token: &str) -> Option<String> {
    let header = token.replace(['\n', '\r'], "").replace('\\', "\\\\").replace('"', "\\\"");
    let config = format!("header = \"PRIVATE-TOKEN: {header}\"\n");
    run_with_stdin("curl", &["--fail", "--silent", "--show-error", "--config", "-", "--url", endpoint], &config)
}

fn worktree_branches(root: &str) -> HashSet<String> {
    run("git", &["-C", root, "worktree", "list", "--porcelain"])
        .unwrap_or_default()
        .lines()
        .filter_map(|line| line.strip_prefix("branch refs/heads/"))
        .map(String::from)
        .collect()
}

mod review_worker {
    use super::*;

    pub(super) trait Provider: Send + 'static {
        /// Returns normalized `PrInfo` JSON. A failure leaves the current cache untouched.
        fn fetch(&self) -> Option<String>;
    }

    fn mtime(path: &Path) -> Option<i64> {
        std::fs::metadata(path)
            .and_then(|metadata| metadata.modified())
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|time| time.as_secs() as i64)
    }

    fn stale(cache: &Path, ttl: i64) -> bool {
        mtime(cache).map(|at| now() - at >= ttl).unwrap_or(true)
    }

    /// A short-lived sidecar lock makes panels share one refresh without touching a valid cache.
    fn claim_refresh(cache: &Path) -> Option<PathBuf> {
        let lock = cache.with_extension("lock");
        if mtime(&lock).is_some_and(|at| now() - at < 30) {
            return None;
        }
        let _ = std::fs::remove_file(&lock);
        std::fs::OpenOptions::new().write(true).create_new(true).open(&lock).ok()?;
        Some(lock)
    }

    fn replace_cache(cache: &Path, body: &str) {
        let tmp = cache.with_extension(format!("tmp.{}", std::process::id()));
        if std::fs::write(&tmp, body).is_ok() {
            let _ = std::fs::rename(tmp, cache);
        }
    }

    fn cached_prs(cache: &Path) -> Vec<PrInfo> {
        std::fs::read_to_string(cache).ok().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default()
    }

    pub(super) fn refresh_if_stale(provider: &impl Provider, cache: &Path, ttl: i64) {
        if !stale(cache, ttl) {
            return;
        }
        let Some(lock) = claim_refresh(cache) else {
            return;
        };
        if let Some(body) = provider.fetch() {
            replace_cache(cache, &body);
        }
        let _ = std::fs::remove_file(lock);
    }

    pub(super) fn spawn<P: Provider>(provider: P, cache: PathBuf, tx: Sender<Msg>, kick: Receiver<()>) {
        let ttl = env_secs("WORKTREES_PR_TTL", 60) as i64;
        std::thread::spawn(move || loop {
            refresh_if_stale(&provider, &cache, ttl);
            if tx.send(Msg::Pr(cached_prs(&cache), mtime(&cache).unwrap_or(0))).is_err() {
                return;
            }
            let _ = kick.recv_timeout(Duration::from_secs(5));
        });
    }
}

struct GithubProvider {
    slug: String,
    gh: String,
}

impl review_worker::Provider for GithubProvider {
    fn fetch(&self) -> Option<String> {
        let (owner, name) = self.slug.split_once('/').unwrap_or_default();
        let args = [
            "api",
            "graphql",
            "-F",
            &format!("owner={owner}"),
            "-F",
            &format!("name={name}"),
            "-f",
            &format!("query={PR_QUERY}"),
            "--jq",
            PR_JQ,
        ];
        run(&self.gh, &args)
    }
}

struct GitlabProvider {
    api_base: String,
    project: String,
    token: String,
    root: String,
}

impl review_worker::Provider for GitlabProvider {
    fn fetch(&self) -> Option<String> {
        let project_url = url_encode(&self.project);
        let endpoint = format!(
            "{}/projects/{project_url}/merge_requests?state=opened&scope=all&per_page=100",
            self.api_base.trim_end_matches('/')
        );
        let mut mrs = parse_gitlab_mrs(&gitlab_request(&endpoint, &self.token)?)?;
        let branches = worktree_branches(&self.root);
        for mr in &mut mrs {
            if branches.contains(&mr.source_branch) {
                let detail =
                    format!("{}/projects/{project_url}/merge_requests/{}", self.api_base.trim_end_matches('/'), mr.iid);
                if let Some(json) = gitlab_request(&detail, &self.token) {
                    if let Ok(detail) = serde_json::from_str::<GitlabMr>(&json) {
                        mr.head_pipeline = detail.head_pipeline;
                    }
                }
            }
        }
        serde_json::to_string(&normalize_gitlab_mrs(mrs)).ok()
    }
}

pub fn spawn_github_pr(slug: String, cache: PathBuf, gh: String, tx: Sender<Msg>, kick: Receiver<()>) {
    review_worker::spawn(GithubProvider { slug, gh }, cache, tx, kick);
}

pub fn spawn_gitlab_mr(
    api_base: String,
    project: String,
    token: String,
    root: String,
    cache: PathBuf,
    tx: Sender<Msg>,
    kick: Receiver<()>,
) {
    review_worker::spawn(GitlabProvider { api_base, project, token, root }, cache, tx, kick);
}

#[cfg(test)]
mod tests {
    use super::{
        cache_name, forge, gitlab_ci, normalize_gitlab_mrs, parse_gitlab_mrs, review_worker, select_remote, Forge,
    };

    struct FailedProvider;

    impl review_worker::Provider for FailedProvider {
        fn fetch(&self) -> Option<String> {
            None
        }
    }

    #[test]
    fn identifies_github_and_self_hosted_gitlab_remotes() {
        assert_eq!(forge("git@github.com:owner/repo.git"), Some(Forge::Github { slug: "owner/repo".into() }));
        assert_eq!(forge("https://github.com/owner/repo"), Some(Forge::Github { slug: "owner/repo".into() }));
        assert_eq!(
            forge("git@git.uzum.io:ecom/platform/qa/qa-backend-tests.git"),
            Some(Forge::Gitlab { host: "git.uzum.io".into(), project: "ecom/platform/qa/qa-backend-tests".into() })
        );
        assert_eq!(
            forge("https://gitlab.example.com/group/nested/repo.git"),
            Some(Forge::Gitlab { host: "gitlab.example.com".into(), project: "group/nested/repo".into() })
        );
        assert_eq!(
            forge("ssh://git@gitlab.example.com/group/nested/repo.git"),
            Some(Forge::Gitlab { host: "gitlab.example.com".into(), project: "group/nested/repo".into() })
        );
    }

    #[test]
    fn selects_parseable_origin_before_earlier_remotes() {
        let remotes = vec![
            ("github".into(), "git@github.com:mirror/repo.git".into()),
            ("origin".into(), "git@gitlab.example.com:group/repo.git".into()),
        ];
        assert_eq!(
            select_remote(&remotes),
            Some(("origin".into(), Forge::Gitlab { host: "gitlab.example.com".into(), project: "group/repo".into() }))
        );
    }

    #[test]
    fn selects_github_when_it_is_the_only_parseable_remote() {
        let remotes = vec![("origin".into(), "https://github.com/owner/repo.git".into())];
        assert_eq!(select_remote(&remotes), Some(("origin".into(), Forge::Github { slug: "owner/repo".into() })));
    }

    #[test]
    fn selects_first_parseable_remote_without_origin() {
        let remotes = vec![
            ("broken".into(), "not a remote URL".into()),
            ("upstream".into(), "https://gitlab.example.com/group/repo.git".into()),
        ];
        assert_eq!(
            select_remote(&remotes),
            Some((
                "upstream".into(),
                Forge::Gitlab { host: "gitlab.example.com".into(), project: "group/repo".into() }
            ))
        );
    }

    #[test]
    fn falls_back_when_origin_is_unparseable() {
        let remotes = vec![
            ("github".into(), "https://github.com/owner/repo.git".into()),
            ("origin".into(), "not a remote URL".into()),
        ];
        assert_eq!(select_remote(&remotes), Some(("github".into(), Forge::Github { slug: "owner/repo".into() })));
    }

    #[test]
    fn failed_refresh_keeps_cache_and_removes_lock() {
        let dir = std::env::temp_dir().join(format!("wt-board-review-test-{}-{}", std::process::id(), super::now()));
        std::fs::create_dir_all(&dir).unwrap();
        let cache = dir.join("reviews.json");
        let original = r#"[{"b":"old","n":1}]"#;
        std::fs::write(&cache, original).unwrap();

        review_worker::refresh_if_stale(&FailedProvider, &cache, 0);

        assert_eq!(std::fs::read_to_string(&cache).unwrap(), original);
        assert!(!cache.with_extension("lock").exists());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn normalizes_gitlab_merge_requests() {
        let prs = normalize_gitlab_mrs(parse_gitlab_mrs(r#"[{"source_branch":"feat/x","iid":42,"web_url":"https://gitlab/mr/42","draft":true,"head_pipeline":{"status":"running"}}]"#).unwrap());
        assert_eq!(prs[0].b, "feat/x");
        assert_eq!(prs[0].n, 42);
        assert_eq!(prs[0].ci, "PENDING");
        assert!(prs[0].draft);
        assert_eq!(gitlab_ci("success"), "SUCCESS");
        assert_eq!(gitlab_ci("failed"), "FAILURE");
        assert_eq!(gitlab_ci("unknown"), "");
        assert_eq!(cache_name(&forge("git@github.com:owner/repo.git").unwrap()), "reviews-github-owner_repo.json");
    }
}
