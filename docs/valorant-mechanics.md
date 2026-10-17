# VALORANT mechanics reference

Every gameplay constant in `aim_core` comes from this page. Confidence tags:

- **verified**: the value comes from Riot or the official wiki, or several independent sources agree on it.
- **approximate**: a community measurement or a derivation.
- **estimate**: no reliable source exists; this is our best guess, and it is configurable where possible.

Research date: 2026-10-16.

## Sensitivity

| Value | Used as | Confidence | Source |
|---|---|---|---|
| Yaw: 0.07° per count at sens 1.0 | `Game::Valorant.yaw_deg_per_count()` | verified | https://sensiconverter.com/mouse-sensitivity-converter/valorant/ , https://sensitivitymatch.com/mouse-sensitivity-converter/ |
| Pitch equals yaw | the same factor applies to both axes | approximate | converters and Aimlabs' VALORANT profile treat them as equal |
| cm/360 = 360 × 2.54 / (0.07 × sens × DPI) | `Sensitivity::cm_per_360` | verified | https://mousesensitivityconverters.com/how-it-works.php |
| CS2 / Apex yaw: 0.022 (VAL→CS2 factor ×3.1818) | `Game::Cs2`, `Game::Apex` | verified | https://www.mouse-sensitivity.com/forums/topic/6875-csgo-to-valorant-sensitivity-conversion-is-wrong/ |
| Overwatch 2 yaw: 0.0066 | `Game::Overwatch2` | approximate | common converter value |
| A scoped/ADS multiplier of 1.0 is a 100% horizontal monitor match, so zoomed °/count = base × mult / zoom | `Sensitivity::zoomed_deg_per_count` | approximate | https://game-sens.jor.dev/ , https://edpi-calculator.org/tools/valorant-scoped-sensitivity |

## Field of view

| Value | Confidence | Source |
|---|---|---|
| Vertical FOV is locked at ≈70.53°, which gives 103° horizontal at 16:9 (Hor+) | verified | https://www.mouse-sensitivity.com/forums/topic/9354-about-wrong-fov-about-valorant-in-43/ |
| Horizontal FOV per aspect ratio: 4:3 → 86.63°, 16:10 → 97.05°, 21:9 → 117.57° | verified (follows from the formula) | https://fovcalculatorpro.com/games/valorant-fov/ |
| Zoom levels: Vandal/Phantom 1.25×, Guardian 1.5×, Spectre 1.15×, Marshal/Outlaw 3.5×, Operator 2.5×/5× | verified | https://wiki.playvalorant.com/en-us/Vandal , https://wiki.playvalorant.com/en-us/Operator |
| Zoomed HFOV = 103 / zoom | approximate (Riot does not document it) | https://game-sens.jor.dev/ |

## Movement

| Value | Confidence | Source |
|---|---|---|
| Run speed: knife 6.75 m/s, rifles 5.4, Classic 5.73, Operator 5.13 | verified | https://wiki.playvalorant.com/en-us/Melee , https://wiki.playvalorant.com/en-us/Classic |
| Walk is ≈0.6× run speed | approximate | https://devtrackers.gg/valorant/p/f29d0021-walking-running-accuracy-too-high , https://vgraphs.com/weapons/rifles/vandal |
| Fully accurate at ≤27.5% of max speed (patch 3.0); Operator at 15% | verified | https://playvalorant.com/en-us/news/game-updates/valorant-patch-notes-3-0/ |
| Stopping curve after **releasing** keys (fraction of run speed): 61% at 0.065 s, 25% at 0.110 s, 0 at 0.170 s | verified for 2020; the rules have changed since | Riot designer test: https://devtrackers.gg/valorant/p/f29d0021-walking-running-accuracy-too-high |
| Stopping curve while **counter-strafing**: 61% at 0.055 s, 25% at 0.104 s, 0 at 0.160 s | same as above | same as above |
| Acceleration ≈18.75 m/s² with a rifle | approximate | https://www.reddit.com/r/VALORANT/comments/utc7qf/valorant_physics_derived/ |

**How the model works (`movement.rs`).** We do not fit a friction formula. Each frame, the deceleration is replayed along the measured curve, starting from the current speed fraction. That way, partial states (a key tap, or a release mid-acceleration) stop the same way the game does. One assumption is ours: the curves scale with each weapon's run speed, since they were measured with a Phantom.

## Hitbox and camera

In VALORANT every agent shares the same hitbox. Riot does not publish its dimensions, so these are estimates (`HumanoidDims`):

| Value | Confidence | Source |
|---|---|---|
| Height 2.0 m | estimate | a Riot dev reportedly said agents are "two virtual meters" tall: https://www.esportstalk.com/blog/riot-dev-raises-curtain-on-valorant-crosshair-placement/ |
| Head 0.25 m (≈13% of height) | estimate | pixel study: https://www.reddit.com/r/VALORANT/comments/pu9pbv/ |
| Body width 0.5 m, depth 0.3 m, waist at 1.0 m | estimate | none |
| Eye height 1.82 m, below the head center, so a level crosshair hits the mouth | approximate | https://www.reddit.com/r/VALORANT/comments/1ds8juc/head_level_feels_weird/ |

## Weapons

The source for all of these is the official wiki: https://wiki.playvalorant.com/en-us/Vandal , https://wiki.playvalorant.com/en-us/Phantom , https://wiki.playvalorant.com/en-us/Weapons

| Weapon | Fire rate (rounds/s) | First-shot spread | Run spread | Head / body / legs damage |
|---|---|---|---|---|
| Vandal | 9.75 | 0.25° | +6° | 160 / 40 / 34 |
| Phantom | 11 | 0.2° | +6° | 156 / 39 / 33 |
| Guardian | 5.25 | 0.1° | n/a | 195 / 65 / 48.75 |
| Operator | n/a | 5° unscoped | n/a | 255 / 150 / 127 |

Two of these values are approximate: the Classic's spread and the Guardian/Operator run-spread values.

**Spread model (`session.rs`, used only when `simulate_spread = true`):** spread = first-shot spread + run spread × (how far the speed is between the accuracy threshold and the run speed). This is a linear approximation. It shows why stopping before you shoot matters, but it does not reproduce Riot's internal curve.
