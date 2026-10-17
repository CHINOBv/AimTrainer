# Sensitivity and FOV data for the converter

Researched 2026-10-17.

**Confidence tags:**

- **V**: verified (official source, game files or exact measurement).
- **A**: approximate (converters agree).
- **U**: uncertain (conflicting or non-linear).

**Sources:**

- KovaaK's SensitivityMatcher: https://github.com/KovaaK/SensitivityMatcher
- autosens: https://github.com/sillynov/autosens/blob/HEAD/autosens/games.json
- mouse-sensitivity.com forum posts by DPI Wizard, linked per row.

| Game | Degrees/count at sens 1 | FOV setting | Default | Conf. |
|---|---|---|---|---|
| VALORANT | 0.07 | fixed 103° horizontal at 16:9 (Hor+ below 16:9, pillarboxed above) | 103 | V |
| CS2 | 0.022 | 4:3 horizontal, vertical locked (90 at 4:3 = 106.26° at 16:9) | 90 | V |
| Apex Legends | 0.022 | 4:3 horizontal, `cl_fovScale` × 70 | 90 | V |
| Overwatch 2 | 0.0066 | horizontal at 16:9 | 103 | V |
| Fortnite | 0.5555 per slider unit (%) → 0.005555 per 1 % | ~80° horizontal | 80 | V / U for FOV |
| Call of Duty (MW2019–BO6) | 0.0066 | horizontal at 16:9 | 80 | V / A for FOV |
| Destiny 2 | 0.0066 | horizontal at 16:9 | 85 | A |
| Rainbow Six Siege | slider × (multiplier / 0.02) × 0.00572958 | vertical | 60–90 | V hip-fire |
| Deadlock | 0.044 | 75–90 | — | A |

These games are deliberately **not** in the converter, because their constants conflict between sources or their scaling is non-linear: The Finals, Marvel Rivals, Halo Infinite, Escape from Tarkov, PUBG and Battlefield.

## Monitor distance matching (V)

Source: https://www.mouse-sensitivity.com/forums/topic/5592-does-0-mm-have-its-own-formula/ and https://tomhepz.com/post/monitor-match-math/

- **MD p %:**
  `counts_out = counts_in · atan(p·tan(FOV_in/2)) / atan(p·tan(FOV_out/2))`
  Both FOVs must be horizontal (MDH) or both vertical (MDV).
- **0 % (focal length):**
  `counts_out = counts_in · tan(FOV_in/2) / tan(FOV_out/2)`
- **100 % horizontal:** reduces to `FOV_out / FOV_in`.
- **Converting between bases:** MDV p % = MDH (p / aspect) %. For example, MDV 177.78 % = MDH 100 % at 16:9.
- **VALORANT ADS multiplier:**
  - 1.0 = MDH 100 % at 16:9.
  - The 0 % multiplier is `tan((103/zoom)/2) / tan(51.5°) · zoom`. For the Vandal (zoom 1.25) this gives 0.870439.
- **Worked example:** CS2 sens 1 at 800 DPI (106.26° horizontal) converts to VALORANT (103°) as follows:

| Matching method | VALORANT sens |
|---|---|
| 360° distance | 0.3143 |
| MDH 0 % | 0.2963 |
| MDH 100 % | 0.3046 |

## Implementation notes

Implemented in `aim_core::sensitivity` (`Game::profile()` and `matching`). Where the table above leaves a gap, the code makes these assumptions:

- **Rainbow Six Siege:** the default FOV is taken as 60 (the low end of the 60–90 range). The yaw assumes the default `MouseSensitivityMultiplierUnit` of 0.02.
- **Fortnite:** the ~80° FOV is treated as horizontal at 16:9 and fixed. Monitor-distance results for Fortnite are therefore estimates.
- **Deadlock:** no FOV model. The converter offers only 360° matching for it.
- **All games** are modelled as Hor+ (vertical FOV locked). Only VALORANT is pillarboxed above 16:9.
