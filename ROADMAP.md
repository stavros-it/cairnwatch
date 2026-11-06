# ROADMAP

Tasks map 1:1 to `docs/superpowers/plans/2026-11-05-giruststatus.md`.
Status: planned → in progress → done. Update after every task.

## M0 — Planning
- [x] Design doc approved
- [x] AGENTS.md protocol
- [x] Task plan written
- [x] PROJECT.md written

## M1 — Skeleton
- [x] Task 1: Cargo scaffold + design system generation
- [x] Task 2: Core state — models, config, persistence

## M2 — Sources
- [x] Task 3: Repo URL parsing
- [x] Task 4: GitHub + GitLab release clients

## M3 — Core feature
- [x] Task 5: VirusTotal client + download engine + command bus
- [x] Task 6: egui shell — theme, main list, repo detail
- [x] Task 7: Download + VT gate UX

## M4 — Organization
- [x] Task 8: Search, sort, pins, filters polish

## M5 — Diagnostics & portability
- [x] Task 9: Diagnostics — logs, rate panel, statistics
- [x] Task 10: Import/export + settings view

## M6 — Ship
- [x] Task 11: Icon + packaging
- [x] Task 12: Final review + verification pass
- [x] Fix: config missing-file misclassified as corrupt (Windows os error 2 string-sniff) — typed LoadError
- [x] Fix: repo removal undiscoverable — Remove button in detail view + right-click hint
- [x] Fix: UI contrast/glitches — deep-green selection (was bright green under light text), visible hover, colored status pills, stranded "new" filter, pin marker, row hover, date-only
- [x] Fix: visual pass (screenshots of every view) — themed buttons/checkbox/slider, panel padding, full-width rail nav, 44px uniform controls, tofu glyphs removed, asset grid, wrapped stats tiles, rates remaining+reset time, settings grid, masked tokens/passwords, Add dialog Enter/focus/pending, markdown cleanup; debug-only devshots driver
- [x] Fix: gate/dialog verification — gate is truly modal (dim backdrop, click-through blocked, focus trapped), Cancel focused on open with 2px ring, Enter/Esc cancel; re-scan no longer hijacks the download gate (clean re-scan showed "Download failed", mid-download re-scan discarded the file) — own result dialog; category palette shows the derived color; ribbon for release-less repos; Done dialog "Open folder"
- [x] Feat: secrets in Windows Credential Manager (keyring only, user decision) — VT key + GitHub/GitLab tokens out of config.toml, auto-migration of plaintext configs, encrypted export carries keys / plain export doesn't, import keeps stored keys; export salt/IV from OS CSPRNG
- [x] E2E with real network + user's key: GitHub+GitLab add/refresh; no-key Not-scanned gate (cancel deletes temp, confirm delivers); VT hash lookup clean (0/68, 0/64, 0/67) → auto-delivered, SHA-256 = upstream; badges + key persist across restart. Fixed: Rates never updated (headers never parsed, snapshot overwritten); VT sends no quota headers → app-side request counts vs free tier; GitLab "0 B" sizes → "—"

## Post-1.0 backlog (deferred minors from the final review)
- [ ] tmp/ startup cleanup of stale temp files
- [ ] Streamed VT upload (avoid whole-file RAM read for 650 MB files)
- [ ] "Last checked" sort needs a last-check field per repo
- [ ] Asset date column in detail table
- [ ] Rate-limit reset as local-time countdown instead of raw epoch
- [ ] Timestamps + full API-call coverage + configurable ring size in logs
- [ ] CSPRNG salt/IV (getrandom) for export encryption
- [ ] Smoke test against the app's own client/DTO code
