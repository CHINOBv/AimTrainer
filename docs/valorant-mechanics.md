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
| Overwatch 2 yaw: 0.0066; FOV horizontal at 16:9, default 103 | `Game::Overwatch2` | verified | https://github.com/KovaaK/SensitivityMatcher , https://github.com/sillynov/autosens/blob/HEAD/autosens/games.json |
| CS2 FOV: 90 horizontal at 4:3 with the vertical locked (106.26° at 16:9), fixed | `Game::Cs2.fov_model()` | verified | https://github.com/KovaaK/SensitivityMatcher , autosens `games.json` |
| Apex FOV: the in-game slider is horizontal at 4:3 (`cl_fovScale` × 70), default 90 | `Game::Apex.fov_model()` | verified | https://github.com/KovaaK/SensitivityMatcher , autosens `games.json` |
| Fortnite yaw: 0.005555 per 1 % of the X/Y slider | `Game::Fortnite` | verified | https://github.com/KovaaK/SensitivityMatcher , autosens `games.json` |
| Fortnite FOV: ≈80° horizontal at 16:9, fixed | `Game::Fortnite.fov_model()` | estimate (sources conflict) | autosens `games.json`; see `docs/research/sensitivity-games.md` |
| Call of Duty (MW2019–BO6) yaw: 0.0066 | `Game::CallOfDuty` | verified | https://github.com/KovaaK/SensitivityMatcher , autosens `games.json` |
| Call of Duty FOV: horizontal at 16:9, default 80 | `Game::CallOfDuty.fov_model()` | approximate | autosens `games.json` |
| Destiny 2 yaw: 0.0066; FOV horizontal at 16:9, default 85 | `Game::Destiny2` | approximate | autosens `games.json`, mouse-sensitivity.com forum |
| Rainbow Six Siege yaw: 0.00572958 per slider unit with the default `MouseSensitivityMultiplierUnit` 0.02 (general: slider × multiplier / 0.02 × 0.00572958) | `Game::RainbowSixSiege` | verified (hip-fire) | https://github.com/KovaaK/SensitivityMatcher |
| Rainbow Six Siege FOV: vertical; default 60 (range 60–90) | `Game::RainbowSixSiege.fov_model()` | approximate | autosens `games.json` |
| Deadlock yaw: 0.044; no FOV model (360° matching only) | `Game::Deadlock` | approximate | autosens `games.json` |
| Monitor distance p %: °/count scales by atan(p·tan(FOV_to/2)) / atan(p·tan(FOV_from/2)); at 0 % by tan(FOV_to/2) / tan(FOV_from/2); both FOVs on the same axis | `sensitivity::matching::convert`, `monitor_distance_ratio` | verified | https://www.mouse-sensitivity.com/forums/topic/5592-does-0-mm-have-its-own-formula/ , https://tomhepz.com/post/monitor-match-math/ |
| VALORANT ADS multiplier for p % = ratio(103°, 103°/zoom, p) × zoom (0 % at zoom 1.25 = 0.870439; 100 % = 1.0) | `sensitivity::matching::valorant_ads_multiplier` | approximate (zoomed FOV = 103°/zoom is a converter model) | same as above |
| A scoped/ADS multiplier of 1.0 is a 100% horizontal monitor match, so zoomed °/count = base × mult / zoom | `Sensitivity::zoomed_deg_per_count` | approximate | https://game-sens.jor.dev/ , https://edpi-calculator.org/tools/valorant-scoped-sensitivity |

## Field of view

| Value | Confidence | Source |
|---|---|---|
| Vertical FOV is locked at ≈70.53°, which gives 103° horizontal at 16:9 (Hor+) | verified | https://www.mouse-sensitivity.com/forums/topic/9354-about-wrong-fov-about-valorant-in-43/ |
| Horizontal FOV per aspect ratio: 4:3 → 86.63°, 16:10 → 97.05° | verified (follows from the formula) | https://fovcalculatorpro.com/games/valorant-fov/ |
| Ultrawide (wider than 16:9) is pillarboxed to 16:9, so the horizontal FOV stays 103°; the trainer renders a centered 16:9 viewport | approximate (one forum thread claims a wider FOV) | https://www.evetech.co.za/what-is-ultrawide-21-9-vs-16-9-gaming/e/3723 , https://www.change.org/p/ultrawide-fix-in-valorant , contra: https://www.mouse-sensitivity.com/forums/topic/8438-valorant-locked-fov/ |
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
