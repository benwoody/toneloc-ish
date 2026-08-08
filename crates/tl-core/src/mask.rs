//! Dial masks: `555-1XXX`.
//!
//! A mask is literal digits with `X` standing for a wildcard. ToneLoc replaced
//! each `X` with a digit — randomly by default, never repeating within a mask
//! — and the fixed part is the prefix every dialed number shares.
//!
//! Masks matter here for a reason beyond dialing: **the data file's name is
//! the mask.** The manual is explicit — *"If you only provide a filename, the
//! filename is also used as the mask"* — and the `.DAT` header has nowhere to
//! record one, so the filename is the only surviving record of what a scan
//! covered. `562XXXX.DAT` is a scan of `562-0000` through `562-9999`, and
//! `TONE.LOG` confirms it: `Mask used: 562XXXX`.
//!
//! This module is deliberately partial. It recognizes masks and reads their
//! shape; generating the dial sequence — random versus sequential, the
//! duplicate check against already-dialed cells — is the scan engine's job and
//! is not built yet.

use std::fmt;

/// A dial mask: literal digits and `X` wildcards.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mask {
    text: String,
}

/// Why a string is not a usable mask.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum MaskError {
    #[error("a mask needs at least one X wildcard")]
    NoWildcards,

    #[error(
        "{0} wildcards is too many: the original used 16-bit integers, so more \
         than 4 X's overflowed and produced garbage"
    )]
    TooManyWildcards(usize),

    #[error("{0:?} is not a digit or an X")]
    InvalidCharacter(char),
}

impl Mask {
    /// The documented wildcard limit.
    ///
    /// From the manual: *"You should never have more than 4 X's in a mask.
    /// ToneLoc will run, but since ToneLoc uses integer variables, the numbers
    /// will be all screwed up, since 5 X's would have 100,000 possible numbers
    /// which is more than 32,768 (integer) and 65,536 (word)."*
    ///
    /// We use `u32`, so the ceiling is gone — but the rule is kept as a
    /// validation rather than quietly allowed. A 5-X mask meant something
    /// specific in 1994 (a corrupted scan), and silently accepting one would
    /// make us able to represent scans the original could not have produced.
    pub const MAX_WILDCARDS: usize = 4;

    /// Parse a mask, rejecting anything the original would have mangled.
    ///
    /// Separators are ignored, so `555-1XXX` and `5551XXX` are the same mask.
    pub fn parse(text: &str) -> Result<Mask, MaskError> {
        let cleaned: String = text
            .chars()
            .filter(|c| !matches!(c, '-' | ' ' | '(' | ')'))
            .map(|c| c.to_ascii_uppercase())
            .collect();

        for c in cleaned.chars() {
            if !c.is_ascii_digit() && c != 'X' {
                return Err(MaskError::InvalidCharacter(c));
            }
        }

        let wildcards = cleaned.chars().filter(|&c| c == 'X').count();
        if wildcards == 0 {
            return Err(MaskError::NoWildcards);
        }
        if wildcards > Self::MAX_WILDCARDS {
            return Err(MaskError::TooManyWildcards(wildcards));
        }

        Ok(Mask { text: cleaned })
    }

    /// Read the mask a `.DAT` file's name encodes, if it encodes one.
    ///
    /// Takes any path; the directory and extension are ignored. Returns `None`
    /// for names that are not masks — the shipped `SAMPLE5.DAT` is a label,
    /// not a scan range.
    pub fn from_filename(path: &str) -> Option<Mask> {
        let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
        let stem = name.split('.').next().unwrap_or(name);
        Mask::parse(stem).ok()
    }

    /// The mask as written, separators stripped.
    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// The fixed digits before the first wildcard — the prefix every number in
    /// this scan shares.
    pub fn prefix(&self) -> &str {
        let end = self.text.find('X').unwrap_or(self.text.len());
        &self.text[..end]
    }

    /// How many wildcards the mask has.
    pub fn wildcards(&self) -> usize {
        self.text.chars().filter(|&c| c == 'X').count()
    }

    /// How many distinct numbers this mask covers.
    pub fn count(&self) -> u32 {
        10u32.pow(self.wildcards() as u32)
    }

    /// Build a full number by substituting into the wildcards, most
    /// significant first. Digits beyond the wildcard count are ignored.
    ///
    /// Wildcards need not be contiguous or trailing: `55X1XXX` is legal, and
    /// substitution walks them left to right.
    pub fn apply(&self, value: u32) -> String {
        let n = self.wildcards();
        let digits = format!("{:0width$}", value % self.count(), width = n);
        let mut digits = digits.chars();
        self.text
            .chars()
            .map(|c| {
                if c == 'X' {
                    digits.next().unwrap_or('0')
                } else {
                    c
                }
            })
            .collect()
    }
}

impl fmt::Display for Mask {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_manuals_own_examples() {
        let m = Mask::parse("555-1XXX").unwrap();
        assert_eq!(m.as_str(), "5551XXX");
        assert_eq!(m.prefix(), "5551");
        assert_eq!(m.wildcards(), 3);
        // "Dial the numbers from 555-1000 to 555-1999"
        assert_eq!(m.count(), 1000);
    }

    #[test]
    fn separators_do_not_change_the_mask() {
        assert_eq!(
            Mask::parse("555-1XXX").unwrap(),
            Mask::parse("5551XXX").unwrap()
        );
        assert_eq!(
            Mask::parse("(562) XXXX").unwrap(),
            Mask::parse("562XXXX").unwrap()
        );
    }

    #[test]
    fn lower_case_x_is_still_a_wildcard() {
        // The manual's own examples mix cases: "/x:3XX /x:1XX".
        assert_eq!(Mask::parse("555-1xxx").unwrap().wildcards(), 3);
    }

    #[test]
    fn the_five_x_rule_is_enforced_rather_than_silently_allowed() {
        // 16-bit overflow territory. We could represent it; the original
        // could not, so a file claiming it would be ahistorical.
        assert_eq!(Mask::parse("55XXXXX"), Err(MaskError::TooManyWildcards(5)));
        assert!(
            Mask::parse("555XXXX").is_ok(),
            "4 X's is the documented limit"
        );
    }

    #[test]
    fn a_mask_needs_a_wildcard_and_only_digits() {
        assert_eq!(Mask::parse("5551234"), Err(MaskError::NoWildcards));
        assert_eq!(
            Mask::parse("555-ABCD"),
            Err(MaskError::InvalidCharacter('A'))
        );
    }

    #[test]
    fn reads_the_mask_out_of_a_data_file_name() {
        // TONE.LOG for this file records "Mask used: 562XXXX".
        let m = Mask::from_filename("reference/562XXXX.DAT").unwrap();
        assert_eq!(m.prefix(), "562");
        assert_eq!(m.count(), 10_000);

        assert_eq!(
            Mask::from_filename("C:\\TL\\562XXXX.DAT").unwrap().prefix(),
            "562"
        );
    }

    #[test]
    fn sample_files_are_labels_not_masks() {
        // Guessing a prefix from these would invent provenance.
        assert!(Mask::from_filename("SAMPLE5.DAT").is_none());
        assert!(Mask::from_filename("SAMPLE12.DAT").is_none());
        assert!(Mask::from_filename("TEST.DAT").is_none());
    }

    #[test]
    fn substitution_fills_wildcards_left_to_right() {
        let m = Mask::parse("555-1XXX").unwrap();
        assert_eq!(m.apply(0), "5551000");
        assert_eq!(m.apply(7), "5551007");
        assert_eq!(m.apply(999), "5551999");

        let full = Mask::parse("562XXXX").unwrap();
        assert_eq!(full.apply(9490), "5629490"); // the number in TONE.LOG
    }

    #[test]
    fn wildcards_need_not_be_trailing() {
        // Legal, and the substitution order still runs left to right.
        let m = Mask::parse("55X1XX").unwrap();
        assert_eq!(m.wildcards(), 3);
        assert_eq!(m.prefix(), "55");
        assert_eq!(m.apply(123), "5511".to_string() + "23");
        assert_eq!(m.apply(123), "551123");
    }

    #[test]
    fn every_value_in_range_yields_a_distinct_number() {
        let m = Mask::parse("555-1XX").unwrap();
        let all: std::collections::BTreeSet<String> = (0..m.count()).map(|v| m.apply(v)).collect();
        assert_eq!(all.len(), 100);
        assert!(all.contains("5551 00".trim()) || all.contains("555100"));
    }
}
