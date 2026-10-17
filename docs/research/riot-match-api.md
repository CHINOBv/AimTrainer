# Riot VAL-MATCH-V1 for match analysis

Researched 2026-10-17.

**Sources:**

- https://developer.riotgames.com/docs/valorant
- https://www.riotgames.com/en/DevRel/valorant-api-launch
- https://developer.riotgames.com/policies/general
- Community OpenAPI schema: https://www.mingweisamuel.com/riotapi-schema/openapi-3.0.0.json

## Access (verified)

- Match endpoints need a **production key**. Personal keys and development keys get no VALORANT access.
- Each player must opt in through **RSO**.
- Apps for personal use only are not approved.
- Endpoints:
  - `GET /val/match/v1/matchlists/by-puuid/{puuid}`
  - `GET /val/match/v1/matches/{matchId}`
  - `GET /riot/account/v1/accounts/by-riot-id/{name}/{tag}`
- There is no official, key-free way to export match data.

## What the data contains (verified)

- `roundResults[].playerStats[]` holds:
  - `kills[]`: `timeSinceRoundStartMillis`, `killer`, `victim`, `victimLocation {x,y}`, `playerLocations[] {puuid, viewRadians, location}`, `finishingDamage {damageType, damageItem}`;
  - `damage[]`: `receiver`, `damage`, `legshots`, `bodyshots`, `headshots`;
  - `economy`.
- The data has **no shots fired, no misses, no height and no crosshair data**.

## Policy (verified)

- Products must not create an unfair advantage. Real-time in-game data apps are not approved.
- The required notice: "[Product] isn't endorsed by Riot Games and doesn't reflect the views or opinions of Riot Games or anyone officially involved in producing or managing Riot Games properties. Riot Games, and all associated properties are trademarks or registered trademarks of Riot Games, Inc."

## Design consequence

- Offline import of a `MatchDto` JSON the player provides; post-match only, never live.
- Online fetch is possible only for a registered production product with RSO, so it is not shipped.
- Analysis is limited to:
  - headshot rate from hit counts;
  - kill and death distances from 2D locations (engine units, assumed to be cm);
  - opening duels;
  - weapon use.
