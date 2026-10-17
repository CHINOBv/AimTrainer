//! Choosing which player in a match to analyze.

use crate::Error;
use crate::dto::{MatchDto, PlayerDto};

/// Identifies one player in a match.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlayerSelector {
    /// The player's PUUID, compared exactly.
    Puuid(String),
    /// A Riot ID `gameName#tagLine`, compared case-insensitively like Riot does.
    RiotId { game_name: String, tag_line: String },
}

impl PlayerSelector {
    /// Parses `gameName#tagLine` as a Riot ID and anything else as a PUUID.
    /// The last `#` separates the tag, so a name may itself contain `#`.
    pub fn parse(text: &str) -> Result<Self, Error> {
        let text = text.trim();
        if text.is_empty() {
            return Err(Error::InvalidSelector(text.to_owned()));
        }
        match text.rsplit_once('#') {
            Some((name, tag)) => {
                let (name, tag) = (name.trim(), tag.trim());
                if name.is_empty() || tag.is_empty() {
                    return Err(Error::InvalidSelector(text.to_owned()));
                }
                Ok(Self::RiotId {
                    game_name: name.to_owned(),
                    tag_line: tag.to_owned(),
                })
            }
            None => Ok(Self::Puuid(text.to_owned())),
        }
    }

    fn matches(&self, player: &PlayerDto) -> bool {
        match self {
            Self::Puuid(puuid) => player.puuid == *puuid,
            Self::RiotId {
                game_name,
                tag_line,
            } => {
                player.game_name.to_lowercase() == game_name.to_lowercase()
                    && player.tag_line.to_lowercase() == tag_line.to_lowercase()
            }
        }
    }

    pub(crate) fn find<'a>(&self, match_dto: &'a MatchDto) -> Result<&'a PlayerDto, Error> {
        match_dto
            .players
            .iter()
            .find(|p| self.matches(p))
            .ok_or_else(|| Error::PlayerNotFound(self.to_string()))
    }
}

impl std::fmt::Display for PlayerSelector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Puuid(puuid) => write!(f, "{puuid}"),
            Self::RiotId {
                game_name,
                tag_line,
            } => write!(f, "{game_name}#{tag_line}"),
        }
    }
}
