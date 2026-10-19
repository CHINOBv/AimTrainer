# Creative pass: features the player was not seeing
Objective: answer "what else would help a VALORANT player improve?" with features beyond the planned roadmap, starting from the research's recommended first batch.
Branch: `feat/creative` (worktree `../AimTrainer-worktrees/creative`) · Delivery: local merge to `main`, push at the end of the batch · Test runner: `cargo test --workspace`

## Specs

- **S1 Ranked session journal.**
  - A one-tap post-match entry: win/loss, RR change, a 1–5 self-rating, one tag.
  - It is linked to readiness (F3) and fatigue (F4) of that day, and it flags losing streaks.
  - **Tilt reset:** after two losses in a row, suggest a 3-minute routine (paced breathing timer, then easy smooth tracking). No medical claims.
- **S2 DPI deviation test.** Move the mouse a measured distance along a ruler; compute the real DPI against the nominal one and the deviation %. Offer to correct the sensitivity's DPI value.
- **S3 Personal warm-up dose.** From the player's own warm-up runs, find where scores plateau, and size the queue warm-up (F1) to the player instead of a fixed 5 min.
- **S4 Wrist/RSI breaks.**
  - Break prompts by minutes of continuous play, a weekly volume cap, and a short stretch list with a disclaimer (prevalence is sourced; break efficacy is unproven, and the app says so).
  - All configurable and off-able.
- **S5 Plateau detector.** No personal best in N weeks for a subcategory → suggest a change of stimulus (variant, sensitivity variation, emphasis switch) and a deload week. N is an estimate.
- **S6 Verifiable score proofs.** Runs are deterministic, so a shared replay can be re-simulated to confirm its score. Provide a share code that bundles scenario id + content hash + seed + inputs, and a verify function. It enables fair friend challenges without a server.

Rules:
- Pure logic in `aim_core` (`training::journal`, `training::ergonomics`, `sensitivity::dpi_check`, `training::progress` additions, `analytics::proof`); persistence through ports; UI in `aim_app` afterwards.
- No live game access of any kind.
- Every constant without a source is marked as an estimate in `docs/training-science.md`.

## Tasks

| ID | Specs | Route | Status | Commit |
|---|---|---|---|---|
| T1 | plan | inline | done | 1b76f80 |
| T2 | S1–S6 core (pure) | delegated (worktree `creative-core`) | done | b1838d7..7381cf1, merged |
| T3 | S1–S6 app UI + persistence | delegated, after T2 and after the coach/history app pieces merge | pending | |
| T4 | seam: verifier, review, merge to `main` | inline | pending | |

## Log

- L1 (2026-10-18) User: "Cuando termines todo esto, entonces ahora sí, por favor, viendo qué más podemos mejorar referente al propósito, ¿no? No sé, sé creativo, por ahí ábrete de mente y busca posibilidades. Y eso." and "Bien, busca funcionalidades que yo no esté viendo y hazte tú mismo las preguntas correctas para llegar a..."
- L2 (2026-10-18) User on the human coach: "un coach humano analiza las partidas de replays de Valorant. Del replay del juego. Analiza mis mecánicas durante el juego o durante una grabación por video. O en tiempo real. Viendo un stream de mi juego."
  - Verdict from Riot policy: real-time help is not approved; reflection "game over game" is fine.
  - Safe now: a manual death-review log (a later spec).
  - An offline sidecar for the player's own finished recordings needs an explicit policy decision by the owner, and must never include enemy/head detection.
  - Live analysis is forbidden.
- L3 (2026-10-18) The research ranked 22 proposals. This first batch follows its recommendation (journal, DPI test, warm-up dose, RSI breaks, verifiable proofs) plus the plateau detector.
- L4 (2026-10-18) T2 merged.
  - Modules: `training::journal` (tilt reset), `sensitivity::dpi_check`, `training::warmup::warmup_dose`, `training::ergonomics`, `progress::plateaus`, `analytics::proof` (FNV-1a content hash, base64url share code, replay re-simulation).
  - Decision for T3: `queue_warmup` silently caps at 12 runs × 30 s = 6 min, although the Settings slider allows 30 and the dose goes up to 15. Raise the cap to 15 min (30 runs) so explicit settings are honoured, with a test.
