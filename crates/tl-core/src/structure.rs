//! Reading a ToneMap: measuring the patterns the authors described in words.
//!
//! `SAMPLES.DOC`, shipped with the original, is the authors annotating their
//! own scans — "a generic residential exchange", "a split exchange, 0-4000 is
//! residential, business from 4000-9999", "an exchange with many carriers in
//! one band". Those are structural claims about real data, and they turn out
//! to be measurable.
//!
//! That matters for more than curiosity. The synthetic exchange has to produce
//! the *kinds* of pattern real scanners saw, or it is only coloured noise. This
//! module is the yardstick it will be held to, calibrated against fourteen real
//! scans and the authors' own labels for them — built before the generator, so
//! the generator cannot be graded against a standard invented to flatter it.
//!
//! Everything here is pure counting over a [`DatFile`]. No I/O, no heuristics
//! beyond thresholds that are stated, cited and testable.

use crate::cell::CellClass;
use crate::dat::{DatFile, GRID_SIDE};

/// A range of ToneMap columns. Column `c` is the hundred numbers `c00`-`c99`,
/// so a column range is a range of number blocks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Columns {
    pub start: usize,
    pub end: usize,
}

impl Columns {
    /// All hundred columns — the whole prefix.
    pub const ALL: Columns = Columns {
        start: 0,
        end: GRID_SIDE,
    };

    pub fn new(start: usize, end: usize) -> Columns {
        Columns {
            start: start.min(GRID_SIDE),
            end: end.min(GRID_SIDE),
        }
    }

    /// The columns covering a range of numbers, e.g. `4000..10000`.
    pub fn for_numbers(start: u16, end: u16) -> Columns {
        Columns::new(start as usize / GRID_SIDE, end as usize / GRID_SIDE)
    }

    pub fn len(&self) -> usize {
        self.end.saturating_sub(self.start)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// What kind of exchange a region looks like.
///
/// The manual's own contrast: residential prefixes show "even distribution, no
/// pattern", business exchanges show "strings or clusters of modems" and bands
/// where a PBX owns a contiguous DID range.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Character {
    /// Even, structureless. Numbers handed out one household at a time.
    Residential,
    /// Banded. Blocks allocated to businesses in contiguous runs.
    Business,
    /// Structure present but not dominant.
    Mixed,
    /// Too little of the region was dialed to say anything.
    ///
    /// An abandoned scan is not an even one. Without this, a file with a
    /// single dialed number scores zero banding and reads as textbook
    /// residential — confidently, and about nothing.
    Unscanned,
}

/// Measurements of one region of a scan.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Profile {
    /// Columns measured.
    pub columns: Columns,
    /// How strongly results clump by column, `0.0` (even) upward.
    /// See [`Profile::banding`].
    pub banding: f64,
    pub voice: f64,
    pub busy: f64,
    pub ringout: f64,
    pub timeout: f64,
    pub carrier: f64,
    pub tone: f64,
    /// The most carrier-dense single column, as a fraction of that column.
    pub peak_carrier_column: f64,
    /// Which column that was.
    pub peak_carrier_index: usize,
    /// Fraction of the region actually dialed. Everything else here is only
    /// meaningful to the extent this is high.
    pub coverage: f64,
}

impl Profile {
    /// Banding score below which a region reads as evenly distributed.
    ///
    /// Calibrated on the archive: SAMPLE1 ("a generic residential exchange")
    /// scores 0.080, and the residential half of SAMPLE2 scores 0.046.
    pub const RESIDENTIAL_BANDING: f64 = 0.10;

    /// Banding score above which a region reads as block-allocated.
    ///
    /// SAMPLE4 ("wider bands") scores 0.331, SAMPLE5 0.219, and the business
    /// half of SAMPLE2 scores 0.194.
    pub const BUSINESS_BANDING: f64 = 0.19;

    /// Least fraction of a region that must be dialed before its shape means
    /// anything. A partial scan can look like anything at all.
    pub const MIN_COVERAGE: f64 = 0.5;

    /// Classify the region from its banding score.
    pub fn character(&self) -> Character {
        if self.coverage < Self::MIN_COVERAGE {
            Character::Unscanned
        } else if self.banding < Self::RESIDENTIAL_BANDING {
            Character::Residential
        } else if self.banding >= Self::BUSINESS_BANDING {
            Character::Business
        } else {
            Character::Mixed
        }
    }

    /// Whether carriers clump into a band rather than scattering.
    ///
    /// SAMPLE11 — "an exchange with many carriers in one band" — has a column
    /// that is 92% carriers. A scattered exchange never approaches that.
    pub fn has_carrier_band(&self) -> bool {
        self.peak_carrier_column >= 0.25
    }
}

/// Fraction of cells in `columns` belonging to `class`.
pub fn class_fraction(dat: &DatFile, columns: Columns, class: CellClass) -> f64 {
    if columns.is_empty() {
        return 0.0;
    }
    let mut hits = 0usize;
    for col in columns.start..columns.end {
        for row in 0..GRID_SIDE {
            if dat.at(col, row).class() == class {
                hits += 1;
            }
        }
    }
    hits as f64 / (columns.len() * GRID_SIDE) as f64
}

/// Fraction of one column belonging to `class`.
pub fn column_fraction(dat: &DatFile, col: usize, class: CellClass) -> f64 {
    if col >= GRID_SIDE {
        return 0.0;
    }
    let hits = (0..GRID_SIDE)
        .filter(|&row| dat.at(col, row).class() == class)
        .count();
    hits as f64 / GRID_SIDE as f64
}

/// Measure a region.
pub fn profile(dat: &DatFile, columns: Columns) -> Profile {
    let (peak_carrier_index, peak_carrier_column) = (columns.start..columns.end)
        .map(|c| (c, column_fraction(dat, c, CellClass::Carrier)))
        .fold(
            (0usize, 0.0f64),
            |best, cur| {
                if cur.1 > best.1 { cur } else { best }
            },
        );

    Profile {
        columns,
        banding: banding(dat, columns),
        voice: class_fraction(dat, columns, CellClass::Voice),
        busy: class_fraction(dat, columns, CellClass::Busy),
        ringout: class_fraction(dat, columns, CellClass::Ringout),
        timeout: class_fraction(dat, columns, CellClass::Timeout),
        carrier: class_fraction(dat, columns, CellClass::Carrier),
        tone: class_fraction(dat, columns, CellClass::Tone),
        peak_carrier_column,
        peak_carrier_index,
        coverage: coverage(dat, columns),
    }
}

/// Fraction of a region that was actually dialed.
///
/// "Tried" is the original's own definition: everything except undialed and
/// excluded (`TCONVERT.C:126-134`).
pub fn coverage(dat: &DatFile, columns: Columns) -> f64 {
    if columns.is_empty() {
        return 0.0;
    }
    let mut tried = 0usize;
    for col in columns.start..columns.end {
        for row in 0..GRID_SIDE {
            if dat.at(col, row).is_tried() {
                tried += 1;
            }
        }
    }
    tried as f64 / (columns.len() * GRID_SIDE) as f64
}

/// How strongly results clump by column.
///
/// The population standard deviation, across columns, of each column's
/// "allocated but nobody home" fraction — busy plus ringout. Those two are the
/// signature of a block assigned to an organization: the numbers exist and the
/// switch routes them, but no person answers.
///
/// A residential prefix spreads such numbers evenly and scores near zero. A
/// business exchange concentrates them into DID ranges, so per-column
/// fractions swing between nearly none and nearly all, and the score rises.
///
/// This is deliberately *not* a fit to any particular band shape. The authors
/// noted bands that do not align to hundreds ("hunt and DID groups do not fill
/// even bands of 100", SAMPLE6/7), so anything assuming tidy block edges would
/// miss the messier real cases.
pub fn banding(dat: &DatFile, columns: Columns) -> f64 {
    if columns.len() < 2 {
        return 0.0;
    }
    let fractions: Vec<f64> = (columns.start..columns.end)
        .map(|c| {
            column_fraction(dat, c, CellClass::Busy) + column_fraction(dat, c, CellClass::Ringout)
        })
        .collect();

    let n = fractions.len() as f64;
    let mean = fractions.iter().sum::<f64>() / n;
    let variance = fractions.iter().map(|f| (f - mean).powi(2)).sum::<f64>() / n;
    variance.sqrt()
}

/// Fraction of `class` within a band of *rows* — the last two digits of the
/// number, across every hundred-block.
///
/// Useful for the gradient the authors noticed in SAMPLE12: "notice how this
/// exchange fades off towards the bottom in places... perhaps low numbers are
/// allocated first?"
pub fn row_band_fraction(dat: &DatFile, rows: std::ops::Range<usize>, class: CellClass) -> f64 {
    let rows = rows.start.min(GRID_SIDE)..rows.end.min(GRID_SIDE);
    if rows.is_empty() {
        return 0.0;
    }
    let count = rows.len() * GRID_SIDE;
    let mut hits = 0usize;
    for col in 0..GRID_SIDE {
        for row in rows.clone() {
            if dat.at(col, row).class() == class {
                hits += 1;
            }
        }
    }
    hits as f64 / count as f64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Cell;

    /// Build a scan where every column is identical — perfectly even.
    fn even_scan() -> DatFile {
        let mut dat = DatFile::new();
        for col in 0..GRID_SIDE {
            for row in 0..GRID_SIDE {
                // Same mix in every column: a third busy, two thirds voice.
                let cell = if row % 3 == 0 { Cell(10) } else { Cell(20) };
                dat.set((col * GRID_SIDE + row) as u16, cell);
            }
        }
        dat
    }

    /// Build a scan with hard bands: half the columns all busy, half all voice.
    fn banded_scan() -> DatFile {
        let mut dat = DatFile::new();
        for col in 0..GRID_SIDE {
            let cell = if col < 50 { Cell(10) } else { Cell(20) };
            for row in 0..GRID_SIDE {
                dat.set((col * GRID_SIDE + row) as u16, cell);
            }
        }
        dat
    }

    #[test]
    fn an_even_scan_has_no_banding_and_reads_residential() {
        let p = profile(&even_scan(), Columns::ALL);
        assert!(p.banding < 1e-9, "banding was {}", p.banding);
        assert_eq!(p.character(), Character::Residential);
    }

    #[test]
    fn a_hard_banded_scan_scores_high_and_reads_business() {
        let p = profile(&banded_scan(), Columns::ALL);
        // Half the columns at 1.0, half at 0.0 — the maximum possible spread.
        assert!((p.banding - 0.5).abs() < 1e-9, "banding was {}", p.banding);
        assert_eq!(p.character(), Character::Business);
    }

    #[test]
    fn banding_is_blind_to_how_much_there_is_only_to_how_it_clumps() {
        // Doubling the busy fraction evenly must not read as more structure.
        let mut dat = DatFile::new();
        for col in 0..GRID_SIDE {
            for row in 0..GRID_SIDE {
                let cell = if row % 2 == 0 { Cell(10) } else { Cell(20) };
                dat.set((col * GRID_SIDE + row) as u16, cell);
            }
        }
        let p = profile(&dat, Columns::ALL);
        assert!(p.banding < 1e-9);
        assert!((p.busy - 0.5).abs() < 1e-9);
    }

    #[test]
    fn columns_map_onto_number_ranges() {
        let c = Columns::for_numbers(4000, 10000);
        assert_eq!(c.start, 40);
        assert_eq!(c.end, 100);
        assert_eq!(c.len(), 60);
        assert_eq!(Columns::ALL.len(), 100);
    }

    #[test]
    fn a_carrier_band_is_detected_and_located() {
        let mut dat = DatFile::new();
        // Column 98 = numbers 9800-9899, nearly all carriers.
        for row in 0..90 {
            dat.set((98 * GRID_SIDE + row) as u16, Cell(90));
        }
        // A few carriers scattered elsewhere.
        for col in 0..20 {
            dat.set((col * GRID_SIDE) as u16, Cell(90));
        }
        let p = profile(&dat, Columns::ALL);
        assert!(p.has_carrier_band());
        assert_eq!(p.peak_carrier_index, 98);
        assert!((p.peak_carrier_column - 0.9).abs() < 1e-9);
    }

    #[test]
    fn scattered_carriers_are_not_a_band() {
        let mut dat = DatFile::new();
        for col in 0..GRID_SIDE {
            dat.set((col * GRID_SIDE) as u16, Cell(90));
        }
        let p = profile(&dat, Columns::ALL);
        assert!(!p.has_carrier_band(), "1% per column is not a band");
    }

    #[test]
    fn a_region_can_be_measured_independently_of_the_whole() {
        // Residential lower half, business upper half — SAMPLE2's shape.
        let mut dat = DatFile::new();
        for col in 0..GRID_SIDE {
            for row in 0..GRID_SIDE {
                let cell = if col < 40 {
                    if row % 3 == 0 { Cell(10) } else { Cell(20) }
                } else if col % 2 == 0 {
                    Cell(10)
                } else {
                    Cell(20)
                };
                dat.set((col * GRID_SIDE + row) as u16, cell);
            }
        }
        let lower = profile(&dat, Columns::new(0, 40));
        let upper = profile(&dat, Columns::new(40, 100));
        assert_eq!(lower.character(), Character::Residential);
        assert_eq!(upper.character(), Character::Business);
        assert!(upper.banding > lower.banding);
    }

    #[test]
    fn a_barely_dialed_scan_is_unscanned_not_residential() {
        // 562XXXX.DAT is this case: one number dialed, then the operator quit.
        // Its banding is a perfect 0.000, which means nothing whatsoever.
        let mut dat = DatFile::new();
        dat.set(9490, Cell(50));

        let p = profile(&dat, Columns::ALL);
        assert!(p.banding < 1e-9, "an empty grid has no variance");
        assert_eq!(
            p.character(),
            Character::Unscanned,
            "coverage was {:.4}",
            p.coverage
        );
    }

    #[test]
    fn coverage_ignores_undialed_and_excluded() {
        let mut dat = DatFile::new();
        for n in 0..5_000u16 {
            dat.set(n, Cell(20)); // voice — dialed
        }
        for n in 5_000..7_000u16 {
            dat.set(n, Cell(100)); // excluded — never dialed
        }
        let c = coverage(&dat, Columns::ALL);
        assert!((c - 0.5).abs() < 1e-9, "coverage was {c}");
        // Above the threshold, so it classifies rather than abstaining.
        assert_ne!(
            profile(&dat, Columns::ALL).character(),
            Character::Unscanned
        );
    }

    #[test]
    fn empty_regions_do_not_divide_by_zero() {
        let dat = DatFile::new();
        let empty = Columns::new(50, 50);
        assert_eq!(banding(&dat, empty), 0.0);
        assert_eq!(class_fraction(&dat, empty, CellClass::Voice), 0.0);
        assert_eq!(column_fraction(&dat, 500, CellClass::Voice), 0.0);
        assert_eq!(row_band_fraction(&dat, 10..10, CellClass::Voice), 0.0);
    }

    #[test]
    fn row_bands_read_the_last_two_digits_across_every_block() {
        let mut dat = DatFile::new();
        // Mark row 99 of every column: numbers x99.
        for col in 0..GRID_SIDE {
            dat.set((col * GRID_SIDE + 99) as u16, Cell(80));
        }
        assert!((row_band_fraction(&dat, 99..100, CellClass::Tone) - 1.0).abs() < 1e-9);
        assert_eq!(row_band_fraction(&dat, 0..99, CellClass::Tone), 0.0);
    }
}
