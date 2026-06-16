# Validator Clock Design Plan

Open Design audit direction: keep the dark sportcar-dashboard concept, but make the UI feel like one instrument cluster instead of several adjacent card styles.

## Phase 1 - Visual System Tightening

- Normalize the dark surface stack: page background, panels, cards, tables, and controls should share one carbon-glass material language.
- Keep the existing blue / green / gold semantic accents, but reduce random glow strength and make accent use more intentional.
- Improve typography rhythm: consistent label sizes, stronger numeric hierarchy, and quieter secondary copy.
- Make repeated components feel related: status pills, network tabs, stat cards, validator rows, and map controls.
- Improve contrast in the network message card and table rows without making the dashboard brighter overall.

First focused pass:

- Added shared table material tokens for shell, header, row, hover, divider, and chip surfaces.
- Tightened validator table density by reducing desktop row/header height and aligning row separators to one hairline system.
- Brought recent-round panels and empty states onto the same carbon-glass material stack as the main round panels.
- Reduced validator type badge palette drift by remapping outlier badge colors back into the dashboard's blue / green / gold / red / cyan accent family.
- Kept the DOM untouched; this pass is CSS-only to preserve the current product behavior.

## Phase 2 - Dashboard Composition

- Rebalance the top cockpit area so Election, Clock, and Network read as one instrument row. Done in the first Phase 2 pass with a true desktop grid instead of absolute side-panel placement.
- Add a more deliberate center-stage treatment around the clock, preserving the current SVG gauge. Done in the first Phase 2 pass with a shared cockpit bridge and central gauge halo.
- Step 3 strengthened the first viewport into a single cockpit cluster: a visible horizontal backplate now spans Election, Clock, and Network; the side modules use matched height, padding, material, and connector rails; Network is back to one outer module with inset selector/message surfaces; mobile and tablet hide the rails and stack cleanly.
- Step 3b premium restraint pass reduced the cockpit structure instead of adding more glow: the desktop backplate is now a quieter recessed plinth, connector blocks became faint datum lines, panel color shadows were toned down, and Election / Network rely more on material, spacing, and heading hierarchy. Mobile keeps the existing clean stacked order.
- Step 3c critique: the previous pass still depended on a soft plinth, circular clock haze, and a horizontal light line, so Election, Clock, and Network remained separate card systems connected by effects. The redesign replaces that with one matte cockpit shell, hard internal dividers, integrated side bays, and a clipped central gauge bay so the clock stays heroic through structure instead of glow.
- Step 3d structural refinement: removed the full-height divider paint that cut through Election and Network, shifted separators to clipped bay edges, matched side-panel chamfers to the clock bay, removed the center horizontal light band, and kept tablet/mobile on the clean stacked card layout.
- Step 3e micro-layout pass: kept the cockpit structure intact while giving Network selector internals more breathing room, reducing portrait edge pressure, aligning tab/message gutters, and harmonizing Election labels, countdowns, tab text, and message copy to one compact dashboard type rhythm.
- Review screenshots against live data after every embedded-asset rebuild because the CSS is compiled into the Rust binary.

## Phase 3 - Data Density And Polish

- Refine validator table scanning: row separators, hover state, numeric alignment, and badge contrast. First CSS-only pass completed; still needs screenshot review against dense real data.
- Review long validator/source values for clipping behavior.
- Add final visual regression screenshots for desktop and mobile before merging.

Step 4 result - Validator Tables + Responsive Polish:

- Kept the Step 3e cockpit/top console untouched and localized the design patch to validator table/card presentation plus copy feedback accessibility.
- Rebalanced current and recent-absent table columns with minmax grid tracks so source, validator, history, and numeric fields keep the same dense scanning rhythm without forcing horizontal scroll.
- Strengthened row scanning with focus-within parity, restrained left-edge row emphasis, tabular numeric alignment, quieter dense-data weights, and compact history dots.
- Tightened validator/source address handling through mono ellipsis, larger copy hit targets, visible focus rings, and polite copied/failed feedback.
- Refined mobile cards for 390 / 360 / 320 width behavior: clear type row, labelled source/validator/history/numeric blocks, right-aligned numeric values, and equal-width metric cells through the smallest checked viewport.
- Kept source/type badges on the existing blue / green / gold / red / cyan family and avoided new glows, orbs, or unrelated layout changes.

Step 5 result - Tycho Map Module:

- Kept the Step 3e cockpit/top console and Step 4 validator tables as fixed baselines; the implementation patch is localized to the Tycho map module styles plus a small status-state hook.
- Reframed the MAP toggle as a compact instrument control with 44px height, chamfered geometry, active state indicator, disabled state, hover/active feedback, and visible focus ring.
- Rebuilt the Tycho Validator Map panel shell as a clipped carbon-glass bay with cockpit-matching edge treatment, restrained gold/green/cyan accents, and a framed dark map canvas instead of a separate embedded-widget feel.
- Polished the panel titlebar, summary chip, reset control, MapLibre navigation control, status overlay, popup shell, and popup rows so they share the dashboard's glass/material hierarchy.
- Added distinct loading, empty, and error status overlay states without changing map data loading, fallback behavior, clustering, popups, or reset behavior.
- Improved mobile behavior with full-width MAP control, 44px reset/navigation tap targets, tighter panel padding, readable summary/status placement, and stacked popup rows.

Step 5b result - Tycho Map Module Screenshot Critique:

- Kept the refinement scoped to the Tycho map module plus the adjacent MAP toggle icon treatment; cockpit, network/election panels, validator tables, data loading, clustering, popups, reset behavior, and responsive map layout are unchanged.
- Reduced the map title from the previous oversized 18px treatment to the same compact uppercase dashboard heading scale used by round panels, with quieter color and matched weight.
- Replaced the route-node title icon and pin toggle icon with a restrained locator/crosshair glyph that fits the existing thin-stroke cockpit icon family.
- Rationalized the panel corner language: the map now has a rounded outer shell like the main cockpit, while the chamfered geometry moves to the inset edge detail so the module feels integrated rather than like a separate clipped widget.

Step 5c result - Election Timing Semantic Rails:

- Restored a restrained left-edge semantic cue on the two Election cockpit timing cards without changing the cockpit grid, rounded/chamfered structure, countdown typography, map, tables, DOM, or behavior.
- The Active round window rail now inherits the existing live `--card-accent` value from `public/app/metrics.js`, so it follows the same blue/green state as ROUND ENDS IN.
- The Elections window rail uses the existing gold `--card-accent`, matching ELECTIONS START/END IN while staying quieter than the validator table rails.
- Implemented the cue as an inset 3px pseudo-element inside `.election-time-card`, avoiding new glow, orb, smudge, or extra surface effects.

Step 6 result - Design-System Coherence Pass:

- Kept the patch CSS-only and left map loading, Network message timing, cockpit structure, validator data, DOM, and JavaScript behavior unchanged.
- Normalized `TYCHO VALIDATOR MAP` back to the same compact 13px uppercase module-heading scale used by the same-level Election and Network labels, so the long title no longer reads as a higher hierarchy.
- Reworked the MAP panel as a closer cockpit relative: rounded outer frame, rounded inset liner, and a chamfered map bay, instead of a rounded shell followed immediately by a clipped inset.
- Replaced the Election timing cards' simple pseudo-element rails with the richer dashboard accent language used by Network selected tabs, round stats, and validator tables: a 4px left edge plus a restrained horizontal accent wash driven by the existing `--card-accent`.
- Preserved the Network message block as an intentional transient message surface; no empty-state styling was added around the captured fade-out moment.

Step 7 result - Design Token + Rhythm Audit:

- Kept Step 6's dark sportcar dashboard concept intact and limited implementation changes to CSS plus this documentation and critique update; no data behavior, timers, DOM, Network timing, or map loading changed.
- Added local rhythm/control tokens for repeated values that had drifted across cockpit, MAP, round cards, validator tables, and mobile cards: panel gaps, card gaps, panel padding, mobile padding, shell/cockpit radii, control height, panel/control icon sizes, focus outline, control transitions, card shadow, module heading size, round heading size, and numeric value size.
- Rebound the cockpit shell, Election/Network panels, MAP toggle/panel/reset control, validator round headings, round stat cards, table row spacing, and mobile module padding to those tokens where it removed meaningful inconsistency without broad refactoring.
- Normalized validator table row rhythm with explicit row min-height and block-padding variables, while preserving existing desktop/tablet grid tracks and compact scan density.
- Brought round-card title icons down to the same 34px instrument icon tier used by cockpit, map, recent-round, and stat icons, tightening heading alignment without changing the content hierarchy.
- Preserved the 390 / 360 / 320 mobile structure: single-column cockpit, full-width MAP control, 44px map/reset/navigation targets, one-column round cards, and the existing 340px edge tightening.

Step 8 result - Targeted Simplification:

- Kept the Step 7 dark sportcar dashboard design intact and limited the implementation patch to CSS plus this documentation and critique update; no DOM, data loading, map behavior, timers, source display logic, or validator rendering changed.
- Removed the remaining clipped/chamfered geometry from the cockpit center bay, clock bay, Election/Network side bays, and Tycho map shell so those surfaces now follow the rounded panel language used across the rest of the dashboard.
- Removed the MAP toggle status dot, including its expanded green state and disabled fallback, while preserving the compact icon + MAP label, active/hover/focus styling, and 44px control target.
- Simplified TON SOURCE metadata labels to one restrained neutral-cyan dashboard style so source ownership scans quietly; TYPE contract badges keep their distinct blue / green / gold / red / cyan labels.
- Preserved the cockpit structure, map dimensions, material stack, hierarchy, table density, and responsive behavior while reducing visual concepts that were not developing across the site.

Step 9 result - Screenshot Critique Frame + Typography Calm:

- Kept the Step 8 dark sportcar dashboard design, layout, DOM, map behavior, timers, validator data behavior, and table alignment intact; the implementation patch is CSS-only plus this documentation and critique update.
- Simplified cockpit framing to one outer shell by removing the redundant inner cockpit liner, central cockpit bay frame, and clock-stage inset frame. The clock remains centered by the existing grid and SVG scale, but the cockpit no longer competes with the single-frame round panels.
- Simplified the Tycho MAP module by removing the panel inset liner and map-canvas overlay frame while preserving the outer map shell, canvas dimensions, controls, status overlay, popups, and MapLibre behavior.
- Reduced long-viewing eye strain by muting the global white text token slightly and lowering weights on module headings, round/meta labels, round stat values, election/date values, table headers, validator cells, addresses, numbers, source metadata, and type badges.
- Preserved semantic accent colors and hierarchy: active/election state colors, round blue/green identity, status colors, map controls, and validator type color families remain recognizable, just supported by calmer surrounding typography.

Step 10 result - Clock Instrument Prominence:

- Kept Step 9's calmer typography, single outer cockpit shell, side-panel structure, DOM, JavaScript behavior, API behavior, map behavior, timers, validator tables, runtime status dot, and validator history dots intact.
- Restored the clock as the hero instrument by making `.clock-stage` itself one larger rounded enclosure around the existing SVG gauge, rather than bringing back the removed nested cockpit/bay frame stack.
- Increased the gauge enclosure and SVG scale enough to read above the Election and Network panels while keeping the enclosure calm: low-contrast border, restrained cockpit glass, and softer shadow.
- Removed the pseudo-dot from round status badges so active and elections-open/election labels render as plain text badges; status color remains in the badge border/background/text treatment.

Step 11 result - Cockpit Footprint Reduction:

- Kept Step 10's single larger calm clock frame, text-only active/elections-open round badges, calmer typography, DOM, JavaScript behavior, Rust API behavior, runtime status dot, validator history dots, map behavior, timers, and validator rendering intact.
- Reduced the desktop cockpit footprint proportionally by tightening the outer top grid, center clock enclosure, clock SVG, Election/Network panel heights, Network message block, selector enclosure, and the gap into the round tables.
- Preserved Network breathing room by keeping its existing tab rhythm and a readable selector/message split instead of compressing every inner control equally.
- The first viewport should now expose more of the round tables below the cockpit on desktop while retaining the premium dark sportcar dashboard hierarchy.

Step 12 result - Clock Frame Removal + Fold Recovery:

- Canceled the follow-up compression direction after screenshot review showed the cockpit side panels could be broken by over-compressing their fixed height.
- Removed the large internal `.clock-stage` glass frame around the clock SVG instead of further reducing Election and Network panel content space.
- Restored content-safe Election/Network panel height while tightening the page title rhythm, MAP control gap, round-card header/stat/table density, and validator row/header height.
- On the checked 1630x920 desktop viewport, the first screen now preserves the cockpit composition and exposes four validator rows below the round cards.

Step 13 result - Manual Final Polish:

- Increased the Election countdown values so ROUND ENDS IN and ELECTIONS START/END IN regain dashboard-instrument visibility without making the labels heavier.
- Restored breathing room between the VALIDATOR CLOCK title ornament and the cockpit shell by increasing the titlebar rhythm and making the gold divider mark slightly more deliberate.
- Expanded the MAP toggle back into a confident 46px control instead of a compressed pill.
- Rebalanced the open map heading: the reset control now aligns with the right-side summary instead of sitting tight against the title, and the map canvas uses even 20px panel gutters.
