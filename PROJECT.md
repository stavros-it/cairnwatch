# PROJECT.md — Cairnwatch Project Context

Written for any person or agent picking this project up cold. Keep current;
update when decisions change.

## What it is

Portable Windows desktop app (Rust + egui). Tracks GitHub/GitLab repos the
user follows (added by pasting URLs, never account-synced). On manual
Refresh, fetches each repo's latest release. Downloads assets on demand —
never installs (user installs manually). Every download is gated by a
VirusTotal scan: hash lookup first, upload fallback second; flagged files
block behind an explicit confirmation.

Inspiration: Obtainium+ (`ObtainiumPlus-main/`, read-only reference, excluded
from git). Borrowed concepts: release tracking from source, categories/pins,
VT safety scan (ours uses the real VT API, Obtainium+ only links out),
log viewer, rate-limit health, import/export, statistics. Dropped by design:
all Android-specific features, background polling, 30+ source types.

## Locked decisions (user-approved)

- Rust + egui/eframe. No web tech. Single binary.
- Fully portable: config (`config.toml`), state (`repos.json`), `tmp/`,
  `downloads/` all next to the exe. No registry, no installer, no writes
  outside the app folder (except user-chosen download dir and the secrets
  below). eframe's own persistence API is NOT used.
- Manual refresh only — no background polling, no notifications/toasts.
- Sources: GitHub + GitLab (incl. self-hosted GitLab). Nothing else.
- VT flow: download to temp → SHA-256 → hash lookup (`GET /v3/files/{sha}`)
  → verdict. 404 → upload fallback (`POST /v3/files`, >32 MB via upload_url,
  poll `/v3/analyses/{id}` 30 s interval, 10 min timeout), toggle default ON.
  Flagged (malicious+susp > 0) → blocking dialog, Cancel default. No verdict
  obtainable (no key/401/429/network/upload-off/too-large/timeout) → yellow
  "unverified" + warn-and-confirm, never a silent pass or hard block.
- Prerelease-only repos are trackable ("no stable release yet" status);
  per-repo filters: prerelease include, tag regex, asset name glob.
- Secrets (VT key, GitHub/GitLab tokens) in Windows Credential Manager (user decision 2026-11-06),
  "keyring only": never in `config.toml` (`Config` skips them on
  serialize), never in the plain export; the encrypted export carries them.
  `core::secrets` migrates old plaintext configs at start (rewrite only if
  all moved), syncs on Settings edits, and an import without keys keeps
  the stored ones. Supersedes the earlier "plaintext for portability"
  decision. Debug devshots runs use an in-memory store, never the real one.
- Export salt/IV come from the OS CSPRNG (`getrandom`); the old time+pid
  hash was predictable.
- Anonymous GitHub by default (60 req/hr), optional PAT.
- App icon: exe resource (`winresource`) + egui window icon; art: dark slate
  rounded square, radar-sweep arc, shield cutout, green accent.

## Layout

- `docs/superpowers/specs/2026-11-05-giruststatus-design.md` — approved design
- `docs/superpowers/plans/2026-11-05-giruststatus.md` — task plan (12 tasks)
- `ROADMAP.md` — milestone status
- `AGENTS.md` — working protocol: checkpoint (prompt user for model+effort
  after each task), per-task checklist (update ROADMAP → update this file if
  needed → verify → commit), model/effort mapping table, project rules
- `design-system/giruststatus/MASTER.md` — UI tokens (generated Task 1,
  gitignored)
- `ObtainiumPlus-main/` — read-only reference

## Architecture

Three layers, one binary:
- `src/core/` — models, config, state, paths, URL parsing, import/export.
  Pure; fully unit-tested.
- `src/net/` — reqwest clients (GitHub, GitLab, VirusTotal), download
  engine (streams to `tmp/*.part`, sha256 while streaming), command bus
  (worker thread + tokio runtime; UI sends `Command`s, receives `Event`s;
  download/refresh flows spawn as tasks so commands never queue behind a
  10-minute VT poll). VT throttle: 1 request / 16 s, enforced by sleeping.
  No background timers.
- `src/ui/` — eframe shell: theme (Telemetry Room tokens), list, detail
  (VT badges per asset + Re-scan), gate (download/VT state machine;
  flagged/unverified confirms carry the real temp path; cancel is wired
  through the bus), settings (save-on-change), diagnostics views.

UI conventions (learned from the visual pass):
- Controls are 44px tall (`interact_size.y`, accessibility floor); pills
  are painted (`widgets::pill`), never Frames, or they stretch to row
  height. Selection = deep green `SELECTED` + light text, never bright
  green under light text.
- egui's default fonts lack `✓ → ← ↓ ✕` (render as tofu). Safe: `— … • ·
  ⚠ 📌 ⬇`. Check new glyphs with the devshots `glyphs` command.
- Visual testing: `src/devshots.rs` (debug builds only, compiled out of
  release). Run the debug exe with `GIRUST_SHOTS=<dir>` and write
  commands to `<dir>/cmd.txt` (`view …`, `select N`, `add`, `shot name`,
  `glyphs`, `quit`); it saves PNGs via egui's own screenshot. Needed
  because this machine's agent session can't screen-grab GL windows.
  Also `mouse/click/rclick X Y`, `key esc|enter|tab` (injected via
  `raw_input_hook`), `gate <phase>`, `rescan …`, `category N`,
  `focusinfo`. Full list in the module doc.
- Modals: `widgets::modal_backdrop` (egui 0.29 has no modal) on
  `Order::PanelResizeLine`; safety dialogs also trap focus to their own
  buttons and default to Cancel. `widgets::focus_ring` for buttons with
  custom fill/stroke (egui's own focus stroke is hidden there).
- Re-scan results never enter `GatePhase`; they use `rescan_result`.
- Rate limits: every API response calls `net::ratelimits::observe`, which
  *merges* into one process snapshot (each API sends only its own
  headers); the UI re-reads it after any bus event.
- E2E driver commands (real network): `addurl`, `refresh`,
  `download R A`, `status`; `GIRUST_SHOTS_KEYRING=real` opts into the
  real Credential Manager (default: in-memory).
- VirusTotal returns no quota headers on `/files` (verified live), so the
  Rates VT rows show app-side counts (`observe_vt`) vs free tier 4/min,
  500/day.
- Gate events are matched to the gate's repo+asset (`is_gate_asset`);
  each download has its own cancel flag, checked between chunks, before
  each VT step, inside the analysis poll, and before `FileReady`.

Status: **v0.1.0 shipped** — all 12 plan tasks complete, final review
fixes in. See `ROADMAP.md` post-1.0 backlog for deferred minors.

## Known limitations

- Dark theme only (design system: light not recommended; user-approved).
- Import reads a fixed file next to the exe (`cairnwatch-export.json` /
  `.enc`); no native file picker (portability).
- VT engine list absent for uploaded-by-us verdicts (analysis payloads
  carry stats only, not per-engine results).
- Deferred minors in `ROADMAP.md` post-1.0 backlog.
