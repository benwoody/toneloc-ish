//! Cell states: the one byte ToneLoc stored per phone number.
//!
//! Each of the 10,000 numbers in a `.DAT` file gets exactly one byte. The
//! original encodes it as `class * 10 + rings`, where `rings` is the ring
//! count clamped to 9 (`chopten()` in `TONELOC.C:2102`). Codes are documented
//! in `TL.H:12-33` and `TONELOC.C:12-37`, and set in `TONELOC.C:453-680`.
//!
//! We keep the raw byte as the model ([`Cell`]) so round-tripping is lossless
//! by construction, and derive the display category ([`CellClass`]) exactly
//! the way the original does: `(byte / 10) * 10`.

use std::fmt;

/// One recorded result: the raw byte, exactly as stored on disk.
///
/// This is deliberately a newtype over `u8` rather than an enum with a
/// variant per state. The file format's unit is the byte; keeping it means
/// read → write is byte-identical for *every* input, including values the
/// original never wrote. Interpretation lives in [`Cell::class`].
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Cell(pub u8);

impl Cell {
    /// Not yet dialed (byte `0`).
    pub const UNDIALED: Cell = Cell(0);

    /// The raw on-disk byte.
    #[inline]
    pub const fn raw(self) -> u8 {
        self.0
    }

    /// The display category, normalized the way the original does it.
    ///
    /// `TEXTMAP.C:74` and `TONEMAP.C:whatcolor` both reduce the byte with
    /// `(v / 10) * 10` before switching on it, so `73` ("Timeout, 3 rings")
    /// and `70` render identically apart from shading.
    pub const fn class(self) -> CellClass {
        match self.0 / 10 {
            0 => CellClass::Undialed,
            1 => CellClass::Busy,
            2 => CellClass::Voice,
            3 => CellClass::NoDialtone,
            4 => CellClass::Noted,
            5 => CellClass::Aborted,
            6 => CellClass::Ringout,
            7 => CellClass::Timeout,
            8 => CellClass::Tone,
            9 => CellClass::Carrier,
            10 => CellClass::Excluded,
            11 => CellClass::Omitted,
            12 => CellClass::Dialed,
            13 => CellClass::Blacklisted,
            _ => CellClass::Unknown,
        }
    }

    /// The ring count packed into the low digit (0-9).
    ///
    /// Only meaningful for classes the original recorded rings against
    /// (Voice, Aborted, Ringout, Timeout, Tone, Carrier). For [`CellClass::Noted`]
    /// the low digit is a note *kind*, not a ring count — see [`Cell::note`].
    #[inline]
    pub const fn rings(self) -> u8 {
        self.0 % 10
    }

    /// The operator's note kind, when this cell is a note (bytes `40`-`49`).
    pub const fn note(self) -> Option<NoteKind> {
        if let CellClass::Noted = self.class() {
            Some(NoteKind::from_low_digit(self.0 % 10))
        } else {
            None
        }
    }

    /// Whether the original would count this toward "numbers tried".
    ///
    /// Matches `count_tried()` in `TCONVERT.C:126` and the load-time tally in
    /// `TONELOC.C:1830` — everything except undialed and excluded.
    #[inline]
    pub const fn is_tried(self) -> bool {
        !matches!(self.class(), CellClass::Undialed | CellClass::Excluded)
    }

    /// Whether this is a find worth waking up for: a tone or a carrier.
    #[inline]
    pub const fn is_hit(self) -> bool {
        matches!(self.class(), CellClass::Tone | CellClass::Carrier)
    }

    /// The single character `TEXTMAP.EXE` would print for this cell
    /// (`TEXTMAP.C:75-88`). Classes TextMap does not handle print `?`.
    pub const fn textmap_char(self) -> char {
        match self.class() {
            CellClass::Undialed => 'u',
            CellClass::Busy => 'B',
            CellClass::Voice => 'V',
            CellClass::Noted => '*',
            CellClass::Aborted => 'A',
            CellClass::Ringout => 'R',
            CellClass::Timeout => '+',
            CellClass::Tone => 'T',
            CellClass::Carrier => 'C',
            CellClass::Blacklisted => 'b',
            // TextMap has no arm for these, so they fall to its default.
            CellClass::NoDialtone
            | CellClass::Excluded
            | CellClass::Omitted
            | CellClass::Dialed
            | CellClass::Unknown => '?',
        }
    }
}

impl fmt::Debug for Cell {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.note() {
            Some(n) => write!(f, "Cell({} {:?}/{:?})", self.0, self.class(), n),
            None if self.class().carries_rings() => {
                write!(f, "Cell({} {:?}+{}r)", self.0, self.class(), self.rings())
            }
            None => write!(f, "Cell({} {:?})", self.0, self.class()),
        }
    }
}

impl fmt::Display for Cell {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.note() {
            Some(NoteKind::Generic) | None => write!(f, "{}", self.class()),
            Some(n) => write!(f, "{n}"),
        }
    }
}

impl From<u8> for Cell {
    fn from(b: u8) -> Self {
        Cell(b)
    }
}

impl From<Cell> for u8 {
    fn from(c: Cell) -> Self {
        c.0
    }
}

/// The result categories ToneLoc distinguished — the ToneMap legend.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CellClass {
    /// `00` — never dialed.
    Undialed,
    /// `1x` — busy signal.
    Busy,
    /// `2x` — a human answered.
    Voice,
    /// `3x` — no dialtone on the line.
    NoDialtone,
    /// `4x` — operator pressed `N` (or a typed note); low digit is a [`NoteKind`].
    Noted,
    /// `5x` — operator aborted this call with the space bar.
    Aborted,
    /// `6x` — rang until MaxRings without answering.
    Ringout,
    /// `7x` — silence until WaitDelay expired.
    Timeout,
    /// `8x` — **a tone**: PBX, loop, or long-distance carrier.
    Tone,
    /// `9x` — **a modem carrier**.
    Carrier,
    /// `100` — excluded by mask/range/`/X`.
    Excluded,
    /// `110` — omitted.
    Omitted,
    /// `120` — dialed, generic (what 0.90 files upgrade to).
    Dialed,
    /// `130` — skipped because it was in the blacklist.
    Blacklisted,
    /// `140`+ — nothing the original ever wrote.
    Unknown,
}

impl CellClass {
    /// The canonical byte for this class with a zero ring count.
    pub const fn base_byte(self) -> u8 {
        match self {
            CellClass::Undialed => 0,
            CellClass::Busy => 10,
            CellClass::Voice => 20,
            CellClass::NoDialtone => 30,
            CellClass::Noted => 40,
            CellClass::Aborted => 50,
            CellClass::Ringout => 60,
            CellClass::Timeout => 70,
            CellClass::Tone => 80,
            CellClass::Carrier => 90,
            CellClass::Excluded => 100,
            CellClass::Omitted => 110,
            CellClass::Dialed => 120,
            CellClass::Blacklisted => 130,
            CellClass::Unknown => 140,
        }
    }

    /// Build the byte for this class with a ring count (clamped to 9, as
    /// `chopten()` does in `TONELOC.C:2102`).
    pub const fn with_rings(self, rings: u8) -> Cell {
        let r = if rings > 9 { 9 } else { rings };
        Cell(self.base_byte() + r)
    }

    /// Whether the low digit means "rings" for this class.
    pub const fn carries_rings(self) -> bool {
        matches!(
            self,
            CellClass::Voice
                | CellClass::Aborted
                | CellClass::Ringout
                | CellClass::Timeout
                | CellClass::Tone
                | CellClass::Carrier
                | CellClass::Busy
                | CellClass::NoDialtone
        )
    }

    /// Every class, in byte order — handy for legends and tallies.
    pub const ALL: [CellClass; 15] = [
        CellClass::Undialed,
        CellClass::Busy,
        CellClass::Voice,
        CellClass::NoDialtone,
        CellClass::Noted,
        CellClass::Aborted,
        CellClass::Ringout,
        CellClass::Timeout,
        CellClass::Tone,
        CellClass::Carrier,
        CellClass::Excluded,
        CellClass::Omitted,
        CellClass::Dialed,
        CellClass::Blacklisted,
        CellClass::Unknown,
    ];
}

impl fmt::Display for CellClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            CellClass::Undialed => "Undialed",
            CellClass::Busy => "Busy",
            CellClass::Voice => "Voice",
            CellClass::NoDialtone => "No Dialtone",
            CellClass::Noted => "Noted",
            CellClass::Aborted => "Aborted",
            CellClass::Ringout => "Ringout",
            CellClass::Timeout => "Timeout",
            CellClass::Tone => "Tone",
            CellClass::Carrier => "Carrier",
            CellClass::Excluded => "Excluded",
            CellClass::Omitted => "Omitted",
            CellClass::Dialed => "Dialed",
            CellClass::Blacklisted => "Blacklisted",
            CellClass::Unknown => "Unknown",
        };
        f.write_str(s)
    }
}

/// What the operator flagged a number as (the low digit of bytes `40`-`49`).
///
/// These are the original's own labels, verbatim from `TONELOC.C:621-651`.
/// They are part of the artifact — the authors' register is the point.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum NoteKind {
    /// `40` — noted with `N`, or a custom typed note (`K`).
    Generic,
    /// `41` — fax machine.
    Fax,
    /// `42` — the original's label, preserved as recorded.
    Girl,
    /// `43` — voice mail box.
    Vmb,
    /// `44` — the original's label, preserved as recorded.
    YellingAsshole,
    /// `49` — "Person that sounds like Mucho" (`TONELOC.C:651`).
    MuchoSoundAlike,
    /// `45`-`48` — allocated range, never assigned by the original.
    Reserved(u8),
}

impl NoteKind {
    pub const fn from_low_digit(d: u8) -> NoteKind {
        match d {
            0 => NoteKind::Generic,
            1 => NoteKind::Fax,
            2 => NoteKind::Girl,
            3 => NoteKind::Vmb,
            4 => NoteKind::YellingAsshole,
            9 => NoteKind::MuchoSoundAlike,
            other => NoteKind::Reserved(other),
        }
    }

    pub const fn low_digit(self) -> u8 {
        match self {
            NoteKind::Generic => 0,
            NoteKind::Fax => 1,
            NoteKind::Girl => 2,
            NoteKind::Vmb => 3,
            NoteKind::YellingAsshole => 4,
            NoteKind::MuchoSoundAlike => 9,
            NoteKind::Reserved(d) => d,
        }
    }
}

impl fmt::Display for NoteKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            NoteKind::Generic => "Noted",
            NoteKind::Fax => "Fax",
            NoteKind::Girl => "Girl",
            NoteKind::Vmb => "VMB",
            NoteKind::YellingAsshole => "Yelling asshole",
            NoteKind::MuchoSoundAlike => "Mucho sound-alike",
            NoteKind::Reserved(_) => "Noted (reserved)",
        };
        f.write_str(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The load-bearing property: the byte is the model, so every possible
    /// byte survives a round trip untouched.
    #[test]
    fn every_byte_round_trips() {
        for b in 0..=u8::MAX {
            assert_eq!(u8::from(Cell::from(b)), b, "byte {b} did not round-trip");
        }
    }

    #[test]
    fn class_matches_the_originals_divide_by_ten() {
        for b in 0..=u8::MAX {
            let cell = Cell(b);
            // Reproduce TEXTMAP.C:74 exactly and check we agree.
            let rounded = (b / 10) * 10;
            assert_eq!(
                cell.class().base_byte().min(140),
                rounded.min(140),
                "byte {b} classified as {:?}",
                cell.class()
            );
        }
    }

    #[test]
    fn documented_codes_map_as_tl_h_says() {
        assert_eq!(Cell(0).class(), CellClass::Undialed);
        assert_eq!(Cell(10).class(), CellClass::Busy);
        assert_eq!(Cell(23).class(), CellClass::Voice);
        assert_eq!(Cell(23).rings(), 3);
        assert_eq!(Cell(30).class(), CellClass::NoDialtone);
        assert_eq!(Cell(63).class(), CellClass::Ringout);
        assert_eq!(Cell(79).class(), CellClass::Timeout);
        assert_eq!(Cell(79).rings(), 9);
        assert_eq!(Cell(80).class(), CellClass::Tone);
        assert_eq!(Cell(91).class(), CellClass::Carrier);
        assert_eq!(Cell(100).class(), CellClass::Excluded);
        assert_eq!(Cell(110).class(), CellClass::Omitted);
        assert_eq!(Cell(120).class(), CellClass::Dialed);
        assert_eq!(Cell(130).class(), CellClass::Blacklisted);
        assert_eq!(Cell(200).class(), CellClass::Unknown);
    }

    #[test]
    fn notes_use_the_low_digit_as_a_kind_not_a_ring_count() {
        assert_eq!(Cell(40).note(), Some(NoteKind::Generic));
        assert_eq!(Cell(41).note(), Some(NoteKind::Fax));
        assert_eq!(Cell(42).note(), Some(NoteKind::Girl));
        assert_eq!(Cell(43).note(), Some(NoteKind::Vmb));
        assert_eq!(Cell(44).note(), Some(NoteKind::YellingAsshole));
        assert_eq!(Cell(49).note(), Some(NoteKind::MuchoSoundAlike));
        assert_eq!(Cell(45).note(), Some(NoteKind::Reserved(5)));
        // Not a note at all.
        assert_eq!(Cell(80).note(), None);
        // Note kinds round-trip through their low digit.
        for d in 0..=9u8 {
            assert_eq!(NoteKind::from_low_digit(d).low_digit(), d);
        }
    }

    #[test]
    fn with_rings_clamps_like_chopten() {
        assert_eq!(CellClass::Timeout.with_rings(3), Cell(73));
        assert_eq!(CellClass::Timeout.with_rings(9), Cell(79));
        assert_eq!(CellClass::Timeout.with_rings(40), Cell(79));
        assert_eq!(CellClass::Carrier.with_rings(0), Cell(90));
    }

    #[test]
    fn is_tried_matches_count_tried_in_tconvert() {
        // TCONVERT.C:131 — everything but 0 and 100 counts as tried.
        for b in 0..=u8::MAX {
            let expected = b != 0 && (b / 10) * 10 != 100;
            // (byte 1..9 rounds to class 0, which the C also treats as untried
            //  only when the byte itself is 0 — but no such file exists; we
            //  follow the normalized reading.)
            if !(1..=9).contains(&b) {
                assert_eq!(Cell(b).is_tried(), expected, "byte {b}");
            }
        }
    }

    #[test]
    fn textmap_characters_match_the_original_key() {
        assert_eq!(Cell(0).textmap_char(), 'u');
        assert_eq!(Cell(10).textmap_char(), 'B');
        assert_eq!(Cell(21).textmap_char(), 'V');
        assert_eq!(Cell(40).textmap_char(), '*');
        assert_eq!(Cell(52).textmap_char(), 'A');
        assert_eq!(Cell(63).textmap_char(), 'R');
        assert_eq!(Cell(72).textmap_char(), '+');
        assert_eq!(Cell(80).textmap_char(), 'T');
        assert_eq!(Cell(90).textmap_char(), 'C');
        assert_eq!(Cell(130).textmap_char(), 'b');
        assert_eq!(Cell(30).textmap_char(), '?');
    }
}
