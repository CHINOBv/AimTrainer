# Vandal / Phantom recoil and spread

Researched 2026-10-17.

**Sources:**

- Weapon pages: https://wiki.playvalorant.com/en-us/Vandal , https://wiki.playvalorant.com/en-us/Phantom
- Patch notes 0.50: https://playvalorant.com/en-us/news/game-updates/valorant-patch-notes-0-50/
- Patch notes 11.08: https://playvalorant.com/en-us/news/game-updates/valorant-patch-notes-11-08/

## Verified parameters

| | Vandal | Phantom |
|---|---|---|
| First-shot spread, standing | 0.25° | 0.20° |
| Max spread, standing | 1.0° | 0.9° |
| Crouch spread multiplier | ×0.85 | ×0.85 |
| Protected bullets (11.08) | 6 | 8 |
| Yaw switch chance per bullet | 10 % | 10 % |
| Yaw switch time | 0.6 s | 0.6 s |
| Recovery time (0.50) | 0.375 s | 0.35 s |
| Tap efficiency | 6 | 4 |
| Fire rate | 9.75 rps | 11 rps |
| Magazine | 25 | 30 |
| Damage | 160/40/34 (0–50 m) | 156/39/33 (0–20 m), 140/35/29 (20–50 m) |

## Not published

- Recoil per bullet in degrees.
- Spread growth per bullet.
- How much of the climb moves the camera versus only the bullets. In ADS the crosshair follows recoil.

## Trainer model (estimate, tagged as such in the app)

1. **Climb:** vertical climb over the protected bullets, then horizontal sway to a random side, with a 10 % chance per bullet to switch and a 0.6 s ease.
2. **Spread:** grows linearly from first-shot spread to max spread over 10 bullets.
3. **Reset:** full reset after the recovery time.
4. **Burst rule:** a burst is any run of shots that fits inside the protected bullets. A full first-shot reset requires waiting the recovery time.
