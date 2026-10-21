//! Verifiable score proofs: a share code that lets a friend re-simulate a run and confirm its
//! score, with no server.
//!
//! `Session` is deterministic, so a run's seed and input log reproduce its score exactly. A
//! [`ScoreProof`] bundles the replay ([`codec`] bytes: the scenario id, seed and inputs)
//! with a [`content_hash`] of the scenario definition that was played and the claimed score
//! and accuracy. [`verify`] rebuilds that definition from the verifier's own library, checks
//! the hash (an edited scenario is refused), replays the inputs and compares the result bit
//! for bit.
//!
//! Share code: base64url (no padding) of
//!
//! ```text
//! magic     4 bytes  "AIMP"
//! version   u16      1
//! hash      u64      content_hash of the played definition
//! score     f32      claimed score
//! accuracy  f32      claimed accuracy
//! replay    rest     analytics::codec replay bytes
//! ```
//!
//! All numbers are little-endian. A proof shows that the inputs produce the score; it cannot
//! show that a human produced the inputs.

use super::codec::{self, CodecError};
use crate::scenario::{Recording, RunKey, RunResult, ScenarioDef, Session};

/// Four-byte signature at the start of every proof.
pub const MAGIC: [u8; 4] = *b"AIMP";
/// Current proof format version.
pub const VERSION: u16 = 1;
const HEADER_LEN: usize = 4 + 2 + 8 + 4 + 4;

/// A run bundled for verification.
#[derive(Debug, Clone, PartialEq)]
pub struct ScoreProof {
    /// The run: the definition played (its id is the run key), seed and inputs. Verification
    /// uses only the id, seed and inputs; the definition comes from the verifier's library.
    pub recording: Recording,
    /// [`content_hash`] of the definition that was played.
    pub content_hash: u64,
    /// Score the run claims.
    pub claimed_score: f32,
    /// Accuracy the run claims.
    pub claimed_accuracy: f32,
}

/// A proof that replayed to its claimed result.
#[derive(Debug, Clone, PartialEq)]
pub struct Verified {
    /// Run key of the verified run.
    pub scenario_id: String,
    /// The replayed result, equal to the claim.
    pub result: RunResult,
}

/// Why a proof could not be read or did not verify.
#[derive(Debug, PartialEq, thiserror::Error)]
pub enum ProofError {
    /// The text is not a share code (bad characters or no proof signature).
    #[error("not a score proof code")]
    BadCode,
    /// The proof format version is not [`VERSION`]; carries the version found.
    #[error("unsupported proof version {0}")]
    UnsupportedVersion(u16),
    /// The embedded replay could not be encoded or decoded.
    #[error("proof replay is invalid: {0}")]
    Replay(CodecError),
    /// The scenario definition could not be serialized for hashing; carries the reason.
    #[error("scenario cannot be hashed: {0}")]
    Scenario(String),
    /// No scenario in the library has the proof's base id; carries the run key.
    #[error("unknown scenario {0}")]
    UnknownScenario(String),
    /// The library's definition differs from the one played (the scenario was edited).
    #[error(
        "scenario {scenario_id} differs from the one played (hash {found:016x}, proof {expected:016x})"
    )]
    HashMismatch {
        /// Run key of the scenario.
        scenario_id: String,
        /// Hash carried by the proof.
        expected: u64,
        /// Hash of the library's definition.
        found: u64,
    },
    /// Replaying the inputs gave another score or accuracy than claimed.
    #[error(
        "claimed {claimed_score} at {claimed_accuracy}, replay gives {replayed_score} at {replayed_accuracy}"
    )]
    ScoreMismatch {
        /// Score the proof claims.
        claimed_score: f32,
        /// Score of the replay.
        replayed_score: f32,
        /// Accuracy the proof claims.
        claimed_accuracy: f32,
        /// Accuracy of the replay.
        replayed_accuracy: f32,
    },
}

/// Stable 64-bit FNV-1a hash of a definition's canonical TOML. Definitions hold no maps, so
/// field order is fixed and the text is canonical for a given build. Not cryptographic.
pub fn content_hash(def: &ScenarioDef) -> Result<u64, ProofError> {
    let src = toml::to_string(&*def.hashed()).map_err(|e| ProofError::Scenario(e.to_string()))?;
    Ok(fnv1a_64(src.as_bytes()))
}

fn fnv1a_64(bytes: &[u8]) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0100_0000_01b3;
    bytes
        .iter()
        .fold(OFFSET, |h, b| (h ^ u64::from(*b)).wrapping_mul(PRIME))
}

impl ScoreProof {
    /// A proof of `recording` claiming `result` (the result of playing it).
    pub fn new(recording: Recording, result: &RunResult) -> Result<Self, ProofError> {
        Ok(Self {
            content_hash: content_hash(&recording.def)?,
            claimed_score: result.score,
            claimed_accuracy: result.accuracy,
            recording,
        })
    }

    /// Run key of the scenario played.
    pub fn scenario_id(&self) -> &str {
        &self.recording.def.id
    }

    /// Seed of the run.
    pub fn seed(&self) -> u64 {
        self.recording.seed
    }

    /// The share code.
    pub fn encode(&self) -> Result<String, ProofError> {
        let replay = codec::encode(&self.recording).map_err(ProofError::Replay)?;
        let mut bytes = Vec::with_capacity(HEADER_LEN + replay.len());
        bytes.extend_from_slice(&MAGIC);
        bytes.extend_from_slice(&VERSION.to_le_bytes());
        bytes.extend_from_slice(&self.content_hash.to_le_bytes());
        bytes.extend_from_slice(&self.claimed_score.to_le_bytes());
        bytes.extend_from_slice(&self.claimed_accuracy.to_le_bytes());
        bytes.extend_from_slice(&replay);
        Ok(base64url::encode(&bytes))
    }

    /// Reads a share code made by [`ScoreProof::encode`]. Surrounding whitespace is ignored.
    pub fn decode(code: &str) -> Result<Self, ProofError> {
        let bytes = base64url::decode(code.trim()).ok_or(ProofError::BadCode)?;
        if bytes.len() < HEADER_LEN || bytes[..4] != MAGIC {
            return Err(ProofError::BadCode);
        }
        let le = |at: usize, n: usize| &bytes[at..at + n];
        let version = u16::from_le_bytes(le(4, 2).try_into().expect("2 bytes"));
        if version != VERSION {
            return Err(ProofError::UnsupportedVersion(version));
        }
        Ok(Self {
            content_hash: u64::from_le_bytes(le(6, 8).try_into().expect("8 bytes")),
            claimed_score: f32::from_le_bytes(le(14, 4).try_into().expect("4 bytes")),
            claimed_accuracy: f32::from_le_bytes(le(18, 4).try_into().expect("4 bytes")),
            recording: codec::decode(&bytes[HEADER_LEN..]).map_err(ProofError::Replay)?,
        })
    }
}

/// The library scenario rebuilt as the variant a run key names: difficulty, duration,
/// tuning and floor height, applied in run-key order so the id matches the key.
pub fn variant_for_key(library: &[ScenarioDef], key: &str) -> Option<ScenarioDef> {
    let k = RunKey::parse(key);
    let base = library.iter().find(|d| d.id == k.base)?;
    let mut def = base.at_difficulty(k.level);
    if let Some(secs) = k.secs {
        def = def.with_duration(secs);
    }
    def = def.with_tuning(k.tuning);
    if let Some(m) = k.elevation {
        def = def.with_elevation(m);
    }
    Some(def)
}

/// Re-simulate `proof` against `library` and confirm its claimed score and accuracy.
pub fn verify(proof: &ScoreProof, library: &[ScenarioDef]) -> Result<Verified, ProofError> {
    let id = proof.scenario_id();
    let def =
        variant_for_key(library, id).ok_or_else(|| ProofError::UnknownScenario(id.to_owned()))?;
    let found = content_hash(&def)?;
    if found != proof.content_hash {
        return Err(ProofError::HashMismatch {
            scenario_id: id.to_owned(),
            expected: proof.content_hash,
            found,
        });
    }
    let mut session = Session::new(def, proof.seed());
    for input in &proof.recording.inputs {
        session.update(*input);
    }
    let result = session.result();
    let same = |a: f32, b: f32| a.to_bits() == b.to_bits();
    if !same(result.score, proof.claimed_score) || !same(result.accuracy, proof.claimed_accuracy) {
        return Err(ProofError::ScoreMismatch {
            claimed_score: proof.claimed_score,
            replayed_score: result.score,
            claimed_accuracy: proof.claimed_accuracy,
            replayed_accuracy: result.accuracy,
        });
    }
    Ok(Verified {
        scenario_id: id.to_owned(),
        result,
    })
}

/// URL-safe base64 without padding (RFC 4648 §5), so codes survive chat apps and URLs.
mod base64url {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

    pub fn encode(bytes: &[u8]) -> String {
        let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
        for chunk in bytes.chunks(3) {
            let n = chunk
                .iter()
                .enumerate()
                .fold(0u32, |n, (i, b)| n | u32::from(*b) << (16 - 8 * i));
            for i in 0..=chunk.len() {
                out.push(ALPHABET[(n >> (18 - 6 * i) & 63) as usize] as char);
            }
        }
        out
    }

    fn value(c: u8) -> Option<u32> {
        ALPHABET.iter().position(|a| *a == c).map(|p| p as u32)
    }

    /// `None` on a character outside the alphabet or an impossible length.
    pub fn decode(text: &str) -> Option<Vec<u8>> {
        let text = text.as_bytes();
        if text.len() % 4 == 1 {
            return None;
        }
        let mut out = Vec::with_capacity(text.len() / 4 * 3 + 2);
        for chunk in text.chunks(4) {
            let mut n = 0u32;
            for (i, c) in chunk.iter().enumerate() {
                n |= value(*c)? << (18 - 6 * i);
            }
            for i in 0..chunk.len() - 1 {
                out.push((n >> (16 - 8 * i)) as u8);
            }
        }
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analytics::test_support::{record, record_def};
    use crate::scenario::builtin;
    use crate::sim::AimerProfile;

    fn proof_of(rec: Recording) -> ScoreProof {
        let result = rec.replay(|_, _| {}).result();
        ScoreProof::new(rec, &result).unwrap()
    }

    #[test]
    fn fnv1a_matches_the_reference_vectors() {
        assert_eq!(fnv1a_64(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a_64(b"a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(fnv1a_64(b"foobar"), 0x8594_4171_f739_67e8);
    }

    #[test]
    fn base64url_round_trips_every_length_and_rejects_bad_text() {
        assert_eq!(base64url::encode(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64url::encode(b"fo"), "Zm8");
        assert_eq!(base64url::encode(&[0xfb, 0xff]), "-_8");
        for n in 0..10u8 {
            let bytes: Vec<u8> = (0..n).map(|i| i.wrapping_mul(97)).collect();
            assert_eq!(base64url::decode(&base64url::encode(&bytes)), Some(bytes));
        }
        assert_eq!(base64url::decode("Zm9v+mFy"), None);
        assert_eq!(base64url::decode("Zm9vY"), None);
    }

    #[test]
    fn a_recorded_run_round_trips_through_its_share_code_and_verifies() {
        let rec = record("micro-adjust", AimerProfile::PRESETS[1], 11);
        let proof = proof_of(rec.clone());
        assert_eq!(proof.scenario_id(), "micro-adjust");
        assert_eq!(proof.seed(), 11);
        let code = proof.encode().unwrap();
        assert!(
            code.bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
        );
        let back = ScoreProof::decode(&format!("  {code}\n")).unwrap();
        assert_eq!(back, proof);
        let ok = verify(&back, &builtin::all()).unwrap();
        assert_eq!(ok.scenario_id, "micro-adjust");
        assert_eq!(ok.result, rec.replay(|_, _| {}).result());
        assert!(ok.result.score > 0.0, "{:?}", ok.result);
    }

    #[test]
    fn a_run_at_another_difficulty_and_length_verifies_against_the_base_scenario() {
        let lib = builtin::all();
        let base = lib.iter().find(|d| d.id == "track-precise").unwrap();
        let def = base.at_difficulty(0.25).with_duration(20.0);
        let rec = record_def(def, AimerProfile::PRESETS[2], 3);
        let proof = proof_of(rec);
        assert_eq!(proof.scenario_id(), "track-precise@+0.25~20s");
        assert!(verify(&proof, &lib).is_ok());
    }

    #[test]
    fn an_unknown_scenario_is_refused() {
        let proof = proof_of(record("micro-adjust", AimerProfile::PRESETS[1], 2));
        let lib: Vec<_> = builtin::all()
            .into_iter()
            .filter(|d| d.id != "micro-adjust")
            .collect();
        assert_eq!(
            verify(&proof, &lib),
            Err(ProofError::UnknownScenario("micro-adjust".to_owned()))
        );
    }

    #[test]
    fn an_edited_scenario_is_refused_by_its_hash() {
        let proof = proof_of(record("micro-adjust", AimerProfile::PRESETS[1], 2));
        let mut lib = builtin::all();
        let edited = lib.iter_mut().find(|d| d.id == "micro-adjust").unwrap();
        edited.duration_secs += 1.0;
        let found = content_hash(edited).unwrap();
        assert_eq!(
            verify(&proof, &lib),
            Err(ProofError::HashMismatch {
                scenario_id: "micro-adjust".to_owned(),
                expected: proof.content_hash,
                found,
            })
        );
    }

    #[test]
    fn a_claim_the_replay_does_not_reproduce_is_refused() {
        let mut proof = proof_of(record("micro-adjust", AimerProfile::PRESETS[1], 2));
        let real = proof.claimed_score;
        proof.claimed_score += 1.0;
        let err = verify(&proof, &builtin::all()).unwrap_err();
        assert_eq!(
            err,
            ProofError::ScoreMismatch {
                claimed_score: real + 1.0,
                replayed_score: real,
                claimed_accuracy: proof.claimed_accuracy,
                replayed_accuracy: proof.claimed_accuracy,
            }
        );
        // Tampered inputs are caught the same way.
        let mut proof = proof_of(record("micro-adjust", AimerProfile::PRESETS[1], 2));
        proof
            .recording
            .inputs
            .truncate(proof.recording.inputs.len() / 2);
        assert!(matches!(
            verify(&proof, &builtin::all()),
            Err(ProofError::ScoreMismatch { .. })
        ));
    }

    #[test]
    fn corrupt_codes_are_rejected() {
        let proof = proof_of(record("micro-adjust", AimerProfile::PRESETS[1], 2));
        let code = proof.encode().unwrap();
        assert_eq!(ScoreProof::decode("not a code!"), Err(ProofError::BadCode));
        assert_eq!(
            ScoreProof::decode(&base64url::encode(b"AIMR\x01\x00 replay, not a proof....")),
            Err(ProofError::BadCode)
        );
        let mut bytes = base64url::decode(&code).unwrap();
        bytes[4] = 9;
        assert_eq!(
            ScoreProof::decode(&base64url::encode(&bytes)),
            Err(ProofError::UnsupportedVersion(9))
        );
        let bytes = base64url::decode(&code).unwrap();
        assert_eq!(
            ScoreProof::decode(&base64url::encode(&bytes[..bytes.len() - 1])),
            Err(ProofError::Replay(CodecError::Truncated))
        );
    }
}
