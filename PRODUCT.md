# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Stack

Rust + egui/eframe desktop app (user-locked). Native window toolkit, not
web-tech; "web" platform entry reflects that this is a desktop-rendered UI
with no browser dependency. Single portable binary for Windows.

## Users

Single user (the developer-owner): power user, comfortable with Git hosting
services, release artifacts, hashes, and VirusTotal. Runs the app ad-hoc on
Windows from a portable folder.

## Product Purpose

Track GitHub/GitLab repos the user follows; on manual refresh, surface new
releases; download release assets on demand — never install (manual
install). Every download passes a VirusTotal gate (hash lookup, upload
fallback, flagged → blocking confirm, unverifiable → warn-and-confirm).
Success: the user never misses a release they care about and never saves an
unscanned file without knowing it.

## Positioning

Obtainium+ inspired, but: desktop, Rust, single-binary portable, no account
sync, no telemetry, and a real VirusTotal API integration where Obtainium+
only links out to the VT website.

## Operating Context

Portable folder next to the exe: `config.toml`, `repos.json`, `tmp/`,
`downloads/`. No registry, no installer, no writes outside the folder
(except user-chosen download dir). Manual refresh only — no background
polling, no notifications. VT key + optional PATs stored in plaintext in
config (accepted trade-off for portability).

## Capabilities and Constraints

- Sources: GitHub + GitLab (incl. self-hosted GitLab). Nothing else.
- VT flow locked: download to temp → SHA-256 → `GET /v3/files/{sha}` → 404 →
  upload fallback (≤32 MB direct, ≤650 MB via upload_url; poll 30 s, 10 min
  timeout), toggle default on. Flagged → blocking dialog, Cancel default.
  No verdict obtainable → yellow unverified + confirm. Never hard block.
- Prerelease-only repos trackable ("no stable release yet"); per-repo
  filters: prerelease include, tag regex, asset glob.
- No telemetry. Network: GitHub/GitLab APIs, VT APIs, asset URLs only.
- Undecided: none material at init time.

## Brand Commitments

Name: GiRustStatus. Dark slate + green accent palette (design system
`design-system/giruststatus/MASTER.md`). Dark-only UI (light mode explicitly
not recommended by the design system; user confirmed dark-only). Icon art:
flat rounded-square, dark slate, radar-sweep arc, shield cutout, green
accent.

## Evidence on Hand

- Approved design doc: `docs/superpowers/specs/2026-11-05-giruststatus-design.md`
- Approved plan: `docs/superpowers/plans/2026-11-05-giruststatus.md`
- Generated design system: `design-system/giruststatus/MASTER.md`
- Reference (read-only): `ObtainiumPlus-main/`
- No screenshots, users, metrics, or testimonials exist yet; do not
  fabricate any.

## Product Principles

1. Portable above all — the folder is the app; nothing may break that.
2. Safety visible — every download shows its verdict or asks explicitly.
3. Manual by design — the user pulls; the app never pushes or polls.
4. Zero trust in third parties — no telemetry, no extra network surface.
5. Restraint in chrome — the repos and releases are the content; UI recedes.

## Accessibility & Inclusion

Colorblind-safe status/NEW indicators (no information in color alone;
text prefixes + position carry meaning). Keyboard navigation (egui
built-in). Visible focus states required by the design system.
User vision constraints: 45+, eyeglasses, 1920x1080 display. Minimum
body text 16px, monospace data 14px, hard floor 12px. High-contrast
muted text (#A8B5C7 on panel). Row heights ~64px, hit targets >= 44px,
2px focus rings.
