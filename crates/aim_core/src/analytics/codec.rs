//! Compact, versioned binary format for a [`Recording`], used to save replays.
//!
//! The scenario definition is embedded as TOML so a replay still reproduces the run after the
//! scenario file is edited or removed. All integers and floats are little-endian; floats are
//! stored bit-exactly, so decoding returns an identical recording.
//!
//! ```text
//! magic    4 bytes  "AIMR"
//! version  u16      1
//! seed     u64
//! def_len  u32      then def_len bytes of UTF-8 TOML (the ScenarioDef)
//! frames   u32      then one frame record per Session::update call:
//!   dt f32, yaw_deg f32, pitch_deg f32, flags u8
//!   flags: bit0 fire_pressed, bit1 fire_held, bit2 walk, bit3 movement follows
//!   if bit3: right f32, forward f32
//! ```
//!
//! A frame without movement takes 13 bytes: a 60 s run at 240 FPS is about 190 KB.

use crate::math::ViewAngles;
use crate::movement::MoveInput;
use crate::scenario::{FrameInput, Recording, ScenarioDef};

/// Four-byte file signature at the start of every replay.
pub const MAGIC: [u8; 4] = *b"AIMR";
/// Current replay format version.
pub const VERSION: u16 = 1;

const FIRE_PRESSED: u8 = 1 << 0;
const FIRE_HELD: u8 = 1 << 1;
const WALK: u8 = 1 << 2;
const MOVEMENT: u8 = 1 << 3;

/// Why a replay could not be encoded or decoded.
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum CodecError {
    /// The data does not start with [`MAGIC`].
    #[error("not a replay file")]
    BadMagic,
    /// The format version is not [`VERSION`]; carries the version found.
    #[error("unsupported replay version {0}")]
    UnsupportedVersion(u16),
    /// The data ended before the record was complete.
    #[error("replay data ends early")]
    Truncated,
    /// Bytes remained after the last frame; carries how many.
    #[error("replay has {0} unexpected trailing bytes")]
    TrailingBytes(usize),
    /// The embedded scenario could not be written or parsed; carries the reason.
    #[error("embedded scenario is invalid: {0}")]
    Scenario(String),
}

/// Encode a recording as a replay file.
pub fn encode(recording: &Recording) -> Result<Vec<u8>, CodecError> {
    let def = toml::to_string(&recording.def).map_err(|e| CodecError::Scenario(e.to_string()))?;
    let mut out = Vec::with_capacity(22 + def.len() + recording.inputs.len() * 13);
    out.extend_from_slice(&MAGIC);
    out.extend_from_slice(&VERSION.to_le_bytes());
    out.extend_from_slice(&recording.seed.to_le_bytes());
    out.extend_from_slice(&(def.len() as u32).to_le_bytes());
    out.extend_from_slice(def.as_bytes());
    out.extend_from_slice(&(recording.inputs.len() as u32).to_le_bytes());
    for f in &recording.inputs {
        let has_movement = f.movement.right.to_bits() != 0 || f.movement.forward.to_bits() != 0;
        let mut flags = 0;
        for (on, bit) in [
            (f.fire_pressed, FIRE_PRESSED),
            (f.fire_held, FIRE_HELD),
            (f.movement.walk, WALK),
            (has_movement, MOVEMENT),
        ] {
            if on {
                flags |= bit;
            }
        }
        for v in [f.dt, f.view.yaw_deg, f.view.pitch_deg] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.push(flags);
        if has_movement {
            out.extend_from_slice(&f.movement.right.to_le_bytes());
            out.extend_from_slice(&f.movement.forward.to_le_bytes());
        }
    }
    Ok(out)
}

struct Reader<'a> {
    bytes: &'a [u8],
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], CodecError> {
        if self.bytes.len() < n {
            return Err(CodecError::Truncated);
        }
        let (head, tail) = self.bytes.split_at(n);
        self.bytes = tail;
        Ok(head)
    }
    fn array<const N: usize>(&mut self) -> Result<[u8; N], CodecError> {
        let mut a = [0; N];
        a.copy_from_slice(self.take(N)?);
        Ok(a)
    }
    fn u8(&mut self) -> Result<u8, CodecError> {
        Ok(self.array::<1>()?[0])
    }
    fn u16(&mut self) -> Result<u16, CodecError> {
        Ok(u16::from_le_bytes(self.array()?))
    }
    fn u32(&mut self) -> Result<u32, CodecError> {
        Ok(u32::from_le_bytes(self.array()?))
    }
    fn u64(&mut self) -> Result<u64, CodecError> {
        Ok(u64::from_le_bytes(self.array()?))
    }
    fn f32(&mut self) -> Result<f32, CodecError> {
        Ok(f32::from_le_bytes(self.array()?))
    }
}

/// Decode a replay file back into the identical recording.
pub fn decode(bytes: &[u8]) -> Result<Recording, CodecError> {
    let mut r = Reader { bytes };
    if r.take(4).map_err(|_| CodecError::BadMagic)? != MAGIC {
        return Err(CodecError::BadMagic);
    }
    let version = r.u16()?;
    if version != VERSION {
        return Err(CodecError::UnsupportedVersion(version));
    }
    let seed = r.u64()?;
    let def_len = r.u32()? as usize;
    let def_src =
        std::str::from_utf8(r.take(def_len)?).map_err(|e| CodecError::Scenario(e.to_string()))?;
    let def: ScenarioDef =
        toml::from_str(def_src).map_err(|e| CodecError::Scenario(e.to_string()))?;
    let frames = r.u32()? as usize;
    // Each frame takes at least 13 bytes; refuse counts the data cannot hold before allocating.
    if frames > r.bytes.len() / 13 {
        return Err(CodecError::Truncated);
    }
    let mut inputs = Vec::with_capacity(frames);
    for _ in 0..frames {
        let dt = r.f32()?;
        let view = ViewAngles {
            yaw_deg: r.f32()?,
            pitch_deg: r.f32()?,
        };
        let flags = r.u8()?;
        let (right, forward) = if flags & MOVEMENT != 0 {
            (r.f32()?, r.f32()?)
        } else {
            (0.0, 0.0)
        };
        inputs.push(FrameInput {
            dt,
            view,
            fire_pressed: flags & FIRE_PRESSED != 0,
            fire_held: flags & FIRE_HELD != 0,
            movement: MoveInput {
                right,
                forward,
                walk: flags & WALK != 0,
            },
        });
    }
    if !r.bytes.is_empty() {
        return Err(CodecError::TrailingBytes(r.bytes.len()));
    }
    Ok(Recording { def, seed, inputs })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analytics::test_support::record;
    use crate::scenario::builtin;
    use crate::sim::AimerProfile;

    #[test]
    fn every_builtin_definition_round_trips_through_the_embedded_toml() {
        for def in builtin::all() {
            let rec = Recording::new(def, 1);
            assert_eq!(decode(&encode(&rec).unwrap()).unwrap(), rec);
        }
    }

    #[test]
    fn a_recorded_run_round_trips_exactly_and_replays_the_same_result() {
        let mut rec = record("counter-strafe", AimerProfile::PRESETS[1], 8);
        // The simulated aimer does not move; add movement and walk frames to cover them.
        for (i, f) in rec.inputs.iter_mut().enumerate().step_by(7) {
            f.movement = MoveInput {
                right: if i % 2 == 0 { 1.0 } else { -1.0 },
                forward: -0.0,
                walk: i % 3 == 0,
            };
        }
        let bytes = encode(&rec).unwrap();
        let back = decode(&bytes).unwrap();
        assert_eq!(back, rec);
        assert!(
            back.inputs
                .iter()
                .zip(&rec.inputs)
                .all(|(a, b)| a.movement.forward.to_bits() == b.movement.forward.to_bits())
        );
        assert_eq!(
            back.replay(|_, _| {}).result(),
            rec.replay(|_, _| {}).result()
        );
    }

    #[test]
    fn corrupt_data_is_rejected() {
        let rec = record("micro-adjust", AimerProfile::PRESETS[1], 2);
        let bytes = encode(&rec).unwrap();
        assert_eq!(decode(b"nope"), Err(CodecError::BadMagic));
        let mut v = bytes.clone();
        v[4] = 9;
        assert_eq!(decode(&v), Err(CodecError::UnsupportedVersion(9)));
        assert_eq!(
            decode(&bytes[..bytes.len() - 1]),
            Err(CodecError::Truncated)
        );
        let mut v = bytes.clone();
        v.push(0);
        assert_eq!(decode(&v), Err(CodecError::TrailingBytes(1)));
    }
}
