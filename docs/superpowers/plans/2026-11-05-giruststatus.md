# GiRustStatus Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Portable Rust + egui Windows app: track GitHub/GitLab repos, show new releases on manual refresh, download assets behind a VirusTotal hash-first gate with upload fallback.

**Architecture:** Single binary, all state next to the exe (`config.toml`, `repos.json`, `tmp/`, `downloads/`). Three layers: `core` (state, config, models — pure, testable), `net` (GitHub/GitLab/VT clients + download engine — async reqwest behind a command channel), `ui` (eframe/egui views driven by a central theme module from the persisted design system). UI communicates with `net` via a channel of typed commands; `net` replies with typed events. No background polling: refresh fires only from the UI.

**Tech Stack:** Rust, eframe/egui, reqwest (rustls-tls, no native-tls), serde/serde_json/toml, sha2, regex, globset, fuzzy-matcher, aes/pbkdf2 (export encryption), winresource (exe icon), image (window icon).

**Spec:** `docs/superpowers/specs/2026-11-05-giruststatus-design.md` (authoritative; read it first). Working protocol: `AGENTS.md` (checkpoint, per-task checklist, model mapping). Project context: `PROJECT.md`. Milestones: `ROADMAP.md`.

## Global Constraints

- Portable: no writes outside app folder except the user-chosen download folder; no registry, no installer. `eframe` persistence (storage API) must NOT be used — state lives in `repos.json`/`config.toml` next to the exe only.
- Manual refresh only. No background timers/threads polling anything.
- Safety gate: no asset reaches the download folder without a VT verdict or an explicit "unverified — proceed?" confirmation; flagged files block with Cancel as the default-focused button.
- No telemetry; network calls only to GitHub/GitLab APIs, VirusTotal APIs, and release-asset URLs.
- GitHub API base `https://api.github.com`; GitLab base `https://<host>/api/v4`; VT base `https://www.virustotal.com/api/v3`. VT key header: `x-apikey`. GitHub PAT header: `Authorization: Bearer <PAT>`. GitLab PAT header: `PRIVATE-TOKEN`.
- VT quotas: 4 req/min, 500/day — VT requests go through a throttling queue; quota state read from `x-quota_*` response headers.
- 32 MB upload limit via `POST /v3/files`; larger files via `GET /v3/files/upload_url` (650 MB cap). Poll analysis `GET /v3/analyses/{id}` every 30 s, timeout 10 min.
- Windows-first; `cargo test` green before every code commit.

## Review Focus

(Spec-implied failure modes a user will hit; each pinned to its owning task.)

1. **Repo URL variety** (`github.com/owner/repo`, `https://gitlab.com/grp/sub/repo.git`, self-hosted GitLab) → `parse_repo_url` must classify host + owner/repo or return a clear error. Test: Task 3.
2. **Prerelease-only repos** (GitHub `releases/latest` 404) → add succeeds, status "no stable release yet". Test: Task 4.
3. **Verdict-less download** (no VT key / 401 / 429 / network error / 404 with upload off / >650 MB) → warn-and-confirm "unverified", Cancel default, never a silent pass. Test: Task 7.
4. **Flagged file** (malicious+susp > 0) → blocking dialog with engine counts, Cancel default, temp file deleted on cancel. Test: Task 7.
5. **Corrupt/partial state files** (`repos.json` truncated) → load falls back to defaults + a visible error banner, never a crash. Test: Task 2.

---

### Task 1: Cargo scaffold + design system generation

**Files:**
- Create: `Cargo.toml`, `src/main.rs` (minimal `eframe::run_native` "GiRustStatus" window), `.gitignore` (already exists — verify Rust entries)
- Generate: `design-system/giruststatus/MASTER.md` via ui-ux-pro-max skill

**Interfaces:**
- Produces: runnable binary; empty egui window titled "GiRustStatus"; `MASTER.md` tokens used by Task 6's theme module.
- Note: ui-ux-pro-max's search script requires Python; if unavailable, fall back to the skill's stated built-in defaults and mark `MASTER.md` as such.

- [ ] **Step 1: Create `Cargo.toml` with pinned deps**

`eframe` (with `persistence` feature OFF), `reqwest` (features `json`, `rustls-tls`, `stream`; `default-features = false`), `serde`, `serde_json`, `toml`, `sha2`, `regex`, `globset`, `fuzzy-matcher`, `aes`, `pbkdf2` (or `pbkdf2` + `hmac` + `sha2`), `winresource` (build-dep), `image`, `tokio` (rt, rt-multi-thread), `dirs` NOT allowed (portability — paths derive from `std::env::current_exe`).

- [ ] **Step 2: Minimal `src/main.rs`**

`fn main() -> eframe::Result` calling `eframe::run_native("GiRustStatus", options, Box::new(|_| Box::new(App::default())))`; `App` empty impl of `eframe::App`.

- [ ] **Step 3: `cargo run` — window appears**

Run: `cargo run`
Expected: window titled "GiRustStatus" opens; close cleanly.

- [ ] **Step 4: Generate design system**

Run ui-ux-pro-max skill: query `"developer tool release monitoring dark modern"` `--design-system -p "GiRustStatus" --output-dir .` (plus `--density 6 --motion 4`). Read resulting `design-system/giruststatus/MASTER.md`; extract color/spacing/type tokens for Task 6.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml src/main.rs
git commit -m "feat: cargo scaffold + minimal egui window"
```

---

### Task 2: Core state — models, config, persistence

**Files:**
- Create: `src/core/mod.rs`, `src/core/models.rs`, `src/core/config.rs`, `src/core/state.rs`, `src/core/paths.rs`
- Test: `#[cfg(test)]` blocks inside each file

**Interfaces:**
- Produces:
  - `models::Repo { id: String /* "host/owner/name" */, host: RepoHost, owner: String, name: String, category: Option<String>, pinned: bool, filters: ReleaseFilters, last_seen_tag: Option<String>, latest: Option<Release>, status: RepoStatus, new_release: bool }`
  - `models::RepoHost { GitHub, GitLab { host: String /* e.g. "gitlab.com" or "gitlab.example.com" */ } }`
  - `models::ReleaseFilters { include_prereleases: bool /* default false */, tag_regex: Option<String>, asset_pattern: Option<String> }`
  - `models::Release { tag: String, published_at: String /* RFC3339 */, prerelease: bool, notes: String, assets: Vec<Asset> }`, `models::Asset { name: String, size: u64, url: String }`
  - `models::RepoStatus { Ok, NoStableRelease, NoReleases, Error(String), RateLimited }`
  - `config::Config { vt_api_key: String, vt_upload_fallback: bool /* true */, download_dir: PathBuf /* "./downloads" */, github_pat: Option<String>, gitlab_pat: Option<String>, refresh_concurrency: u32 /* 4 */, theme: ThemeChoice }` + `Config::load(path) / save(path)` (TOML)
  - `state::AppState { repos: Vec<Repo>, vt_cache: HashMap<String, VtVerdict> /* key: sha256 */, stats: Stats, load(path) / save_atomic(path) }`
  - `models::VtVerdict { sha256: String, malicious: u32, suspicious: u32, undetected: u32, harmless: u32, timeout: u32, scanned_by_upload: bool, retrieved_at: String }` with `fn is_flagged(&self) -> bool { self.malicious + self.suspicious > 0 }`
  - `paths::app_dir() -> PathBuf` derived from `current_exe().parent()`; `paths::config_file() / repos_file() / tmp_dir() / download_dir()`.

- [ ] **Step 1: Write failing tests**

`models`: default filters exclude prereleases; `is_flagged` true when malicious>0, false when all zeros.
`config`: save→load roundtrip preserves every field.
`state`: save→load roundtrip; `save_atomic` (write `repos.json.tmp`, rename) leaves no `.tmp` on success.
`paths`: all under exe dir.

- [ ] **Step 2: Run tests — fail**

Run: `cargo test`
Expected: FAIL — modules not implemented.

- [ ] **Step 3: Implement models/config/state/paths**

- [ ] **Step 4: Run tests — pass**

Run: `cargo test`
Expected: PASS.

- [ ] **Step 5: Test corrupt-file recovery**

Add test: write `"{{{"` to a temp `repos.json` path, `AppState::load` returns default state (or `Err` the UI maps to a banner — pick one, `AppState::load` returns `Result` and UI shows banner).

- [ ] **Step 6: Commit**

```bash
git add src/core
git commit -m "feat: core models, config, atomic state persistence"
```

---

### Task 3: Repo URL parsing

**Files:**
- Create: `src/core/urlparse.rs` (wire into `src/core/mod.rs`)
- Test: in-file `#[cfg(test)]`

**Interfaces:**
- Consumes: `models::RepoHost`.
- Produces: `parse_repo_url(input: &str) -> Result<(RepoHost, String, String), ParseError>` (owner, name); `ParseError { NotARepoUrl, UnsupportedHost(String) }`.
- Accepts: `github.com/owner/repo` with/without `https://`, `www.`, trailing `.git`, trailing `/`; `gitlab.com/owner/repo` and nested `gitlab.com/grp/sub/repo` (owner = full group path `"grp/sub"`, name = last segment); any `https://gitlab.*` host (self-hosted; host string from URL).

- [ ] **Step 1: Failing tests** — all accepted forms above return correct `(host, owner, name)`; `NotARepoUrl` for `""`, `"github.com"`, `"github.com/owner"`, `"https://example.com/x"`; `UnsupportedHost` for `"bitbucket.org/o/r"`.

- [ ] **Step 2: Run — FAIL.** `cargo test urlparse`

- [ ] **Step 3: Implement** — lowercase host, strip scheme/www, strip `.git`/`/`, split path segments; GitHub requires exactly 2, GitLab ≥ 2.

- [ ] **Step 4: Run — PASS.**

- [ ] **Step 5: Commit** `feat: repo URL parser (github, gitlab, self-hosted gitlab)`

---

### Task 4: GitHub + GitLab release clients

**Files:**
- Create: `src/net/mod.rs`, `src/net/gh.rs`, `src/net/gl.rs`, `src/net/ratelimits.rs`
- Test: in-file unit tests for pure helpers; live-API behavior verified in Task 10 smoke tests.

**Interfaces:**
- Consumes: `models::{RepoHost, Release, Asset, ReleaseFilters, RepoStatus}`.
- Produces:
  - `gh::fetch_latest(client, owner, name, pat: Option<&str>) -> Result<Release, FetchError>` (uses `releases/latest`)
  - `gh::fetch_releases_list(client, owner, name, pat, filters) -> Result<Option<Release>, FetchError>` — applies prerelease include + tag regex, picks newest by `published_at`; `Ok(None)` when nothing matches.
  - `gl::resolve_project_id(client, host, owner_slash_name, pat) -> Result<u64, FetchError>` (`GET /projects/{url-encoded path}`; use `?search_namespaces=true` if needed — implement with URL-encoded full path).
  - `gl::fetch_latest(client, host, project_id, pat) -> Result<Option<Release>, FetchError>` — GitLab `/releases` returns newest first; map links of type `release:assets` where available, else parse `description` links — use the releases endpoint's `assets.links` (title, url, direct_asset_url) plus `assets.sources`. If both empty, `Vec::new()`.
  - `ratelimits::RateLimits::from_headers(headers) -> RateLimits { github_remaining: Option<u32>, github_reset_epoch: Option<u64>, vt_minute_used: Option<u32>, ... }` — parse `x-ratelimit-*` (GitHub), `RateLimit-*` (GitLab), `x-quota-*` (VT).
  - `FetchError { NotFound /* 404 → RepoStatus::NoReleases or NoStableRelease per caller */, Auth, RateLimited, Network(String), Parse(String) }`
- Logic pinned by spec: caller of `gh::fetch_latest` on `NotFound` retries via `fetch_releases_list` with `include_prereleases: true` to decide `NoStableRelease` (prereleases exist) vs `NoReleases` (list empty). Cache resolved GitLab project IDs in `Repo` (add `gl_project_id: Option<u64>` to `models::Repo` in this task).

- [ ] **Step 1: Failing tests** — pure parts: tag-regex filter picks/masks correctly; prerelease include/exclude picks correct release from a fixed list; `RateLimits` parses the three header dialects from fake `http::HeaderMap`s.
- [ ] **Step 2: Run — FAIL.**
- [ ] **Step 3: Implement** reqwest calls + serde structs (GitHub: `tag_name`, `published_at`, `prerelease`, `body`, `assets[].{name,size,browser_download_url}`).
- [ ] **Step 4: Run — PASS.**
- [ ] **Step 5: Commit** `feat: github/gitlab release clients + rate-limit parsing`

---

### Task 5: VirusTotal client + download engine + command bus

**Files:**
- Create: `src/net/vt.rs`, `src/net/download.rs`, `src/net/bus.rs`
- Test: in-file unit tests for pure logic (verdict mapping, throttle timing, quota parsing).

**Interfaces:**
- Consumes: `models::VtVerdict`, Task 4's `client` construction (shared `reqwest::Client` built once in `bus`).
- Produces:
  - `vt::lookup_sha256(client, key, sha256) -> LookupOutcome { Report(VtVerdict), Unknown /* 404 */, Unscanned /* report exists, last_analysis_stats null */, NoVerdict(VtError) }`
  - `vt::upload_file(client, key, path, size) -> Result<AnalysisId, VtError>` — if size ≤ 32 MB use `POST /files`, else `GET /files/upload_url` then POST there; `VtError { Auth, Quota, TooLarge, Network(String), Parse(String) }`
  - `vt::poll_analysis(client, key, id) -> Result<VtVerdict, VtError>` — 30 s interval, 10 min timeout; single poll-step function `poll_once` is what tests exercise.
  - `vt::map_stats(attrs_json) -> VtVerdict` — pure; tests feed sample VT JSON payloads.
  - `download::download_to_temp(client, url, tmp_dir, expected_stem, progress_tx: mpsc::Sender<Progress>, cancel: CancellationToken-like bool flag) -> Result<(PathBuf, String /*sha256*/), DlError>` — streams to `tmp_dir/<stem>.part`, computes sha256 while streaming, renames on completion.
  - `bus::Command` enum: `RefreshAll { repos: Vec<Repo>, config: Config }`, `AddRepo { url: String, config: Config }`, `DownloadAsset { repo_id: String, asset: Asset, config: Config }`, plus variants for Task 7's confirm outcomes.
  - `bus::Event` enum: `RepoUpdated(Repo)`, `AddResult(Result<Repo, String>)`, `DownloadProgress { repo_id, asset_name, downloaded, total }`, `VtVerdictReady { repo_id, asset_name, verdict: LookupOutcome }`, `LogLine(String)`, `RateLimitsUpdated(RateLimits)`.
  - Runtime: `bus::spawn(client, rx) -> std::thread::JoinHandle` — a worker thread owning a `tokio::runtime`, executing commands, sending `Event`s over an `mpsc` to the UI. VT requests pass a throttle gate: max 1 request per 16 s (well under 4/min).

- [ ] **Step 1: Failing tests** — `map_stats` on 3 sample payloads (clean, flagged, null stats → `Unscanned`); throttle gate admits first request immediately, blocks until 16 s elapsed (inject clock); asset-name → temp filename sanitization (illegal Windows chars replaced).
- [ ] **Step 2: Run — FAIL.**
- [ ] **Step 3: Implement** all files; `poll_once` is a plain request so tests hit it with a mock server optional — keep network-touching paths thin and untested (covered by Task 10 smoke).
- [ ] **Step 4: Run — PASS.**
- [ ] **Step 5: Commit** `feat: virustotal client, download engine, command bus`

---

### Task 6: egui shell — theme, main list, repo detail

**Files:**
- Create: `src/ui/mod.rs`, `src/ui/theme.rs`, `src/ui/list.rs`, `src/ui/detail.rs`, `src/ui/widgets.rs` (pills, badges, ribbons)
- Modify: `src/main.rs` (real `App` wiring: state, config, bus channels, 60 fps repaint only when active)
- Test: manual run-through; widget logic (search filter set, sort comparator) as unit tests in `list.rs`.

**Interfaces:**
- Consumes: Task 2 models/state, Task 5 bus, Task 1 design tokens (`MASTER.md`).
- Produces:
  - `theme::apply(ctx, choice)` — sets egui `Visuals` from MASTER.md tokens (dark-first, accent palette, spacing scale); `ThemeChoice { Dark, Light, System }`.
  - `App` top bar: app name, Refresh button (disabled while refresh in flight), search box, sort dropdown (`Name`, `Newest release`, `Last checked`), Settings gear.
  - `list.rs`: rows = host glyph (GH/GL), `owner/name`, category ribbon color strip, release tag + date, NEW indicator, status pill from `RepoStatus`, pin icon. Pinned sort first. Fuzzy search via `fuzzy-matcher` over name+owner+category.
  - NEW indicator (three layers, no info in color alone): (1) leading 2px strip + elevated row background in theme accent-green while `new_release`; (2) version pill `↓ <tag>` — filled green when NEW, muted gray once seen; (3) header chip `N new` while any NEW exists, click filters list to only-new repos. Colorblind-safe: pill text also changes (`NEW` prefix vs version only).
  - `detail.rs`: release notes as styled plain text (bold headings heuristic), asset table (name, size, VT badge or "—", Download button), open-in-browser link, per-repo settings: category, pin, prerelease toggle, tag regex field (validated on change, red on invalid), asset pattern field (globset, validated).
- [ ] **Step 1: Unit tests** — sort comparator (pinned first, then chosen key); search filter matches name/owner/category, non-match excluded.
- [ ] **Step 2: Run — FAIL then implement — PASS.**
- [ ] **Step 3: Wire bus** — Refresh button sends `RefreshAll`; incoming `RepoUpdated` merges into `AppState` and saves; `LogLine` appended to in-memory ring (Task 9 renders it; store now in `App.log_ring: VecDeque<String>` capped 500).
- [ ] **Step 4: `cargo run`** — add a repo via Add dialog (uses Task 3 parser + Task 4 add-verify via `AddRepo` command), see it listed, press Refresh, see tags. Expected: works end-to-end with real network.
- [ ] **Step 5: Commit** `feat: egui shell — themed list, detail view, refresh wiring`

---

### Task 7: Download + VT gate UX

**Files:**
- Create: `src/ui/gate.rs`
- Modify: `src/ui/detail.rs` (Download button flow), `src/net/bus.rs` (confirm/cancel outcomes)

**Interfaces:**
- Consumes: Task 5 `DownloadAsset`/`VtVerdictReady`, Task 2 `VtVerdict::is_flagged`.
- Produces: modal state machine in `App`: `Idle → Downloading(progress) → Scanning → AwaitingConfirm(ConfirmKind) → Done/Cancelled`, where `ConfirmKind { Flagged(VtVerdict, top_engines: Vec<String>), Unverified(String /* reason */) }`. Exactly one download at a time.
- Pinned behaviors (spec-verbatim): flagged dialog lists counts ("N/M engines flagged"), top flagging engines, link to VT report (`https://www.virustotal.com/gui/file/{sha256}`), buttons "Download anyway" / "Cancel", Cancel default-focused. Unverified dialog text: "Not scanned — proceed?" with reason (no key / quota / network / upload disabled / too large / timeout). Cancel deletes the `.part`/temp file. Confirm moves temp file to `config.download_dir`. Verdicts cached into `AppState.vt_cache` keyed by sha256 (skip re-lookup on cache hit for same sha256).

- [ ] **Step 1: Unit tests** — state machine transitions: `Flagged → Cancel` deletes temp file; `Flagged → Confirm` moves file; `Unverified → Confirm` moves file; cache hit short-circuits (no VT command sent — assertable at bus level with a fake).
- [ ] **Step 2: Run — FAIL then implement — PASS.**
- [ ] **Step 3: Manual verify** with a known-clean small asset and (optional) a flagged EICAR-style test hash — do NOT download real malware; simulate verdict via unit-level injection.
- [ ] **Step 4: Commit** `feat: virustotal download gate with flagged/unverified confirm dialogs`

---

### Task 8: Search, sort, pins, filters polish

**Files:**
- Modify: `src/ui/list.rs`, `src/ui/detail.rs`
- Test: unit tests in `list.rs`.

- [ ] **Step 1: Tests + implement** — pin toggle from list context menu; category assignment dialog with fixed palette (category → color map: 8 slots); NEW pill clears when row opened (auto-clear on open — builder's call from spec); asset pattern marks matching assets ("match" mark, sorted first).
- [ ] **Step 2: `cargo test` + manual run — PASS.**
- [ ] **Step 3: Commit** `feat: pins, categories, NEW-pill lifecycle, asset match marks`

---

### Task 9: Diagnostics — logs, rate panel, statistics

**Files:**
- Create: `src/ui/logs.rs`, `src/ui/rates.rs`, `src/ui/stats.rs`
- Modify: `src/main.rs` (nav: bottom or side rail with Repos / Logs / Rates / Stats / Settings)

**Interfaces:**
- Consumes: `App.log_ring`, `bus::Event::RateLimitsUpdated`, `AppState.stats`.
- Produces: Logs view (copy button, export-to-file button, in-memory only); Rates view (per-API rows: remaining/reset or quota used, countdown to reset in local time); Stats view (repos, NEW count, downloads, scans, flagged — cumulative counters saved in state).

- [ ] **Step 1: Implement views** — pure rendering from state; counters incremented at bus sites (download done, scan done, flagged seen).
- [ ] **Step 2: Manual verify + commit** `feat: diagnostics — log viewer, rate panel, statistics`

---

### Task 10: Import/export + settings view

**Files:**
- Create: `src/core/io.rs` (export/import incl. AES), `src/ui/settings.rs`
- Test: `#[cfg(test)]` in `io.rs`; smoke test `tests/smoke.rs` (live GitHub API: fetch latest release of `gitlab/gitlab` unreachable — use a stable small repo, e.g. `serde-rs/serde`; asserts tag non-empty. Skippable offline via env var `GIRUST_SMOKE=0`).

**Interfaces:**
- Consumes: Task 2 `AppState`/`Config`.
- Produces:
  - `io::export(state, config, password: Option<&str>) -> Vec<u8>` — single JSON `{state, config}`; when password: AES-256-CBC, PBKDF2-SHA256, 600k iters, random salt+IV, format `GIRS1|<salt>|<iv>|<ciphertext>` base64 fields.
  - `io::import(bytes, password: Option<&str>) -> Result<(AppState, Config), ImportError>` — detects `GIRS1` magic vs plain JSON; wrong password → `ImportError::BadPassword`.
  - `settings.rs`: VT key field, upload toggle, download dir picker (text field + Browse via `rfd` — add dep, feature portable-safe), PATs, concurrency slider, theme choice, quota display, import/export buttons with confirmation dialog on import (replaces state).

- [ ] **Step 1: Failing tests** — export/import roundtrip plain; roundtrip encrypted with password; wrong password → `BadPassword`; truncated file → `ImportError::Corrupt` not panic.
- [ ] **Step 2: Run — FAIL → implement → PASS.**
- [ ] **Step 3: Smoke test** — `cargo test --test smoke` (network) or `GIRUST_SMOKE=0 cargo test`.
- [ ] **Step 4: Commit** `feat: settings view, encrypted import/export, smoke test`

---

### Task 11: Icon + packaging

**Files:**
- Create: `build.rs` (winresource), `assets/icon.png` (+ `.ico`)
- Modify: `Cargo.toml`

- [ ] **Step 1: Icon art** — flat rounded-square, dark slate background, radar-sweep arc with shield cutout, green accent; 1024px PNG → `.ico` (multi-size 16–256). `winresource` embeds on `cargo build`; window icon via eframe `NativeOptions::icon`.
- [ ] **Step 2: Verify** — `cargo build --release`; check exe icon in Explorer; window icon at runtime; no extra files required next to exe beyond `config.toml`/`repos.json` created at first run.
- [ ] **Step 3: Commit** `feat: app icon (exe resource + window), release build`
- [ ] **Step 4: Update ROADMAP/PROJECT, commit docs** `docs: roadmap/project status`

---

### Task 12: Final review + verification pass

- [ ] **Step 1:** `cargo test` full suite green; `cargo clippy -- -D warnings` clean.
- [ ] **Step 2:** Manual walkthrough of every spec goal against the running app (checklist from `PROJECT.md`).
- [ ] **Step 3:** Fresh-context reviewer subagent (GLM-5.3, high) audits the diff branch vs spec; fix findings; re-verify.
- [ ] **Step 4:** Commit fixes; ROADMAP → all done; PROJECT.md final.
