# GiRustStatus — Design Document

Date: 2026-11-05
Status: Approved in conversation (playback round) — pending final handoff

## Intent

A small, portable Windows desktop app for a single user who follows open-source
projects on GitHub and GitLab. It shows when those projects publish a new
release, lists the release's downloadable assets, and gates every download
behind a VirusTotal verdict. The user installs nothing: releases are
downloaded, not installed — installation is done manually by the user.

Inspiration: Obtainium+ (ObtainiumPlus-main, a Flutter fork of Obtainium). We
borrow its core idea — "app updates straight from the source" — and its
VirusTotal safety-scan concept, but this is a from-scratch Rust app with a much
narrower scope: release monitoring + asset download + VT scan. No app
installation, no Android integration, no Shizuku, no store scraping.

## Goals

1. Track a hand-curated list of GitHub and GitLab repos (added by pasting URLs,
   never synced from any account).
2. On demand (Refresh button only), fetch the latest release of every tracked
   repo and show new releases since the last check.
3. Show each release's asset list; user picks one asset and downloads it.
4. Before any asset file reaches the user's download folder, run a VirusTotal
   check: hash lookup first, upload fallback second (toggleable). If VT flags
   the file, block with a warning dialog that requires explicit confirmation
   before the file is delivered.
5. Fully portable: single folder, no installer, no registry, config stored
   next to the exe.
6. Nice GUI per the ui-ux-pro-max skill (design system section below).
7. App icon embedded in the Windows exe and shown in the egui window.
8. Organize and find repos: pinned repos, category ribbons, fuzzy search,
   sort options.
9. Per-repo release filters: include/exclude prereleases, tag regex filter,
   asset name pattern.
10. Transparency and diagnostics: in-app log viewer, rate-limit health panel,
    statistics view.
11. Import/export of the full app state (JSON, optionally AES-encrypted with
    a password) so the setup moves to another machine by copying one file.

## Non-goals

- Installing or sideloading anything (user installs manually).
- Background/automatic update polling. Refresh happens only when the user
  presses Refresh.
- GitHub/GitLab account sync, stars, or follow-list import.
- Source types beyond GitHub and GitLab (no F-Droid, APKPure, HTML scraping).
- Release history browsing: only the latest release per repo is shown.
- Notifications/toasts: the user opens the app and presses Refresh.
- Multi-platform builds (Windows first; code should not hard-block later
  Linux/macOS ports, but no work spent on them now).
- Android-specific Obtainium+ features: Shizuku/root installers, APK signature
  verification, APK architecture selection, system app management, device
  compatibility hub, microG hub, Play Store mirroring, background fetch.

## Anti-goals

- Any telemetry, analytics, or network call beyond GitHub/GitLab APIs and
  VirusTotal APIs.
- Storing credentials anywhere outside the portable folder.
- Silent downloads of anything. Every download is a user action.
- Passing a flagged file without an explicit, informed confirmation.

## How it works

### Add a repo

User pastes a URL. Accepted forms:

- `https://github.com/owner/repo` (also `github.com/owner/repo`, with or
  without trailing `.git`)
- `https://gitlab.com/owner/repo` (also nested groups like
  `gitlab.com/group/subgroup/repo`)
- Self-hosted GitLab: user pastes any `https://gitlab.example.com/owner/repo`
  URL; the host is remembered for API base-URL derivation
  (`https://host/api/v4/projects/...`).

The app parses owner/repo (and host), verifies the repo exists by fetching its
latest release on add (fail with a clear error if the URL is wrong, the repo
has no releases, or the API rate-limits). Verified repo is saved to the repo
list with: display name (repo name), owner, host type, latest known release
tag + date, and the release's asset list snapshot (names, sizes, download
URLs).

### Per-repo release filters

Each repo can have optional filters, applied during fetch:

- **Prerelease toggle**: include or exclude prereleases when picking the
  "latest" release (default: exclude, like GitHub's `releases/latest`). If
  prereleases are included, the app fetches the releases list and picks the
  first match by published date. Repos whose only releases are prereleases
  are valid to track (status "no stable release yet" until the filter is
  enabled — see Add-time verification below).
- **Tag regex**: only consider releases whose tag matches (e.g. `^v\d+\.\d+`).
  Invalid regex rejected at input time.
- **Asset name pattern**: on the detail view, assets matching this glob-ish
  pattern (e.g. `*-x86_64.apk`, `*.zip`) sort first and get a "match" mark;
  non-matching assets remain listed but de-emphasized. Builder's call on
  glob crate choice (e.g. `globset`).

Filters are stored per repo in `repos.json`.

### Main screen — repo list

Table/list of tracked repos, one row per repo:

- Repo name (owner/repo), favicon-ish host badge (GH/GL glyph), category
  ribbon color (optional, user-assignable category per repo).
- Latest release tag + published date.
- "NEW" pill when the latest release is newer than the last one the user saw
  (seen state is per repo, cleared when the row is opened or marked seen).
- Status pill: OK / no releases / error (last fetch failed) / rate-limited.
- Pin state: pinned repos sort to the top of the list.

Above the list: a search box (fuzzy-matches repo name, owner, category, tag)
and sort control (by name, newest release, last checked — pinned always on
top).

A detail panel (or second screen) opens when a repo row is clicked:

- Release notes (markdown rendered as read-only rich text — egui has no
  markdown widget; render as plain text with basic styling. Builder's call on
  fidelity).
- Asset list: name, size, date, VT verdict badge (once scanned), Download
  button per asset.
- Link to open the release page in the browser.

### Refresh

One Refresh button refreshes all repos sequentially with small concurrency
(default 4 parallel requests, a setting). Each repo: one GitHub
`GET /repos/{owner}/{repo}/releases/latest` or GitLab
`GET /projects/{id}/repository/releases` (first item = latest). If the fetched
tag differs from the stored tag, mark row NEW, update stored state. State is
saved to disk immediately after a refresh completes (crash-safe).

GitHub auth: anonymous by default (60 req/hour). Optional PAT field in
Settings; if present, send `Authorization: Bearer <PAT>` (5000 req/hour).
GitLab: anonymous where the project is public; optional PAT similarly.

### Download + VirusTotal gate (the core safety feature)

When the user clicks Download on an asset:

1. App downloads the asset to a temp file inside the portable folder
   (`./tmp/`), showing progress (bytes, speed, percent).
2. App computes SHA-256 while streaming the download to disk.
3. **Hash lookup**: `GET https://www.virustotal.com/api/v3/files/{sha256}`.
   - Report exists → extract `data.attributes.last_analysis_stats`:
     `{malicious, suspicious, undetected, harmless, timeout}` counts. Verdict
     badge from those counts.
   - Report exists but `last_analysis_stats` is null (never analysed) → treat
     as unknown, offer upload fallback.
4. **Upload fallback** (only if hash lookup returns 404, and only if the user
   has the "Upload unknown files to VirusTotal" toggle on, default on):
   `POST https://www.virustotal.com/api/v3/files` (file ≤ 32 MB; for larger,
   request `GET /v3/files/upload_url` for a 650 MB-capable special URL). Then
   poll `GET /v3/analyses/{id}` every 30 s until `status: completed` (timeout
   after 10 min → verdict "unknown", let the user decide with a warning).
   Uploaded-by-this-app files get a permanent "scanned by upload" note on the
   badge.
5. **Verdict**:
   - `malicious + suspicious == 0` → clean: show green badge, move temp file
     to the download folder, done.
   - `malicious + suspicious > 0` → flagged: block. Modal warning dialog
     naming the asset, the counts (e.g. "3/72 engines flagged this file"),
     the top flagging engines, and a link to the VT report. Buttons:
     "Download anyway" (moves the file) / "Cancel" (deletes temp file).
     Default focus on Cancel.
   - **No verdict obtainable** — covers: no VT key configured, invalid key
     (401), quota exhausted (429), network error, hash lookup 404 with
     upload fallback disabled, file too large for upload with no report,
     analysis timeout. In all these cases: yellow "unverified" badge and a
     warn-and-confirm dialog ("Not scanned — proceed?") with Cancel as the
     default button. Never a hard block: the app stays usable without a VT
     key; every download just requires one explicit confirmation.
6. Badges are cached per (asset URL, sha256) in the state file so re-checks of
   the same asset are instant and free.

The user can also click an asset's "Scan" without downloading it — not
possible without the file for hash lookup, so scan always happens as part of
download. A "Re-scan" button on a previously-downloaded asset re-runs the
lookup to refresh the verdict.

### Add-time verification and prereleases

GitHub's `releases/latest` returns 404 for repos whose only releases are
prereleases (nightly-build repos). Add-time verification therefore:

- Fetches releases/latest first; on 404 (or empty), fetches the releases list.
- Repo adds fine with status "no stable release yet" if only prereleases
  exist. The user can then enable the per-repo prerelease filter.
- Add fails only on: bad URL, nonexistent repo, auth failure, or a repo with
  zero releases of any kind.

### Settings

- VirusTotal API key (plaintext in config file — accepted trade-off for
  portability; the user owns this trade-off, decided in conversation).
- Upload-fallback toggle (default on).
- Download folder (default `./downloads/` next to the exe).
- GitHub PAT, GitLab PAT (optional).
- Refresh concurrency (default 4).
- UI: light/dark/system theme; accent color choice from the design system's
  palette tokens.

### Diagnostics: log viewer, rate-limit panel, statistics

- **Log viewer**: in-app screen showing a rolling log (last N entries, N
  configurable, default 500) of every API call, rate-limit hit, download,
  VT verdict, and error with timestamps. Copy-to-clipboard and export-to-file
  buttons. Logs live in memory only (nothing persisted) unless the user
  exports them.
- **Rate-limit panel**: a Settings/diagnostics section showing live quota
  state read from response headers: GitHub (`x-ratelimit-remaining` /
  `x-ratelimit-reset`), GitLab (`RateLimit-Remaining` headers where present),
  VirusTotal (`x-quota_*` headers: minute/hour/day used vs total). Reset
  times shown as local time countdowns.
- **Statistics**: a small dashboard view: number of tracked repos, number of
  repos with NEW releases, total downloads this app performed, total VT
  scans, flagged files count. Stored as cumulative counters in the state
  file.

### Import / export

- **Export**: one JSON file containing config + repo list + seen state + VT
  badge cache + statistics. Optional AES encryption with a password
  (AES-256, PBKDF2 key derivation). Non-encrypted export also supported.
- **Import**: reads the file, validates, replaces current state (with a
  confirmation dialog). Handles both encrypted and plain JSON.
- Import/export reachable from Settings. Fits the portable-app story: move
  to another machine by copying one file.

### Data layout (all next to the exe)

```
GiRustStatus/
  GiRustStatus.exe
  config.toml          # settings incl. VT key, PATs, paths
  repos.json           # repo list + last-seen release state + VT badge cache
  tmp/                 # in-flight downloads
  downloads/           # default destination
```

Windows exe icon via `winresource` build dependency; same art as the window
icon set via `eframe`'s icon APIs at startup.

## Design system (ui-ux-pro-max)

To be generated with the skill's search tool at plan time and persisted to
`design-system/giruststatus/MASTER.md`. Direction agreed with the user in
conversation: "nice gui", clean modern, dark-first. Concrete tokens (colors,
type scale, spacing, icon rules) come from the generated design system and
govern the egui implementation via a central theme module. The design system
generation and persistence step is part of the implementation plan, not a
pre-build unknown.

## Constraints

- Rust, egui/eframe (user-chosen). `reqwest` (rustls, no native-tls for
  portability) for HTTP, `serde`/`toml`/`serde_json` for data, `sha2` for
  hashing, `winresource` for the exe icon, image crate to load the icon into
  egui, `globset` for asset patterns, `aes`+`pbkdf2` (or `age`/`ring`-based
  crate) for export encryption, `regex` for tag filters, a fuzzy-match crate
  (builder's pick, e.g. `fuzzy-matcher` or hand-rolled) for search. GUI code
  must be plain `eframe::run_native` with a central theme module; no web
  tech.
- Single binary, fully portable: no writes outside the portable folder (except
  the user-chosen download folder), no registry, no installer, no external
  runtime dependencies.
- Manual refresh only. No background threads polling anything.
- VirusTotal public-tier quotas: 4 req/min, 500/day. The app throttles its VT
  requests and shows quota state (headers `x-quota_*`) in Settings.
- GitLab API needs project ID: resolve owner/repo path → numeric ID via
  `GET /api/v4/projects/{url-encoded-path}` on repo add.

## Deliverables after planning

The user asked for two extra project documents alongside the code plan:

1. **Project context file** (`PROJECT.md` at repo root): what the app is, why,
   the locked decisions (Rust + egui, portable folder, manual refresh, VT
   hash-first flow), and pointers to the design doc and design system. Written
   for any person or agent picking the project up cold.
2. **Roadmap file** (`ROADMAP.md` at repo root): phased milestones
   (M1 skeleton + repo add/list, M2 refresh + releases, M3 download + VT gate,
   M4 filters/pins/search/sort, M5 diagnostics + import/export + stats,
   M6 polish + icon + packaging). Marked versioned, with current status.

These are written during planning (not left to the builder) and updated as
the project progresses.

## What's left to the builder

- Exact egui layout of main list vs detail (side panel vs second view) —
  builder's call, following the design system.
- Markdown rendering fidelity for release notes.
- How "NEW" pill state interacts with clicking rows (auto-clear on open vs
  explicit mark-seen) — builder's call.
- Error toast/banner patterns within egui.
- Fuzzy-match crate choice and sort tie-breakers.
- Log buffer implementation details (ring buffer, log levels).
```
