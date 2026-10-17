//! Arena activities: bots on an original map, driven by the same
//! [`aim_core::scenario::FrameInput`] as the scenario session.
//!
//! An arena ([`ArenaDef`], loaded from TOML) names a map, the player's weapon and a mode:
//! deathmatch against roaming bots, or clearing a list of angle markers in order. The
//! session is pure and deterministic for a given definition, map, seed and input sequence.
//! See `docs/maps.md` for the format, the bot model and the simplifications.

mod aim;
mod bot;
pub mod builtin;
mod def;
mod session;
#[cfg(test)]
mod tests;

pub use bot::{BOT_HEALTH, BOT_NOTICE_HALF_ANGLE_DEG, PEEK_DELAY_SECS, ROAM_PAUSE_SECS};
pub use def::{ArenaDef, ArenaError, ArenaMode, MAX_BOTS, bot_profile};
pub use session::{
    ArenaEvent, ArenaSession, ArenaStats, BOT_WEAPON, BotView, DEATH_POINTS, HEADSHOT_BONUS,
    KILL_POINTS, PLAYER_HEALTH, RESPAWN_GRACE_SECS,
};
