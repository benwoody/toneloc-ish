//! What a call sounded like, assembled from [`crate::synth`] primitives.
//!
//! The handshake is the centrepiece. It is not one noise but a negotiation
//! with audible stages, and reconstructing those stages in order is what makes
//! it recognizable rather than merely screechy:
//!
//! 1. the answering modem's 2100 Hz tone, phase-reversed on a cadence so the
//!    calling modem knows to switch its echo canceller on (V.25 / V.8);
//! 2. a V.21-rate exchange of capabilities, low and warbling;
//! 3. training — alternating carrier segments, then scrambled data, the part
//!    that sounds like a fax arguing with a kettle;
//! 4. the carrier settles into flat noise, and `CONNECT` arrives.
//!
//! On a real scan the speaker went quiet after CONNECT, which is why the last
//! thing you heard all night was the hiss cutting off.

use crate::synth::{self, Samples};
use tl_core::{Cell, CellClass};

/// Modulation standard to imitate. Different eras, different screech.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Standard {
    /// 2400 bps, 1984. Short and businesslike: two carriers, a quick scramble.
    V22bis,
    /// 14400 bps, 1991. The long one everybody remembers.
    #[default]
    V32bis,
}

/// One thing you could hear while a scan ran.
#[derive(Clone, Debug, PartialEq)]
pub enum CallSound {
    /// Off-hook: dial tone until the first digit.
    DialTone { seconds: f32 },
    /// DTMF digits. Non-keypad characters (`W`, `-`, `,`) become short pauses,
    /// which is exactly what the modem did with them.
    Dial { digits: String },
    /// Ringback, `count` rings.
    Ringback { count: usize },
    /// Busy signal.
    Busy { cycles: usize },
    /// Fast busy — all circuits busy.
    Reorder { cycles: usize },
    /// The three-note intercept before a recorded message.
    Sit,
    /// A steady dialtone found at the far end: a PBX, a loop, a long-distance
    /// carrier. **This is a Tone** — the thing you were scanning for.
    FarEndTone { seconds: f32 },
    /// A full modem handshake ending in carrier. **This is a Carrier.**
    Handshake { standard: Standard, seed: u64 },
    /// Nothing at all, until WaitDelay gave up.
    Silence { seconds: f32 },
    /// Going back on-hook.
    HangUp,
}

impl CallSound {
    /// Render to samples.
    pub fn render(&self) -> Samples {
        let mut out = match self {
            CallSound::DialTone { seconds } => {
                synth::shaped(synth::dual_tone(synth::DIAL_TONE, *seconds, 0.55))
            }
            CallSound::Dial { digits } => dial(digits),
            CallSound::Ringback { count } => {
                // 2 s on, 4 s off — shortened to 1.6/1.2 so a replay does not
                // spend a full minute on one number. Cadence, not duration, is
                // what makes it read as ringing.
                synth::cadenced(synth::RINGBACK, 1.6, 1.2, *count, 0.5)
            }
            CallSound::Busy { cycles } => synth::cadenced(synth::BUSY, 0.5, 0.5, *cycles, 0.5),
            CallSound::Reorder { cycles } => {
                synth::cadenced(synth::REORDER, 0.25, 0.25, *cycles, 0.5)
            }
            CallSound::Sit => sit(),
            CallSound::FarEndTone { seconds } => {
                // A bare 350+440 with none of the switching-office wobble: the
                // giveaway that you have reached equipment, not a subscriber.
                synth::shaped(synth::dual_tone(synth::DIAL_TONE, *seconds, 0.6))
            }
            CallSound::Handshake { standard, seed } => handshake(*standard, *seed),
            CallSound::Silence { seconds } => synth::silence(*seconds),
            CallSound::HangUp => synth::shaped(synth::noise(0.04, 0.25, 0xC1_1C)),
        };
        synth::limit(&mut out);
        out
    }

    /// How long this sound runs, in seconds.
    pub fn duration(&self) -> f32 {
        self.render().len() as f32 / synth::SAMPLE_RATE as f32
    }

    /// A short human name for this sound.
    pub fn label(&self) -> String {
        match self {
            CallSound::DialTone { .. } => "dial tone".into(),
            CallSound::Dial { digits } => format!("dialing {digits}"),
            CallSound::Ringback { count } => {
                format!("ringback ×{count}")
            }
            CallSound::Busy { .. } => "busy signal".into(),
            CallSound::Reorder { .. } => "reorder (fast busy)".into(),
            CallSound::Sit => "SIT intercept".into(),
            CallSound::FarEndTone { .. } => "steady tone at the far end".into(),
            CallSound::Handshake { standard, .. } => match standard {
                Standard::V22bis => "handshake, V.22bis".into(),
                Standard::V32bis => "handshake, V.32bis".into(),
            },
            CallSound::Silence { .. } => "silence".into(),
            CallSound::HangUp => "hang up".into(),
        }
    }

    /// What is actually being generated, in frequencies.
    ///
    /// This is the part worth printing: it turns "listen to a modem" into a
    /// legible account of *why* it sounds like that, which is the whole reason
    /// these are synthesized rather than sampled.
    pub fn detail(&self) -> String {
        match self {
            CallSound::DialTone { .. } | CallSound::FarEndTone { .. } => {
                format!("{} + {} Hz", synth::DIAL_TONE.0, synth::DIAL_TONE.1)
            }
            CallSound::Dial { digits } => {
                let keys = digits
                    .chars()
                    .filter(|c| synth::dtmf_pair(*c).is_some())
                    .count();
                let pauses = digits.chars().count() - keys;
                match pauses {
                    0 => format!("{keys} DTMF pairs"),
                    1 => format!("{keys} DTMF pairs, 1 pause"),
                    n => format!("{keys} DTMF pairs, {n} pauses"),
                }
            }
            CallSound::Ringback { .. } => {
                format!("{} + {} Hz", synth::RINGBACK.0, synth::RINGBACK.1)
            }
            CallSound::Busy { .. } | CallSound::Reorder { .. } => {
                format!("{} + {} Hz", synth::BUSY.0, synth::BUSY.1)
            }
            CallSound::Sit => format!(
                "{} / {} / {} Hz",
                synth::SIT[0],
                synth::SIT[1],
                synth::SIT[2]
            ),
            CallSound::Handshake { standard, .. } => match standard {
                // Kept short enough to sit inside an 80-column line.
                Standard::V22bis => format!(
                    "{} Hz answer, {}/{} Hz carriers",
                    synth::ANSWER_TONE,
                    synth::V22BIS_CARRIERS.0,
                    synth::V22BIS_CARRIERS.1
                ),
                Standard::V32bis => format!(
                    "{} Hz answer, training, {} Hz carrier",
                    synth::ANSWER_TONE,
                    synth::V32_CARRIER
                ),
            },
            CallSound::Silence { .. } => "nothing on the line".into(),
            CallSound::HangUp => "on-hook".into(),
        }
    }
}

/// DTMF for a dial string, with the inter-digit gap real keypads leave.
fn dial(digits: &str) -> Samples {
    const TONE_LEN: f32 = 0.09;
    const GAP: f32 = 0.06;
    let mut out = Samples::new();
    for ch in digits.chars() {
        match synth::dtmf_pair(ch) {
            Some(pair) => out.extend(synth::shaped(synth::dual_tone(pair, TONE_LEN, 0.5))),
            // `W` means "wait for dialtone" and punctuation means "pause".
            // Both are silence on the line.
            None => out.extend(synth::silence(TONE_LEN)),
        }
        out.extend(synth::silence(GAP));
    }
    out
}

/// The three rising notes of a Special Information Tone.
fn sit() -> Samples {
    let mut out = Samples::new();
    for (i, f) in synth::SIT.iter().enumerate() {
        // The third note is held longer than the first two.
        let len = if i == 2 { 0.38 } else { 0.33 };
        out.extend(synth::shaped(synth::tone(*f, len, 0.5)));
    }
    out
}

/// A full modem handshake, ending in settled carrier.
fn handshake(standard: Standard, seed: u64) -> Samples {
    let mut out = Samples::new();

    // Stage 1 — the answering modem's 2100 Hz tone. V.32 reverses its phase
    // every 450 ms; that periodic "chunk" is what tells the far end an echo
    // canceller is present.
    let reversals = matches!(standard, Standard::V32bis);
    out.extend(answer_tone(if reversals { 3.15 } else { 2.2 }, reversals));
    out.extend(synth::silence(0.08));

    // Stage 2 — capability exchange at V.21 rates: slow, two-tone, warbling.
    out.extend(v21_babble(0.55, synth::V21_ORIG, seed));
    out.extend(v21_babble(0.45, synth::V21_ANSW, seed ^ 0x5A5A));

    match standard {
        Standard::V22bis => {
            // Two carriers come up together and scramble briefly.
            let mut seg = synth::tone(synth::V22BIS_CARRIERS.0, 0.7, 0.28);
            synth::mix_into(&mut seg, &synth::tone(synth::V22BIS_CARRIERS.1, 0.7, 0.28));
            synth::mix_into(&mut seg, &scrambled(0.7, 0.3, seed ^ 0x22B1));
            out.extend(synth::shaped(seg));
        }
        Standard::V32bis => {
            // Stage 3 — training. Alternating "AA" segments (the 1800 Hz
            // carrier beating against its sidebands) interleaved with
            // scrambled data. This is the argument-with-a-kettle part.
            for i in 0..4 {
                let mut aa = synth::tone(synth::V32_CARRIER - 600.0, 0.16, 0.3);
                synth::mix_into(&mut aa, &synth::tone(synth::V32_CARRIER + 600.0, 0.16, 0.3));
                out.extend(synth::shaped(aa));
                out.extend(synth::shaped(scrambled(
                    0.22,
                    0.34,
                    seed ^ ((i as u64) << 8),
                )));
            }
            // Rate negotiation: a short sweep as the modems probe the channel.
            out.extend(synth::shaped(synth::sweep(600.0, 3000.0, 0.35, 0.3)));
            out.extend(synth::shaped(scrambled(0.9, 0.36, seed ^ 0x32B1)));
        }
    }

    // Stage 4 — carrier settles. Flat, wide, almost peaceful.
    let mut carrier = scrambled(1.1, 0.26, seed ^ 0xCA_11E5);
    synth::mix_into(&mut carrier, &synth::tone(synth::V32_CARRIER, 1.1, 0.06));
    out.extend(synth::shaped(carrier));

    // ...and CONNECT: the speaker cuts out mid-hiss.
    out.extend(synth::silence(0.25));
    out
}

/// 2100 Hz, optionally with the V.32 phase reversals.
fn answer_tone(seconds: f32, reversals: bool) -> Samples {
    let n = synth::samples_for(seconds);
    let w = std::f32::consts::TAU * synth::ANSWER_TONE / synth::SAMPLE_RATE as f32;
    let period = synth::samples_for(0.45);
    let mut out = Samples::with_capacity(n);
    for i in 0..n {
        let flip = if reversals && (i / period) % 2 == 1 {
            std::f32::consts::PI
        } else {
            0.0
        };
        out.push(0.4 * (w * i as f32 + flip).sin());
    }
    synth::shaped(out)
}

/// Frequency-shift keying between a mark and a space tone, switching on
/// pseudo-random bits — the low warble of a V.21 exchange.
fn v21_babble(seconds: f32, pair: (f32, f32), seed: u64) -> Samples {
    let (mark, space) = pair;
    let bit_len = synth::samples_for(1.0 / 300.0); // 300 baud
    let n = synth::samples_for(seconds);
    let mut rng = synth::Rng::new(seed);
    let mut out = Samples::with_capacity(n);
    let mut phase = 0.0f32;
    let mut bit = rng.next_u64() & 1 == 1;
    for i in 0..n {
        if bit_len > 0 && i % bit_len == 0 {
            bit = rng.next_u64() & 1 == 1;
        }
        let f = if bit { mark } else { space };
        phase += std::f32::consts::TAU * f / synth::SAMPLE_RATE as f32;
        out.push(0.32 * phase.sin());
    }
    synth::shaped(out)
}

/// Scrambled data: noise shaped into the 3.1 kHz voice band.
///
/// Real QAM data is white-ish inside the channel and absent outside it. A
/// one-pole high-pass fed into a one-pole low-pass gets close enough that the
/// ear reads it as "modem", not "static".
fn scrambled(seconds: f32, amplitude: f32, seed: u64) -> Samples {
    let raw = synth::noise(seconds, 1.0, seed);
    let mut hp_state = 0.0f32;
    let mut lp_state = 0.0f32;
    // Cutoffs at roughly 300 Hz and 3400 Hz, the passband of a phone circuit.
    let hp_a = 0.96;
    let lp_a = 0.35;
    raw.into_iter()
        .map(|x| {
            hp_state = hp_a * (hp_state + x);
            let hp = x - hp_state * (1.0 - hp_a);
            lp_state += lp_a * (hp - lp_state);
            amplitude * lp_state.clamp(-1.0, 1.0)
        })
        .collect()
}

/// The sound of dialing a number and getting the result a `.DAT` recorded.
///
/// This is what makes an archival scan playable: replaying `SAMPLE1.DAT` plays
/// the actual sequence of tones, busies and handshakes that someone sat
/// through in 1993.
pub fn sound_for(number: &str, cell: Cell, standard: Standard) -> Vec<CallSound> {
    let mut seq = vec![
        CallSound::DialTone { seconds: 0.5 },
        CallSound::Dial {
            digits: number.to_string(),
        },
    ];
    let rings = cell.rings().max(1) as usize;
    // Seed from the number so the same cell always sounds the same on replay.
    let seed = number
        .bytes()
        .fold(0x9E37_79B9_u64, |a, b| a.rotate_left(7) ^ b as u64);

    match cell.class() {
        CellClass::Busy => seq.push(CallSound::Busy { cycles: 4 }),
        CellClass::Voice => {
            seq.push(CallSound::Ringback { count: rings });
            // A person picked up. We do not synthesize a voice; the click of
            // the pickup and the silence after it is the honest rendering.
            seq.push(CallSound::HangUp);
        }
        CellClass::NoDialtone => seq.push(CallSound::Silence { seconds: 1.2 }),
        CellClass::Ringout => seq.push(CallSound::Ringback { count: rings }),
        CellClass::Timeout => {
            seq.push(CallSound::Ringback {
                count: rings.min(2),
            });
            seq.push(CallSound::Silence { seconds: 1.0 });
        }
        CellClass::Tone => {
            seq.push(CallSound::FarEndTone { seconds: 1.8 });
        }
        CellClass::Carrier => {
            if cell.rings() > 0 {
                seq.push(CallSound::Ringback { count: rings });
            }
            seq.push(CallSound::Handshake { standard, seed });
        }
        CellClass::Noted => seq.push(CallSound::Ringback { count: rings }),
        CellClass::Aborted => seq.push(CallSound::Ringback { count: rings }),
        // Never dialed, so there is nothing to hear.
        CellClass::Undialed
        | CellClass::Excluded
        | CellClass::Omitted
        | CellClass::Blacklisted
        | CellClass::Dialed
        | CellClass::Unknown => return Vec::new(),
    }

    seq.push(CallSound::HangUp);
    seq
}

/// Render a sequence of sounds end to end.
pub fn render_all(sounds: &[CallSound]) -> Samples {
    let mut out = Samples::new();
    for s in sounds {
        out.extend(s.render());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rms(s: &[f32]) -> f32 {
        if s.is_empty() {
            return 0.0;
        }
        (s.iter().map(|x| x * x).sum::<f32>() / s.len() as f32).sqrt()
    }

    #[test]
    fn a_handshake_has_audible_stages_not_one_flat_noise() {
        let s = handshake(Standard::V32bis, 7);
        let dur = s.len() as f32 / synth::SAMPLE_RATE as f32;
        assert!((6.0..14.0).contains(&dur), "handshake ran {dur}s");

        // The answer tone at the start should be much more tonal than the
        // scrambled carrier at the end. Zero-crossing rate is a decent proxy:
        // a pure 2100 Hz tone crosses regularly, noise crosses erratically.
        let head = &s[..synth::SAMPLE_RATE as usize];
        let tail = &s[s.len() - synth::SAMPLE_RATE as usize..];
        let zc = |x: &[f32]| x.windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count();
        assert!(
            zc(head) < zc(tail),
            "expected the settled carrier to be noisier than the answer tone"
        );
    }

    #[test]
    fn both_standards_render_and_v22bis_is_the_shorter_one() {
        let short = handshake(Standard::V22bis, 1).len();
        let long = handshake(Standard::V32bis, 1).len();
        assert!(short > 0 && long > short, "{short} vs {long}");
    }

    #[test]
    fn handshakes_are_reproducible_from_their_seed() {
        assert_eq!(
            handshake(Standard::V32bis, 99),
            handshake(Standard::V32bis, 99)
        );
        assert_ne!(
            handshake(Standard::V32bis, 99),
            handshake(Standard::V32bis, 98)
        );
    }

    #[test]
    fn dialing_emits_one_burst_per_digit_and_silence_for_pauses() {
        let with_digits = dial("555");
        let with_pause = dial("5W5");
        assert_eq!(with_digits.len(), with_pause.len());
        // The pause version carries less energy: its middle slot is silent.
        assert!(rms(&with_pause) < rms(&with_digits));
    }

    #[test]
    fn a_carrier_cell_produces_a_handshake_and_a_tone_cell_does_not() {
        let carrier = sound_for("5551234", Cell(90), Standard::V32bis);
        assert!(
            carrier
                .iter()
                .any(|s| matches!(s, CallSound::Handshake { .. }))
        );

        let tone = sound_for("5559999", Cell(80), Standard::V32bis);
        assert!(
            tone.iter()
                .any(|s| matches!(s, CallSound::FarEndTone { .. }))
        );
        assert!(
            !tone
                .iter()
                .any(|s| matches!(s, CallSound::Handshake { .. }))
        );
    }

    #[test]
    fn an_undialed_number_makes_no_sound_at_all() {
        assert!(sound_for("5550000", Cell::UNDIALED, Standard::V32bis).is_empty());
        assert!(sound_for("5550000", Cell(130), Standard::V32bis).is_empty()); // blacklisted
        assert!(sound_for("5550000", Cell(100), Standard::V32bis).is_empty()); // excluded
    }

    #[test]
    fn ring_counts_carry_into_the_ringback() {
        let seq = sound_for("5551234", Cell(64), Standard::V32bis); // Ringout, 4 rings
        let rings = seq
            .iter()
            .find_map(|s| match s {
                CallSound::Ringback { count } => Some(*count),
                _ => None,
            })
            .expect("a ringout should ring");
        assert_eq!(rings, 4);
    }

    #[test]
    fn replaying_the_same_number_sounds_identical_every_time() {
        let a = render_all(&sound_for("5551234", Cell(91), Standard::V32bis));
        let b = render_all(&sound_for("5551234", Cell(91), Standard::V32bis));
        assert_eq!(a, b);
        // ...and a different number does not.
        let c = render_all(&sound_for("5551235", Cell(91), Standard::V32bis));
        assert_ne!(a, c);
    }

    #[test]
    fn every_sound_describes_itself_without_leaking_internals() {
        let sounds = sound_for("5551234", Cell(91), Standard::V32bis);
        for s in &sounds {
            let label = s.label();
            let detail = s.detail();
            assert!(!label.is_empty() && !detail.is_empty(), "{s:?}");
            // The seed is an implementation detail, not something to show.
            assert!(!label.contains("seed") && !detail.contains("seed"));
            assert!(!label.contains('{'), "{label} looks like a Debug dump");
        }
    }

    #[test]
    fn the_detail_line_states_the_real_frequencies() {
        assert!(
            CallSound::DialTone { seconds: 1.0 }
                .detail()
                .contains("350")
        );
        assert!(CallSound::Ringback { count: 1 }.detail().contains("440"));
        assert!(CallSound::Busy { cycles: 1 }.detail().contains("620"));
        assert!(
            CallSound::Handshake {
                standard: Standard::V32bis,
                seed: 1
            }
            .detail()
            .contains("2100")
        );
    }

    #[test]
    fn dialing_detail_counts_pauses_separately_from_keys() {
        let d = CallSound::Dial {
            digits: "555W1234".into(),
        }
        .detail();
        assert!(d.contains('7'), "seven real keys: {d}");
        assert!(d.contains("pause"), "the W is a pause: {d}");
    }

    #[test]
    fn everything_stays_inside_the_sample_range() {
        for sound in [
            CallSound::DialTone { seconds: 0.2 },
            CallSound::Dial {
                digits: "5551234".into(),
            },
            CallSound::Ringback { count: 2 },
            CallSound::Busy { cycles: 2 },
            CallSound::Reorder { cycles: 2 },
            CallSound::Sit,
            CallSound::FarEndTone { seconds: 0.3 },
            CallSound::Handshake {
                standard: Standard::V32bis,
                seed: 3,
            },
            CallSound::HangUp,
        ] {
            let s = sound.render();
            assert!(!s.is_empty(), "{sound:?} rendered nothing");
            let peak = s.iter().fold(0.0f32, |a, b| a.max(b.abs()));
            assert!(peak <= 1.0, "{sound:?} peaked at {peak}");
        }
    }
}
