//! Signal generation from first principles.
//!
//! Every sound in toneloc-ish is synthesized from the frequency that actually
//! produced it. Nothing is sampled, so the constants below double as
//! documentation of what a phone line sounded like — which is the point of a
//! preservation project.
//!
//! Sources for the numbers: Bell System call-progress tone plan (precise tones,
//! in service from 1976), ITU-T Q.23 for DTMF, ITU-T V.25 for the answer tone,
//! and ITU-T V.21/V.22bis/V.32 for the modem carriers.

/// Sample rate for everything this crate produces. 44.1 kHz is overkill for a
/// 3.1 kHz phone channel, but it plays anywhere without resampling.
pub const SAMPLE_RATE: u32 = 44_100;

/// A mono buffer of `f32` samples in `[-1.0, 1.0]` at [`SAMPLE_RATE`].
pub type Samples = Vec<f32>;

// ---------------------------------------------------------------------------
// Call-progress tones (Bell System precise tone plan)
// ---------------------------------------------------------------------------

/// Dial tone: 350 Hz + 440 Hz, continuous.
pub const DIAL_TONE: (f32, f32) = (350.0, 440.0);
/// Audible ringback: 440 Hz + 480 Hz, 2 s on / 4 s off.
pub const RINGBACK: (f32, f32) = (440.0, 480.0);
/// Busy: 480 Hz + 620 Hz at 60 interruptions per minute (0.5 s on / 0.5 s off).
pub const BUSY: (f32, f32) = (480.0, 620.0);
/// Reorder, "fast busy": the same pair at twice the rate.
pub const REORDER: (f32, f32) = BUSY;

/// Special Information Tones — the three rising notes before "We're sorry,
/// your call cannot be completed as dialed." Frequencies are the
/// intercept-triple set.
pub const SIT: [f32; 3] = [913.8, 1370.6, 1776.7];

// ---------------------------------------------------------------------------
// DTMF (ITU-T Q.23)
// ---------------------------------------------------------------------------

/// Low (row) frequencies of the DTMF keypad.
pub const DTMF_ROWS: [f32; 4] = [697.0, 770.0, 852.0, 941.0];
/// High (column) frequencies of the DTMF keypad.
pub const DTMF_COLS: [f32; 4] = [1209.0, 1336.0, 1477.0, 1633.0];

/// The tone pair for a keypad character, or `None` for anything that is not a
/// key (a `W` pause in a dial string, punctuation, and so on).
pub fn dtmf_pair(key: char) -> Option<(f32, f32)> {
    let (row, col) = match key.to_ascii_uppercase() {
        '1' => (0, 0),
        '2' => (0, 1),
        '3' => (0, 2),
        'A' => (0, 3),
        '4' => (1, 0),
        '5' => (1, 1),
        '6' => (1, 2),
        'B' => (1, 3),
        '7' => (2, 0),
        '8' => (2, 1),
        '9' => (2, 2),
        'C' => (2, 3),
        '*' => (3, 0),
        '0' => (3, 1),
        '#' => (3, 2),
        'D' => (3, 3),
        _ => return None,
    };
    Some((DTMF_ROWS[row], DTMF_COLS[col]))
}

// ---------------------------------------------------------------------------
// Modem carriers
// ---------------------------------------------------------------------------

/// V.25 answer tone: 2100 Hz. The long flat note the far end sends first.
pub const ANSWER_TONE: f32 = 2100.0;
/// V.21 channel 1 (originating modem) mark and space.
pub const V21_ORIG: (f32, f32) = (980.0, 1180.0);
/// V.21 channel 2 (answering modem) mark and space.
pub const V21_ANSW: (f32, f32) = (1650.0, 1850.0);
/// V.22bis carrier centres: originating 1200 Hz, answering 2400 Hz.
pub const V22BIS_CARRIERS: (f32, f32) = (1200.0, 2400.0);
/// V.32 carrier: a single 1800 Hz centre, both directions, echo-cancelled.
pub const V32_CARRIER: f32 = 1800.0;

// ---------------------------------------------------------------------------
// Generators
// ---------------------------------------------------------------------------

/// Number of samples in a duration.
#[inline]
pub fn samples_for(seconds: f32) -> usize {
    (seconds * SAMPLE_RATE as f32).round().max(0.0) as usize
}

/// Silence.
pub fn silence(seconds: f32) -> Samples {
    vec![0.0; samples_for(seconds)]
}

/// A single sine tone.
pub fn tone(freq: f32, seconds: f32, amplitude: f32) -> Samples {
    let n = samples_for(seconds);
    let w = std::f32::consts::TAU * freq / SAMPLE_RATE as f32;
    (0..n).map(|i| amplitude * (w * i as f32).sin()).collect()
}

/// Two sine tones summed — how every call-progress tone is built.
pub fn dual_tone(pair: (f32, f32), seconds: f32, amplitude: f32) -> Samples {
    let n = samples_for(seconds);
    let (f1, f2) = pair;
    let w1 = std::f32::consts::TAU * f1 / SAMPLE_RATE as f32;
    let w2 = std::f32::consts::TAU * f2 / SAMPLE_RATE as f32;
    (0..n)
        .map(|i| {
            let t = i as f32;
            amplitude * 0.5 * ((w1 * t).sin() + (w2 * t).sin())
        })
        .collect()
}

/// A tone pair interrupted on a cadence, for `cycles` repetitions.
pub fn cadenced(pair: (f32, f32), on: f32, off: f32, cycles: usize, amplitude: f32) -> Samples {
    let mut out = Samples::new();
    for _ in 0..cycles {
        out.extend(shaped(dual_tone(pair, on, amplitude)));
        out.extend(silence(off));
    }
    out
}

/// Deterministic white noise. Seeded so a given scan always sounds identical —
/// the same reason the synthetic exchange seeds its RNG.
pub fn noise(seconds: f32, amplitude: f32, seed: u64) -> Samples {
    let mut rng = Rng::new(seed);
    (0..samples_for(seconds))
        .map(|_| amplitude * rng.next_bipolar())
        .collect()
}

/// A frequency sweep, used for the probing tones inside a handshake.
pub fn sweep(from: f32, to: f32, seconds: f32, amplitude: f32) -> Samples {
    let n = samples_for(seconds);
    let mut phase = 0.0f32;
    (0..n)
        .map(|i| {
            let t = if n > 1 {
                i as f32 / (n - 1) as f32
            } else {
                0.0
            };
            let f = from + (to - from) * t;
            phase += std::f32::consts::TAU * f / SAMPLE_RATE as f32;
            amplitude * phase.sin()
        })
        .collect()
}

/// Apply a short raised-cosine fade to both ends.
///
/// Without this every segment boundary is a discontinuity, and a train of
/// discontinuities is a train of clicks. Real switching equipment gated tones
/// the same way, so this is fidelity as much as hygiene.
pub fn shaped(mut samples: Samples) -> Samples {
    const FADE_SECONDS: f32 = 0.005;
    let fade = samples_for(FADE_SECONDS).min(samples.len() / 2);
    if fade == 0 {
        return samples;
    }
    let len = samples.len();
    for i in 0..fade {
        let g = 0.5 * (1.0 - (std::f32::consts::PI * i as f32 / fade as f32).cos());
        samples[i] *= g;
        samples[len - 1 - i] *= g;
    }
    samples
}

/// Sum one buffer onto another in place, extending if needed. Used to lay a
/// carrier under a training signal.
pub fn mix_into(base: &mut Samples, overlay: &[f32]) {
    if base.len() < overlay.len() {
        base.resize(overlay.len(), 0.0);
    }
    for (b, o) in base.iter_mut().zip(overlay) {
        *b += o;
    }
}

/// Clamp to `[-1.0, 1.0]`, softly, so mixed segments never hard-clip.
pub fn limit(samples: &mut Samples) {
    for s in samples.iter_mut() {
        *s = s.tanh();
    }
}

/// A small deterministic PRNG (xorshift64*), so the crate needs no `rand`
/// dependency and every sound is reproducible from its seed.
#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        // Run the seed through SplitMix64 first. Feeding a raw counter into
        // xorshift gives correlated streams for nearby seeds, and simply
        // forcing the low bit would map 42 and 43 onto the same state — which
        // would make two different numbers in a scan sound identical.
        let mut z = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        // xorshift64* needs any nonzero state.
        Rng(if z == 0 { 0x9E37_79B9_7F4A_7C15 } else { z })
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Uniform in `[-1.0, 1.0)`.
    pub fn next_bipolar(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / 8_388_608.0 - 1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn peak(s: &[f32]) -> f32 {
        s.iter().fold(0.0f32, |a, b| a.max(b.abs()))
    }

    /// Estimate the dominant frequency by counting zero crossings.
    fn approx_freq(s: &[f32]) -> f32 {
        let crossings = s.windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count();
        crossings as f32 * SAMPLE_RATE as f32 / s.len() as f32
    }

    #[test]
    fn a_tone_has_the_frequency_it_was_asked_for() {
        let s = tone(1000.0, 1.0, 0.5);
        assert_eq!(s.len(), SAMPLE_RATE as usize);
        assert!(
            (approx_freq(&s) - 1000.0).abs() < 2.0,
            "got {}",
            approx_freq(&s)
        );
    }

    #[test]
    fn dtmf_pairs_match_the_q23_keypad() {
        assert_eq!(dtmf_pair('1'), Some((697.0, 1209.0)));
        assert_eq!(dtmf_pair('5'), Some((770.0, 1336.0)));
        assert_eq!(dtmf_pair('9'), Some((852.0, 1477.0)));
        assert_eq!(dtmf_pair('0'), Some((941.0, 1336.0)));
        assert_eq!(dtmf_pair('#'), Some((941.0, 1477.0)));
        assert_eq!(dtmf_pair('D'), Some((941.0, 1633.0)));
        // Every key is a distinct pair.
        let keys = "1234567890*#ABCD";
        let mut pairs: Vec<_> = keys.chars().map(|k| dtmf_pair(k).unwrap()).collect();
        pairs.sort_by(|a, b| a.partial_cmp(b).unwrap());
        pairs.dedup();
        assert_eq!(pairs.len(), keys.chars().count());
    }

    #[test]
    fn non_keypad_characters_produce_no_tone() {
        // A dial string's W pause and its punctuation are silent, not noise.
        for c in ['W', ',', '-', ';', ' '] {
            assert_eq!(dtmf_pair(c), None, "{c:?} should not be a DTMF key");
        }
    }

    #[test]
    fn cadence_alternates_sound_and_silence() {
        let busy = cadenced(BUSY, 0.5, 0.5, 2, 0.5);
        assert_eq!(busy.len(), samples_for(2.0));

        // Measure energy over a window, not one sample: any single sample of a
        // sine can sit on a zero crossing.
        let window = |centre: f32| {
            let mid = samples_for(centre);
            let half = samples_for(0.05);
            let slice = &busy[mid - half..mid + half];
            (slice.iter().map(|x| x * x).sum::<f32>() / slice.len() as f32).sqrt()
        };
        assert!(window(0.25) > 0.05, "the 'on' phase should be audible");
        assert_eq!(window(0.75), 0.0, "the 'off' phase should be silent");
        assert!(window(1.25) > 0.05, "the cadence should repeat");
    }

    #[test]
    fn adjacent_seeds_produce_unrelated_streams() {
        // Numbers in a scan seed their sounds; consecutive numbers must not
        // collapse onto the same stream.
        for seed in 0..64u64 {
            assert_ne!(
                noise(0.005, 0.5, seed),
                noise(0.005, 0.5, seed + 1),
                "seeds {seed} and {} collided",
                seed + 1
            );
        }
    }

    #[test]
    fn shaping_removes_the_edge_discontinuity() {
        let raw = tone(440.0, 0.5, 1.0);
        let faded = shaped(raw.clone());
        assert_eq!(faded[0], 0.0);
        assert!(faded[faded.len() - 1].abs() < 1e-6);
        // The middle is untouched.
        let mid = faded.len() / 2;
        assert!((faded[mid] - raw[mid]).abs() < 1e-6);
    }

    #[test]
    fn noise_is_deterministic_for_a_seed_and_differs_across_seeds() {
        assert_eq!(noise(0.01, 0.5, 42), noise(0.01, 0.5, 42));
        assert_ne!(noise(0.01, 0.5, 42), noise(0.01, 0.5, 43));
    }

    #[test]
    fn sweep_starts_low_and_ends_high() {
        let s = sweep(300.0, 3000.0, 1.0, 0.5);
        let head = approx_freq(&s[..SAMPLE_RATE as usize / 10]);
        let tail = approx_freq(&s[s.len() - SAMPLE_RATE as usize / 10..]);
        assert!(head < 600.0, "head {head}");
        assert!(tail > 2500.0, "tail {tail}");
    }

    #[test]
    fn nothing_clips() {
        let mut s = dual_tone(DIAL_TONE, 0.1, 0.9);
        mix_into(&mut s, &tone(1000.0, 0.1, 0.9));
        limit(&mut s);
        assert!(peak(&s) <= 1.0);
    }

    #[test]
    fn mixing_extends_to_the_longer_buffer() {
        let mut base = tone(440.0, 0.01, 0.5);
        let overlay = tone(880.0, 0.02, 0.5);
        mix_into(&mut base, &overlay);
        assert_eq!(base.len(), overlay.len());
    }
}
