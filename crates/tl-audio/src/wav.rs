//! Minimal WAV writer.
//!
//! Hand-rolled rather than pulled in as a dependency: the format is forty
//! lines, and it means the synthesis half of this crate has no dependencies at
//! all. It also means every sound can be dumped to a file and inspected on a
//! machine with no sound card, which is how CI checks them.

use crate::synth::SAMPLE_RATE;
use std::io::{self, Write};

/// Encode mono `f32` samples as a 16-bit PCM WAV file.
pub fn encode(samples: &[f32]) -> Vec<u8> {
    let data_len = samples.len() * 2;
    let mut out = Vec::with_capacity(44 + data_len);

    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&((36 + data_len) as u32).to_le_bytes());
    out.extend_from_slice(b"WAVE");

    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes()); // chunk size
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    out.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes()); // byte rate
    out.extend_from_slice(&2u16.to_le_bytes()); // block align
    out.extend_from_slice(&16u16.to_le_bytes()); // bits per sample

    out.extend_from_slice(b"data");
    out.extend_from_slice(&(data_len as u32).to_le_bytes());
    for &s in samples {
        let v = (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

/// Write samples to a WAV file.
pub fn write(path: impl AsRef<std::path::Path>, samples: &[f32]) -> io::Result<()> {
    let mut f = std::fs::File::create(path)?;
    f.write_all(&encode(samples))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn produces_a_well_formed_riff_header() {
        let wav = encode(&[0.0; 100]);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(&wav[12..16], b"fmt ");
        assert_eq!(&wav[36..40], b"data");
        assert_eq!(wav.len(), 44 + 200);
        // Declared sizes agree with the actual buffer.
        let riff_len = u32::from_le_bytes(wav[4..8].try_into().unwrap());
        assert_eq!(riff_len as usize, wav.len() - 8);
        let data_len = u32::from_le_bytes(wav[40..44].try_into().unwrap());
        assert_eq!(data_len as usize, wav.len() - 44);
    }

    #[test]
    fn full_scale_samples_do_not_wrap_around() {
        let wav = encode(&[1.0, -1.0, 2.0, -2.0]);
        let s: Vec<i16> = wav[44..]
            .chunks_exact(2)
            .map(|c| i16::from_le_bytes([c[0], c[1]]))
            .collect();
        assert_eq!(s, vec![i16::MAX, -i16::MAX, i16::MAX, -i16::MAX]);
    }
}
