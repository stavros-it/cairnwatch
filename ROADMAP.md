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
- [x] E2E upload fallback (random 64 KB files via local server): 404 → upload → poll → clean → delivered in 49 s. Fixed: Scanning dialog had no Cancel/stage/elapsed (up to 10 min locked behind the modal); cancel now stops VT polling within ~1 s and deletes temp; shared cancel flag reset by the next download could un-cancel a running flow (now per-download); late events from a cancelled flow could hit a newer download (events now matched to the gate's asset)

- [x] Rename to Cairnwatch (exe, window title, version info, User-Agent, export/log filenames); window opens centered. Credential Manager service name kept as "GiRustStatus" so stored keys survive
- [x] Polish: exe version info (0.1.0, product/description), README (setup, keyring, move via encrypted export), fresh-folder start verified, GUI subsystem, icon embedded
- [x] Feat: mark installed version per repo (detail "Mark <tag> as installed"/Clear; list shows "update"/"installed" pill; `installed_tag` in repos.json)

## Post-1.0 backlog (deferred minors from the final review)
- [x] tmp/ startup cleanup of stale temp files
- [x] Streamed VT upload (avoid whole-file RAM read for 650 MB files)
- [x] "Last checked" sort needs a last-check field per repo
- [x] Asset date column in detail table
- [x] Rate-limit reset as local-time countdown instead of raw epoch
- [x] Timestamps + full API-call coverage + configurable ring size in logs
- [x] CSPRNG salt/IV (getrandom) for export encryption (already done in the keyring task)
- [ ] Smoke test against the app's own client/DTO code
