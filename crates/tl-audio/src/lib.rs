//! `tl-audio` — the sound of a phone line, generated rather than recorded.
//!
//! toneloc-ish has no modem, so it has no modem noises either — unless it
//! makes them. This crate synthesizes every sound a 1990s scan produced from
//! the frequencies that actually produced it: DTMF pairs from ITU-T Q.23, the
//! Bell precise-tone plan for dial/ringback/busy, a 2100 Hz V.25 answer tone,
//! and a staged V.32bis handshake that trains and settles into carrier.
//!
//! There are no audio files in this repository. The constants in
//! [`synth`] *are* the archive.
//!
//! ```
//! use tl_audio::{sound_for, render_all, Standard};
//! use tl_core::CellClass;
//!
//! // Replay one number from a 1993 scan that found a modem.
//! let sounds = sound_for("5559999", CellClass::Carrier.with_rings(1), Standard::V32bis);
//! let samples = render_all(&sounds);
//! assert!(!samples.is_empty());
//! ```

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

pub mod recorder;
pub mod sounds;
pub mod synth;
pub mod wav;

#[cfg(feature = "playback")]
pub mod player;

pub use recorder::Recorder;
pub use sounds::{CallSound, Standard, render_all, sound_for};
pub use synth::{SAMPLE_RATE, Samples};

#[cfg(feature = "playback")]
pub use player::Player;
