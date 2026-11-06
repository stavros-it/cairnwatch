# AGENTS.md — GiRustStatus Working Protocol

Any agent (or human collaborator) working in this repo follows these rules.
They sit above personal preference and above skill defaults.

## What this project is

GiRustStatus: a portable Windows desktop app (Rust + egui) that tracks
GitHub/GitLab repos the user follows, shows new releases, and gates every
asset download behind a VirusTotal scan (hash lookup first, upload fallback
second). No installation of downloads — the user installs manually.

Authoritative documents, in order:

1. `docs/superpowers/specs/2026-11-05-giruststatus-design.md` — the approved
   design. Do not change scope without the user's explicit approval.
2. `PROJECT.md` — project context: what/why, locked decisions, pointers.
   Keep current.
3. `ROADMAP.md` — phased milestones with status. Keep current.

Reference only: `ObtainiumPlus-main/` (upstream inspiration, excluded from
git, never modify, never treat as our code).

## Checkpoint protocol (MANDATORY)

The user controls model and reasoning effort per task. Never start a new task
with a different model or effort without asking first.

After **each completed task**:

1. **Prompt the user** to choose the model and effort for the *next* task.
   Always include a recommendation. The mapping table below is the default
   recommendation source; the user's pick wins.
2. Wait for the answer. "go" means take the recommendation.

## Per-task completion checklist (MANDATORY)

In order, after each task is functionally complete:

1. **Update `ROADMAP.md`** — move the task's status (planned → in progress →
   done), one line per task, no prose bloat.
2. **Update `PROJECT.md` when necessary** — new locked decisions, changed
   architecture, new conventions, anything a cold-start reader would need.
   Skip when nothing of substance changed.
3. **Commit** — one commit per task, Conventional Commits style
   (`feat:`, `fix:`, `docs:`, `chore:`, `refactor:`, `test:`). The commit
   includes the ROADMAP/PROJECT updates from steps 1–2.
4. **Verify before claiming done** — tests / build must pass; evidence
   (command output) before any "it works" statement.
5. **Prompt the checkpoint** (see above), then continue.

## Model & effort mapping (default recommendations)

Available: `tensic-project-235/GLM-5.3` (effort `low/medium/high/max`),
`tensic-project-235/GLM-5.3-Flash` (effort `low/medium/high`).

Rules: `low` = spec unambiguous, errors caught instantly. `medium` = small
judgment calls. `high` = edge cases or safety. `max` = hardest module only.

| Task type | Model | Effort |
|-----------|-------|--------|
| Planning, docs, design decisions | GLM-5.3 | high |
| Design system / visual judgment | GLM-5.3 | high |
| Scaffold, mechanical code | Flash | low |
| GitHub/GitLab API clients | GLM-5.3 | high |
| VirusTotal module | GLM-5.3 | max |
| egui shell / theme / large UI | Flash | high |
| VT gate UX (safety dialogs) | GLM-5.3 | high |
| Small UI features (search/sort/pin) | Flash | low |
| Diagnostics views (logs/rates/stats) | Flash | low |
| Import/export + crypto | GLM-5.3 | high |
| Icon + exe resource | Flash | low |
| Review / verify / fix loops | GLM-5.3 | high |
| Polish + packaging | Flash | medium |

## Project rules

- **Portable above all**: no writes outside the app folder (except the
  user-chosen download folder and secrets in Windows Credential Manager),
  no registry, no installer, no external runtimes, config (`config.toml`,
  `repos.json`) next to the exe. Never write secrets in plaintext.
- **Manual refresh only**: never add background polling.
- **Safety gate**: no asset reaches the download folder without a VT verdict
  or an explicit "unverified — proceed?" confirmation. Flagged files get a
  blocking dialog with Cancel as the default button.
- **No telemetry**, no network calls beyond GitHub/GitLab/VirusTotal APIs.
- Windows-first; don't hard-block other platforms, don't spend effort on them.
- Follow the design system in `design-system/giruststatus/MASTER.md` once
  generated (via ui-ux-pro-max skill) for all UI work.
- New dependencies: must serve the design doc's constraint list; prefer
  rustls-based crates (no native-tls) for portability.
- Tests next to modules (`#[cfg(test)]`), run `cargo test` before each
  commit that touches code.
