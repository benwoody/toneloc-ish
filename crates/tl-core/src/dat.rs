//! The ToneLoc `.DAT` scan file.
//!
//! Layout, verified against `TL.H:49-54`, `TONELOC.C:1795-1846` (read) and
//! `TONELOC.C:2144-2145` (write):
//!
//! ```text
//! offset  size  field
//!      0     2  ProductCode  "TL"
//!      2     2  VersionID    u16 little-endian (0x0100 = 1.00)
//!      4     2  Minutes      u16 little-endian, minutes spent scanning
//!      6    10  Extra        reserved, always zero in the wild
//!     16 10000  cells        one byte per number 0000-9999
//! ```
//!
//! Total: exactly 10016 bytes. See `docs/dat-format.md` for the full
//! derivation, including the earlier 0.90 (10010) and 0.95 (10012) layouts.

use crate::cell::{Cell, CellClass};
use std::fmt;

/// Bytes of header preceding the cell array.
pub const HEADER_LEN: usize = 16;
/// One cell per number `0000`-`9999`.
pub const CELL_COUNT: usize = 10_000;
/// Total size of a current-format `.DAT` file.
pub const DAT_LEN: usize = HEADER_LEN + CELL_COUNT;

/// Numbers per ToneMap column — the grid is 100 columns of 100.
pub const GRID_SIDE: usize = 100;

/// The two magic bytes every ToneLoc data file opens with.
pub const PRODUCT_CODE: [u8; 2] = *b"TL";

/// `VersionID` written by ToneLoc 1.00 (`TONELOC.C:65`, `DATVERSION`).
pub const VERSION_1_00: u16 = 0x0100;
/// `VersionID` written by ToneLoc 0.98 (`TCONVERT.C:89`).
pub const VERSION_0_98: u16 = 0x0098;
/// `VersionID` found in `562XXXX.DAT`; 0.99 shipped between the two.
pub const VERSION_0_99: u16 = 0x0099;

/// Size of a 0.90-era data file: a 10-byte header of five `int` counters.
pub const LEGACY_LEN_0_90: usize = 10_010;
/// Size of a 0.95-era data file: a 12-byte header of six `int` counters.
pub const LEGACY_LEN_0_95: usize = 10_012;

/// The 16-byte `.DAT` header (`struct _scan`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct DatHeader {
    /// Always `"TL"`.
    pub product_code: [u8; 2],
    /// BCD-ish version marker: `0x0100` is 1.00.
    pub version_id: u16,
    /// Minutes spent on this scan, accumulated across sessions.
    pub minutes: u16,
    /// Ten reserved bytes. The authors invited suggestions for them
    /// (`TL.H:37-39`); nobody ever took them up on it.
    pub extra: [u8; 10],
}

impl Default for DatHeader {
    /// A fresh header, exactly as `loaddata()` initializes one when the file
    /// does not exist (`TONELOC.C:1810-1814`).
    fn default() -> Self {
        DatHeader {
            product_code: PRODUCT_CODE,
            version_id: VERSION_1_00,
            minutes: 0,
            extra: [0; 10],
        }
    }
}

impl DatHeader {
    fn parse(bytes: &[u8]) -> Result<DatHeader, DatError> {
        debug_assert!(bytes.len() >= HEADER_LEN);
        let product_code = [bytes[0], bytes[1]];
        if product_code != PRODUCT_CODE {
            return Err(DatError::BadProductCode(product_code));
        }
        Ok(DatHeader {
            product_code,
            version_id: u16::from_le_bytes([bytes[2], bytes[3]]),
            minutes: u16::from_le_bytes([bytes[4], bytes[5]]),
            extra: bytes[6..16].try_into().expect("slice is 10 bytes"),
        })
    }

    fn write_into(&self, out: &mut [u8]) {
        out[0..2].copy_from_slice(&self.product_code);
        out[2..4].copy_from_slice(&self.version_id.to_le_bytes());
        out[4..6].copy_from_slice(&self.minutes.to_le_bytes());
        out[6..16].copy_from_slice(&self.extra);
    }

    /// Version rendered the way the original prints it: `1.00`, `0.98`.
    pub fn version_string(&self) -> String {
        let hi = self.version_id >> 8;
        let lo = self.version_id & 0xff;
        format!("{hi}.{lo:02x}")
    }

    /// Whether ToneLoc 1.00 itself would accept this file, or demand you run
    /// `TCONVERT` on it first (`TONELOC.C:1822-1830`).
    pub fn is_current(&self) -> bool {
        self.product_code == PRODUCT_CODE && self.version_id == VERSION_1_00
    }

    /// Scan time as `(hours, minutes)`, as the status line shows it
    /// (`TONELOC.C:969`).
    pub fn time_spent(&self) -> (u16, u16) {
        (self.minutes / 60, self.minutes % 60)
    }
}

/// A parsed scan file: header plus 10,000 results.
#[derive(Clone, PartialEq, Eq)]
pub struct DatFile {
    pub header: DatHeader,
    cells: Box<[Cell; CELL_COUNT]>,
}

impl DatFile {
    /// An untouched scan: valid header, every number undialed.
    pub fn new() -> DatFile {
        DatFile {
            header: DatHeader::default(),
            cells: Box::new([Cell::UNDIALED; CELL_COUNT]),
        }
    }

    /// Parse a `.DAT` file from raw bytes.
    ///
    /// Stricter than the original in one way (we reject short files rather
    /// than reading uninitialized stack) and more forgiving in another: any
    /// `VersionID` with a `"TL"` magic is accepted, so the 0.99-era
    /// `562XXXX.DAT` loads instead of being turned away. Use
    /// [`DatHeader::is_current`] if you need the original's strictness.
    pub fn parse(bytes: &[u8]) -> Result<DatFile, DatError> {
        match bytes.len() {
            DAT_LEN => {}
            LEGACY_LEN_0_90 => {
                return Err(DatError::LegacyFormat {
                    version: "0.90",
                    len: bytes.len(),
                });
            }
            LEGACY_LEN_0_95 => {
                return Err(DatError::LegacyFormat {
                    version: "0.95",
                    len: bytes.len(),
                });
            }
            other => return Err(DatError::BadLength(other)),
        }

        let header = DatHeader::parse(&bytes[..HEADER_LEN])?;

        let mut cells = Box::new([Cell::UNDIALED; CELL_COUNT]);
        for (slot, &b) in cells.iter_mut().zip(&bytes[HEADER_LEN..]) {
            *slot = Cell(b);
        }

        Ok(DatFile { header, cells })
    }

    /// Serialize back to the on-disk representation.
    ///
    /// For any file that came out of [`DatFile::parse`] unmodified, this
    /// reproduces the input byte for byte.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = vec![0u8; DAT_LEN];
        self.header.write_into(&mut out[..HEADER_LEN]);
        for (dst, cell) in out[HEADER_LEN..].iter_mut().zip(self.cells.iter()) {
            *dst = cell.raw();
        }
        out
    }

    /// The result recorded for a number, `0000`-`9999`.
    #[inline]
    pub fn get(&self, number: u16) -> Cell {
        self.cells[number as usize]
    }

    /// Record a result for a number, `0000`-`9999`.
    #[inline]
    pub fn set(&mut self, number: u16, cell: Cell) {
        self.cells[number as usize] = cell;
    }

    /// All 10,000 cells in number order.
    #[inline]
    pub fn cells(&self) -> &[Cell; CELL_COUNT] {
        &self.cells
    }

    /// The cell at a ToneMap grid position.
    ///
    /// The grid is **column-major**: column `x` holds the hundred numbers
    /// `x00`-`x99` running top to bottom, so `0000` is top-left and `9999` is
    /// bottom-right. Confirmed by the plotting loop (`TONEMAP.C:131-137`,
    /// `x = i / 100; y = i - x * 100`) and its inverse `whatnum()`
    /// (`TONEMAP.C:683`, `num = (x/2)*100 + y/2` on doubled pixels).
    #[inline]
    pub fn at(&self, col: usize, row: usize) -> Cell {
        self.cells[col * GRID_SIDE + row]
    }

    /// Tally cells by class — the numbers ToneLoc's stats window showed.
    pub fn stats(&self) -> ScanStats {
        let mut stats = ScanStats::default();
        for cell in self.cells.iter() {
            match cell.class() {
                CellClass::Busy => stats.busys += 1,
                CellClass::Voice => stats.voices += 1,
                CellClass::Ringout => stats.rings += 1,
                CellClass::Tone => stats.tones += 1,
                CellClass::Carrier => stats.carriers += 1,
                _ => {}
            }
            if cell.is_tried() {
                stats.tried += 1;
            }
        }
        stats
    }

    /// Every number recorded as a tone or a carrier, in numeric order —
    /// the part of a scan anyone actually cared about the next morning.
    pub fn hits(&self) -> impl Iterator<Item = (u16, Cell)> + '_ {
        self.cells
            .iter()
            .enumerate()
            .filter(|(_, c)| c.is_hit())
            .map(|(i, c)| (i as u16, *c))
    }
}

impl Default for DatFile {
    fn default() -> Self {
        DatFile::new()
    }
}

impl fmt::Debug for DatFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DatFile")
            .field("header", &self.header)
            .field("stats", &self.stats())
            .finish_non_exhaustive()
    }
}

/// Counts ToneLoc kept while scanning (`TONELOC.C:1830-1843`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScanStats {
    /// Numbers dialed — everything but undialed and excluded.
    pub tried: u32,
    pub busys: u32,
    pub voices: u32,
    /// Ringouts (`MaxRings` reached).
    pub rings: u32,
    pub tones: u32,
    pub carriers: u32,
}

/// Why a `.DAT` file would not parse.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum DatError {
    #[error(
        "not a ToneLoc data file: expected magic \"TL\", found {:?}",
        String::from_utf8_lossy(.0)
    )]
    BadProductCode([u8; 2]),

    #[error(
        "{version} data file ({len} bytes); ToneLoc {version} files must be \
         converted to the 1.00 format before use"
    )]
    LegacyFormat { version: &'static str, len: usize },

    #[error("wrong size for a ToneLoc data file: expected {DAT_LEN} bytes, found {0}")]
    BadLength(usize),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_bytes() -> Vec<u8> {
        let mut v = vec![0u8; DAT_LEN];
        v[0..2].copy_from_slice(b"TL");
        v[2..4].copy_from_slice(&VERSION_1_00.to_le_bytes());
        v[4..6].copy_from_slice(&3363u16.to_le_bytes());
        // A few recognizable results.
        v[HEADER_LEN] = 90; // 0000 -> Carrier, 0 rings
        v[HEADER_LEN + 9999] = 80; // 9999 -> Tone (the classic loop parking spot)
        v[HEADER_LEN + 100] = 73; // 0100 -> Timeout, 3 rings
        v
    }

    #[test]
    fn parses_a_well_formed_file() {
        let dat = DatFile::parse(&sample_bytes()).unwrap();
        assert_eq!(dat.header.product_code, *b"TL");
        assert_eq!(dat.header.version_id, VERSION_1_00);
        assert_eq!(dat.header.minutes, 3363);
        assert_eq!(dat.header.time_spent(), (56, 3));
        assert_eq!(dat.header.version_string(), "1.00");
        assert!(dat.header.is_current());
        assert_eq!(dat.get(0), Cell(90));
        assert_eq!(dat.get(9999), Cell(80));
        assert_eq!(dat.get(100), Cell(73));
        assert_eq!(dat.get(1), Cell::UNDIALED);
    }

    #[test]
    fn round_trips_byte_for_byte() {
        let bytes = sample_bytes();
        let dat = DatFile::parse(&bytes).unwrap();
        assert_eq!(dat.to_bytes(), bytes);
    }

    #[test]
    fn round_trips_every_possible_cell_byte() {
        let mut bytes = sample_bytes();
        // Fill the grid so all 256 byte values appear, including ones the
        // original never wrote.
        for (i, slot) in bytes[HEADER_LEN..].iter_mut().enumerate() {
            *slot = (i % 256) as u8;
        }
        bytes[6..16].copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
        let dat = DatFile::parse(&bytes).unwrap();
        assert_eq!(dat.to_bytes(), bytes, "round trip must preserve every byte");
    }

    #[test]
    fn grid_is_column_major() {
        let mut dat = DatFile::new();
        dat.set(0, Cell(90));
        dat.set(99, Cell(80));
        dat.set(100, Cell(10));
        dat.set(9999, Cell(130));

        // Column 0 holds 0000-0099 top to bottom.
        assert_eq!(dat.at(0, 0), Cell(90)); // top-left     = 0000
        assert_eq!(dat.at(0, 99), Cell(80)); // bottom of col 0 = 0099
        assert_eq!(dat.at(1, 0), Cell(10)); // top of col 1 = 0100
        assert_eq!(dat.at(99, 99), Cell(130)); // bottom-right = 9999
    }

    #[test]
    fn rejects_wrong_magic() {
        let mut bytes = sample_bytes();
        bytes[0] = b'X';
        assert_eq!(
            DatFile::parse(&bytes),
            Err(DatError::BadProductCode([b'X', b'L']))
        );
    }

    #[test]
    fn names_the_legacy_formats_rather_than_just_failing() {
        assert!(matches!(
            DatFile::parse(&vec![0u8; LEGACY_LEN_0_90]),
            Err(DatError::LegacyFormat {
                version: "0.90",
                ..
            })
        ));
        assert!(matches!(
            DatFile::parse(&vec![0u8; LEGACY_LEN_0_95]),
            Err(DatError::LegacyFormat {
                version: "0.95",
                ..
            })
        ));
        assert!(matches!(
            DatFile::parse(&vec![0u8; 512]),
            Err(DatError::BadLength(512))
        ));
    }

    #[test]
    fn stats_match_the_load_time_tally() {
        let mut dat = DatFile::new();
        dat.set(0, Cell(90)); // carrier
        dat.set(1, Cell(91)); // carrier, 1 ring
        dat.set(2, Cell(80)); // tone
        dat.set(3, Cell(10)); // busy
        dat.set(4, Cell(21)); // voice
        dat.set(5, Cell(63)); // ringout
        dat.set(6, Cell(100)); // excluded -> not counted as tried
        let s = dat.stats();
        assert_eq!(s.carriers, 2);
        assert_eq!(s.tones, 1);
        assert_eq!(s.busys, 1);
        assert_eq!(s.voices, 1);
        assert_eq!(s.rings, 1);
        assert_eq!(s.tried, 6);
    }

    #[test]
    fn hits_lists_tones_and_carriers_in_order() {
        let mut dat = DatFile::new();
        dat.set(9999, Cell(80));
        dat.set(1234, Cell(90));
        dat.set(5, Cell(10)); // busy is not a hit
        let hits: Vec<_> = dat.hits().collect();
        assert_eq!(hits, vec![(1234, Cell(90)), (9999, Cell(80))]);
    }
}
