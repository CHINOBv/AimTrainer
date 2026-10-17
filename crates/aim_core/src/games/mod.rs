//! Per-game data: VALORANT's mechanics ([`valorant`]) and the sensitivity, FOV and zoom
//! profile of every game the converter supports ([`Game`]). Each value has a source in
//! `docs/valorant-mechanics.md` or `docs/research/sensitivity-games.md`.

mod profiles;
pub mod valorant;

pub use profiles::{FovAxis, FovModel, Game, GameProfile};
