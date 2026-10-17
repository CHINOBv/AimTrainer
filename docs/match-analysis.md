# Match analysis (offline import)

The `aim_integrations` crate reads one finished VALORANT match and suggests drills from the trainer's built-in scenarios. It covers roadmap Phase 8. The data facts come from [research/riot-match-api.md](research/riot-match-api.md).

## What it does

1. It parses a Riot VAL-MATCH-V1 `MatchDto` JSON file that the player provides.
2. It selects one player by PUUID or by Riot ID (`gameName#tagLine`, case-insensitive).
3. It computes that player's numbers for the match (see [What it measures](#what-it-measures)).
4. It applies a small set of explainable rules that map findings to drills (see [Rules](#rules)).
5. It renders a plain-text report with `MatchReport::to_text`.

```rust
let report = aim_integrations::analyze_json(&json, "Ace#EUW")?;
println!("{}", report.to_text());
```

The public entry points are `parse_match`, `analyze(&MatchDto, &PlayerSelector)`, `analyze_json(&str, &str)` and `recommend(&Analysis)`. They return `Error::Json`, `Error::InvalidSelector` or `Error::PlayerNotFound`; malformed input never panics.

## Integrity

- **Post-match and offline only.** The crate makes **no network calls**, ships **no API key** and never reads, hooks or captures the game. It only analyzes a file you give it after the match ended.
- **Nothing real-time.** Riot does not approve apps that use in-game data live, and this project never will (see `docs/coding-standards.md`).

## How to get the match JSON

The official source is `GET /val/match/v1/matches/{matchId}`, with the match ID from `GET /val/match/v1/matchlists/by-puuid/{puuid}`.

- These endpoints need a Riot **production key**. Personal and development keys get no VALORANT match access.
- Each player must opt in through **Riot Sign On (RSO)**.
- There is no official, key-free way to export match data.

So the trainer cannot fetch matches itself. You need the `MatchDto` JSON from an app registered with Riot that you have authorized through RSO, or from your own registered production app. Save the response as a file and import it.

## Riot notice

Riot requires this notice in third-party products:

> CustomAimTrainer isn't endorsed by Riot Games and doesn't reflect the views or opinions of Riot Games or anyone officially involved in producing or managing Riot Games properties. Riot Games, and all associated properties are trademarks or registered trademarks of Riot Games, Inc.

The text report ends with it (`RIOT_NOTICE`).

## What it measures

| Number | How | Confidence |
|---|---|---|
| K/D/A | `players[].stats` | Exact |
| Headshot rate of hits | `headshots / (headshots + bodyshots + legshots)` summed over the player's `damage[]` | Exact for hits |
| Kill and death distance | 2D distance from the killer's `playerLocations[].location` to `victimLocation`, divided by 100 | **Approximate**: the unit is not documented; centimeters are assumed (Unreal Engine) |
| Range buckets | Close < 10 m, mid 10–25 m inclusive, long > 25 m; unknown when a position is missing | Approximate (inherits the distance) |
| Opening duels | The earliest kill of each round: won if the player killed, lost if the player died | Exact |
| Damage sources | Kills counted by `finishingDamage.damageType` and `damageItem` | Exact; items are Riot ids, not names |
| Time to first kill | Mean of `timeSinceRoundStartMillis` of the player's first kill, over rounds with a kill | Exact |

## Limits of the data

- **No shots fired and no misses.** The headshot rate is a share of hits, **not accuracy**. The report never claims accuracy.
- **No height.** Locations are 2D, so distances ignore elevation.
- **No crosshair data.** `viewRadians` is a facing angle at the moment of a kill, not crosshair placement.
- **Units are assumed.** Distances use 100 engine units per meter.
- **One match is a small sample.** Each rule needs a minimum count before it fires.

## Rules

All thresholds are **provisional**: they are judgment calls, not fitted to player data. A value equal to the threshold does not fire. Rules run in the order below.

| Rule | Metric | Fires when | Minimum sample | Drills | Reason |
|---|---|---|---|---|---|
| `low-headshot-rate` | headshots / hits | < 0.20 | 10 hits | `static-headline`, `flick-headshot` | Most hits land below the head: train head-height placement and flicks to heads |
| `long-range-deaths` | long deaths / deaths with known distance | > 0.50 | 3 known deaths | `long-range-heads` | Losing fights at distance: train small, precise targets far away |
| `opening-duels-lost` | opening duels lost / opening duels | > 0.60 | 3 duels | `peek-reaction`, `counter-strafe` | Losing the first contact: train reacting to peeks and stopping before shooting |
| `close-range-deaths` | close deaths / deaths with known distance | > 0.50 | 3 known deaths | `track-reactive-strafe`, `dynamic-strafe-heads` | Losing close fights: train tracking and heads on fast strafing targets |

A test checks that every drill id is a built-in scenario.
