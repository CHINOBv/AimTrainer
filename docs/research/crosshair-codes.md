# VALORANT crosshair profile codes

Researched 2026-10-17. Riot introduced code import/export in patch 4.05 (https://playvalorant.com/en-us/news/game-updates/valorant-patch-notes-4-05/), but has never documented the fields. This spec comes from three independent open-source parsers that agree with each other:

- `@valapi/crosshair`: https://github.com/valapi/node-valapi/blob/master/packages/@valapi/crosshair/src/index.ts
- ValTrainer: https://github.com/idoraz1/ValTrainer/blob/main/ValTrainerGodot/src/Valorant/CrosshairCode.cs
- beautiful-wallpaper: https://github.com/C0uki/beautiful-wallpaper/blob/main/crates/bw-core/src/crosshair.rs

## Grammar (verified)

- A code is a flat list of tokens separated by `;`.
- The bare uppercase tokens `P`, `A` and `S` switch the section: Primary, ADS and Sniper.
- Everything else comes in `key;value` pairs. Keys are lowercase.
- The leading `0` opens the General section. In it, `p` = ADS copies the primary crosshair (default 1), `c` = override all primary crosshairs (default 0), and `s` = advanced options (default 0).
- Only values that differ from the defaults are written, so a decoder must start from the defaults.
- Decimals always use `.`. Exports use at most 3 decimals.
- Parsers skip unknown keys in pairs and tolerate a truncated final key.

## Primary and ADS keys (verified defaults)

`f` and `s` exist only in the Primary section.

| Key | Meaning | Default | Range |
|---|---|---|---|
| `c` | color index (8 = custom) | 0 | 0–8 |
| `u` | custom color `RRGGBBAA` | `FFFFFFFF` | hex |
| `h` | outlines on | 1 | bool |
| `t` | outline thickness | 1 | 1–6 |
| `o` | outline opacity | 0.5 | 0–1 |
| `d` | center dot on | 0 | bool |
| `z` | dot thickness | 2 | 1–6 |
| `a` | dot opacity | 1 | 0–1 |
| `f` | fade with firing error | 1 | bool |
| `s` | show spectated player's crosshair | 1 | bool |
| `m` | override firing-error offset with crosshair offset | 0 | bool |

Line keys use prefix `0` for inner lines and `1` for outer lines.

| Key | Meaning | Inner default (range) | Outer default (range) |
|---|---|---|---|
| `b` | show lines | 1 | 1 |
| `t` | thickness | 2 (0–10) | 2 (0–10) |
| `l` | length | 6 (0–20) | 2 (0–10) |
| `g` | separate vertical length | 0 | 0 |
| `v` | vertical length (only when `g` = 1) | 6 (0–20) | 2 (0–10) |
| `o` | offset (gap) | 3 (0–20) | 10 (0–40) |
| `a` | opacity | 0.8 (0–1) | 0.35 (0–1) |
| `m` | movement error | 0 | 1 |
| `s` | movement error multiplier | 1 (0–3) | 1 (0–3) |
| `f` | firing error | 1 | 1 |
| `e` | firing error multiplier | 1 (0–3) | 1 (0–3) |

Sniper section `S`:

| Key | Meaning | Default |
|---|---|---|
| `d` | center dot | 1 |
| `b` | custom color flag | 0 |
| `c` | color index | 7 |
| `t` | custom hex | — |
| `s` | dot thickness | 1 (0–4) |
| `o` | dot opacity | 0.75 |

## Colors (verified: all parsers agree)

| Index | Name | RGB |
|---|---|---|
| 0 | white | `FFFFFF` |
| 1 | green | `00FF00` |
| 2 | yellow-green | `7FFF00` |
| 3 | green-yellow | `DFFF00` |
| 4 | yellow | `FFFF00` |
| 5 | cyan | `00FFFF` |
| 6 | pink | `FF00FF` |
| 7 | red | `FF0000` |
| 8 | custom | from `u` (alpha last) |

When both `c` and a stale `u` are present, the preset index wins.

## Units (uncertain)

- Lengths, thicknesses and offsets are integer screen pixels. No reference resolution is documented.

## Test vectors

1. `0;P;c;5;h;0;f;0;0l;4;0o;2;0a;1;0f;0;1b;0`
   - cyan, outlines off, fade off;
   - inner length 4, offset 2, opacity 1, firing error off;
   - outer lines off.
2. `0;p;0;s;1;P;c;5;h;0;0l;4;0o;2;0a;1;1b;0;A;c;7;u;EF92BFFF;o;1;d;1;f;0;s;0;0b;0;1b;0;S;o;1`
   - This is a real export.
3. `0;P;d;1;0l;10;0o;2;1b;0`
   - the default crosshair, plus a center dot;
   - inner length 10, offset 2;
   - outer lines off.
