# Visual feedback, mistake replays, deathmatch and dynamic clicking

Objective: make run feedback visual and actionable, add step-by-step mistake replays, improve deathmatch, add dynamic clicking scenarios.
Branches: one per unit (`feat/visual-results`, `feat/mistake-replays`, `feat/deathmatch-plus`, `feat/dynamic-clicking`) in `../CustomAimTrainer-worktrees/<name>`, merged into `main` by the parent. Delivery: auto-chain per unit (one branch each). Test runner: `cargo test --workspace`; `cargo clippy --workspace --all-targets -- -D warnings`; calibrate example for scenario changes.

## Specs

- **S1 — Visual results and stats (unit U1).** User: "El tema de los stats y visualización de puntaje y correcciones y eso. Siento que es muy pesado, es como mucho texto y la verdad, si ni siquiera yo estoy leyéndolo, dudo mucho que cualquier otra persona vaya a hacerlo. Me gustaría que mejoráramos esa parte del UI y UX, no sé, por ahí piensa, invéntate alguna forma para visualizarlo de mejor manera y que se sienta como más intuitivo y, y útil."
  Design (assumption, chosen by the parent because the user asked us to invent it):
  - Results screen leads with visuals, not text: score hero with delta vs personal best; a row of 3–4 drawn gauges (accuracy ring, speed/kills-per-min, consistency/TTK spread, reaction) coloured good/warn/bad against the existing thresholds; ONE "main fix" card (from the potential tip / top finding) with an icon, a one-line headline and a "Watch it" button (hooks into S2); the miss compass drawn as an 8-wedge radial chart; flick directions as a polar/radar chart; pace as a sparkline.
  - Everything else (full metric grid, secondary panels) moves behind a collapsed "Details" toggle. Text per visible card is at most one short line.
  - Progress tab: skills drawn as bars or a radar instead of sentences; readiness as a coloured chip; sentences only on hover.
  - Reusable chart widgets live in `aim_app/src/ui/molecules/` (painter-based, no new plotting crate), with pure, tested geometry helpers.
- **S2 — Mistake replays (unit U2).** User: "implementáramos replays de momentos exactos en donde hice malas cosas y dieras como un ejemplo de paso a paso qué corregir, cómo, en dónde y en dónde. Tipo, no sé, hago un flick y hago un overflick. Mm, que paso a paso me enseñes primero lo que hice completo y lo después la parte justo donde estuvo mal, que es justo donde sobrepaso el target. Y luego un consejo de cómo se puede corregir específicamente ahí y un ejemplo de cómo sería haciéndolo ya correcto. Pero que sea... ¿Cómo se llama? Que sea realista, o sea, tampoco un flick perfecto el 100%, sino primero, o sea, eso. Y que sea, o se puede ver, de error, fallo por fallo, no así todo completo. Porque pues, la idea es pulir detalles para que el último se vea una mejora completa."
  Acceptance:
  - Core (`aim_core::analytics`): each detected mistake (at least overflick, underflick, late click, slow reaction, extra corrections) is a record tied to trace sample indices: segment start, error moment (e.g. the sample where the crosshair passes the target edge on an overflick), and end.
  - Each mistake carries a specific tip for that mistake kind and its numbers, and a deterministic, realistic corrected example motion for the same start view and target: human-like (reaction delay, minimum-jerk, small endpoint error that still lands inside the target, slight natural correction allowed), never a perfect straight snap.
  - UI: a step-by-step player, one mistake at a time with previous/next: step 1 plays the full motion; step 2 replays slowly the exact wrong segment with a marker at the error moment; step 3 shows the tip; step 4 plays the corrected example (overlaid on yours). Reachable from the results screen and from saved replays.
- **S3 — Better deathmatch (unit U3).** User: "Igual en la parte de los deathmatch que tenemos, siento que aún se puede mejorar bastante."
  Design (assumption): death recap after each death (what went wrong: crosshair placement error at first sight, reaction delay, moving while shooting); bot variety inside one arena (per-bot personalities: pusher, angle holder, jiggle peeker, with mixed skill); an adaptive ladder option where bot skill ramps with the player's performance; at least two new deathmatch arenas using them. Keep determinism and existing arenas' behaviour unchanged.
- **S4 — More dynamic clicking scenarios (unit U4).** User: "Y siento que nos faltan más escenarios para la parte de dynamic clicking."
  Acceptance: at least 6 new dynamic clicking scenarios with distinct mechanics (a new motion block such as accelerating/easing strafe or sine/zigzag paths is allowed, with tests and validation); registered in `builtin.rs`, added to the dynamic clicking routine(s); calibrate shows recruit < contender < veteran < elite and contender accuracy roughly 45–85%; existing scenarios' pinned results unchanged.

## Tasks

| ID | Specs | Route | Status | Commit |
|---|---|---|---|---|
| T1 | S1 | delegated (parallel writer, isolated worktree) | pending | |
| T2 | S2 | delegated (parallel writer, isolated worktree) | pending | |
| T3 | S3 | delegated (parallel writer, isolated worktree) | pending | |
| T4 | S4 | delegated (parallel writer, isolated worktree) | pending | |
| T5 | S1-S4 | inline (parent): merge, seam check, docs | pending | |

Route evidence: four independent units with disjoint surfaces, each well over 100 changed lines (Writer trigger a, parallelism). U1 and U2 both meet at the results screen: U1 owns `results_card.rs`/`results.rs`; U2 exposes its player and a `UiAction` to open it; the parent wires the "Watch it" button at merge if needed.

## Log

- L1 (2026-10-19, user, verbatim): "Ya abrió y funciona bien. Solo una cosa más. El tema de los stats y visualización de puntaje y correcciones y eso. Siento que es muy pesado, es como mucho texto y la verdad, si ni siquiera yo estoy leyéndolo, dudo mucho que cualquier otra persona vaya a hacerlo. Me gustaría que mejoráramos esa parte del UI y UX, no sé, por ahí piensa, invéntate alguna forma para visualizarlo de mejor manera y que se sienta como más intuitivo y, y útil. También otra cosa que me gustaría es que implement, implementáramos replays de momentos exactos en donde hice malas cosas y dieras como un ejemplo de paso a paso qué corregir, cómo, en dónde y en dónde. Tipo, no sé, hago un flick y hago un overflick. Mm, que paso a paso me enseñes primero lo que hice completo y lo después la parte justo donde estuvo mal, que es justo donde sobrepaso el target. Y luego un consejo de cómo se puede corregir específicamente ahí y un ejemplo de cómo sería haciéndolo ya correcto. Pero que sea... ¿Cómo se llama? Que sea realista, o sea, tampoco un flick perfecto el 100%, sino primero, o sea, eso. Y que sea, o se puede ver, de error, fallo por fallo, no así todo completo. Porque pues, la idea es pulir detalles para que el último se vea una mejora completa. Igual en la parte de los deathmatch que tenemos, siento que aún se puede mejorar bastante. Y siento que nos faltan más escenarios para la parte de dynamic clicking."
- L2: Mapping done by three explorers. Key facts: results card stacks ~11 text sections (`ui/organisms/results_card.rs`); only charts are `history_chart`, replay plot, heatmap, breathing circle; runs persist as `.aimreplay` input logs and `analytics::trace::trace` rebuilds per-frame samples; `Flick` discards sample indices (`analytics/flick.rs:282-293,350-368`); `sim::SimAimer` gives minimum-jerk human-like flicks; deathmatch lives in `aim_world::arena`; 22 moving click scenarios, no easing/sine motion.
