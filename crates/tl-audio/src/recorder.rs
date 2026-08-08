//! Recording what a session sounded like, without recording anything.
//!
//! Because every sound here is generated rather than captured, a session's
//! audio can be reconstructed exactly instead of being taped off the speakers.
//! Place each sound at the moment it fired and you get a bit-exact soundtrack
//! with no loopback device, no routing, and no noise floor — one that can be
//! muxed straight onto a silent screen recording.
//!
//! Deliberately not tied to a clock: the caller says *when*, so this stays
//! pure and testable.

use crate::synth::{SAMPLE_RATE, Samples};

/// A timeline of audio, filled in at arbitrary offsets.
#[derive(Debug, Default)]
pub struct Recorder {
    timeline: Samples,
}

impl Recorder {
    pub fn new() -> Recorder {
        Recorder {
            timeline: Samples::new(),
        }
    }

    /// Place `samples` starting `offset` seconds into the timeline.
    ///
    /// Overlapping sounds are summed, then soft-limited on the way out, which
    /// is what happens on a real line when something starts before the last
    /// thing finished.
    pub fn add_at(&mut self, offset: f32, samples: &[f32]) {
        if samples.is_empty() {
            return;
        }
        let start = (offset.max(0.0) * SAMPLE_RATE as f32) as usize;
        let end = start + samples.len();
        if self.timeline.len() < end {
            self.timeline.resize(end, 0.0);
        }
        for (slot, s) in self.timeline[start..end].iter_mut().zip(samples) {
            *slot += s;
        }
    }

    /// Extend with silence so the recording is at least `seconds` long.
    ///
    /// Matters for muxing: the audio track has to run as long as the video, or
    /// the last seconds of the scan play over a frozen frame.
    pub fn pad_to(&mut self, seconds: f32) {
        let want = (seconds.max(0.0) * SAMPLE_RATE as f32) as usize;
        if self.timeline.len() < want {
            self.timeline.resize(want, 0.0);
        }
    }

    /// Length of the recording so far, in seconds.
    pub fn duration(&self) -> f32 {
        self.timeline.len() as f32 / SAMPLE_RATE as f32
    }

    pub fn is_empty(&self) -> bool {
        self.timeline.is_empty()
    }

    /// The finished timeline, soft-limited so summed sounds cannot clip.
    pub fn finish(mut self) -> Samples {
        crate::synth::limit(&mut self.timeline);
        self.timeline
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synth;

    #[test]
    fn a_sound_lands_at_the_offset_it_was_given() {
        let mut r = Recorder::new();
        r.add_at(2.0, &synth::tone(440.0, 0.5, 0.5));

        let out = r.finish();
        assert_eq!(out.len(), synth::samples_for(2.5));
        // Silent before, audible after.
        assert!(out[..synth::samples_for(2.0)].iter().all(|s| *s == 0.0));
        let after = &out[synth::samples_for(2.1)..synth::samples_for(2.4)];
        assert!(after.iter().any(|s| s.abs() > 0.1));
    }

    #[test]
    fn sounds_placed_out_of_order_still_land_correctly() {
        let mut late = Recorder::new();
        late.add_at(1.0, &synth::tone(440.0, 0.1, 0.5));
        late.add_at(0.0, &synth::tone(880.0, 0.1, 0.5));

        let mut early = Recorder::new();
        early.add_at(0.0, &synth::tone(880.0, 0.1, 0.5));
        early.add_at(1.0, &synth::tone(440.0, 0.1, 0.5));

        assert_eq!(late.finish(), early.finish());
    }

    #[test]
    fn overlapping_sounds_are_summed_and_do_not_clip() {
        let mut r = Recorder::new();
        r.add_at(0.0, &synth::tone(440.0, 0.5, 0.9));
        r.add_at(0.1, &synth::tone(660.0, 0.5, 0.9));
        r.add_at(0.2, &synth::tone(880.0, 0.5, 0.9));

        let out = r.finish();
        let peak = out.iter().fold(0.0f32, |a, b| a.max(b.abs()));
        assert!(peak <= 1.0, "peaked at {peak}");
        assert!(peak > 0.5, "three overlapping tones should be loud");
    }

    #[test]
    fn padding_extends_but_never_truncates() {
        let mut r = Recorder::new();
        r.add_at(0.0, &synth::tone(440.0, 1.0, 0.5));
        assert!((r.duration() - 1.0).abs() < 0.01);

        r.pad_to(5.0);
        assert!((r.duration() - 5.0).abs() < 0.01);

        // A shorter pad leaves it alone: the audio must not be cut off.
        r.pad_to(2.0);
        assert!((r.duration() - 5.0).abs() < 0.01);
    }

    #[test]
    fn an_empty_recording_is_empty_not_a_click() {
        let r = Recorder::new();
        assert!(r.is_empty());
        assert_eq!(r.duration(), 0.0);
        assert!(r.finish().is_empty());
    }

    #[test]
    fn adding_nothing_changes_nothing() {
        let mut r = Recorder::new();
        r.add_at(10.0, &[]);
        assert!(
            r.is_empty(),
            "an empty sound should not extend the timeline"
        );
    }

    #[test]
    fn a_negative_offset_is_clamped_to_the_start() {
        let mut r = Recorder::new();
        r.add_at(-5.0, &synth::tone(440.0, 0.1, 0.5));
        assert!((r.duration() - 0.1).abs() < 0.01);
    }

    #[test]
    fn the_result_is_a_playable_wav() {
        let mut r = Recorder::new();
        r.add_at(0.5, &synth::tone(440.0, 0.2, 0.5));
        r.pad_to(2.0);
        let wav = crate::wav::encode(&r.finish());
        assert_eq!(&wav[0..4], b"RIFF");
        // 2 seconds, 16-bit mono.
        assert_eq!(wav.len(), 44 + synth::samples_for(2.0) * 2);
    }
}
