---
version: 1
slug: "src-main-rs"
primary_target: "src/main.rs"
related_targets: ["src/ui"]
---

# Surface brief: app shell (src/main.rs + src/ui/*)

Scope: the whole GiRustStatus desktop app UI — repo list, detail, gate,
settings, diagnostics. Visitor mode: Operate. Platform: native desktop
(egui/eframe), Windows-first.

Audience/job: single power user tracking GitHub/GitLab repos; task is
"refresh, see what's new, download safely". Proof: real repo rows, real
verdict states. Constraint: egui widget set only; no emoji icons — glyphs
drawn as text shapes or embedded vector; keyboard navigable.

## Direction contract

THESIS: the operator's telemetry wall — a wall of channels where one light
changed; refuses the category default of a flat gray table with one accent
by committing to signal semantics: slate panels, hairline seams, monospace
data voice, and state colors that speak only for verdicts.

OWN-WORLD: matte slate (#0F172A ground, #1B2336 panels) with 1px hairline
seams (#272F42), signal green #22C55E carrying every new/OK state, verdict
red #EF4444 and unverified amber #FACC15 reserved for the safety gate;
JetBrains Mono for all data (tags, dates, pills, hashes), IBM Plex Sans for
prose; raised from Developer Console: destructive confirms isolated in empty
space; raised from Catalog Sleeve: the verdict display owns one quiet field
in the detail view; raised from Datamatics: pixel-crisp hairlines and
tabular numerics in the rate panel.

STORY: the visitor scans the wall, instantly sees which channels changed
(green strip + filled pill), trusts the scan states (amber progress), and
never saves an unverified file without the wall telling them exactly what
they're taking.

FIRST VIEWPORT: left rail (Repos/Logs/Rates/Stats/Settings) on #0B1120 with
green active edge; top bar with search, green Refresh, `N new` chip; repo
rows as #1B2336 cards, leading 3px strip (green when NEW), GH/GL glyph tile,
owner/name, mono tag + date, version pill (filled green NEW / muted gray
seen), status pill; one row mid-scan with amber bottom progress hairline;
category ribbon on trailing edge. Matches
`.impeccable/mocks/decision/assigned-telemetry-room.html`.

ACCESSIBILITY (user-locked): target display 1920x1080, single window.
User is 45+, wears eyeglasses: base text >= 16px, monospace data
(tags/dates/pills/hashes) 14-15px, NOTHING below 12px anywhere. Muted
foreground raised to #A8B5C7 for small-size contrast on #1B2336. Row
height ~64px, hit targets >= 44px, focus ring 2px. Bump every MASTER.md
type size up one step when implementing the egui theme.

FORM: Telemetry Room, position 1 of my grounded list, fused from the roll's
assigned slot. SEED REFERENCE: seed:1a2111ead84a42f0.

FINISH: unreviewed and undocumented is unfinished; this build ends with the
finish review, the verdict, DESIGN.md, and every shipping raster carrying
its provenance.
