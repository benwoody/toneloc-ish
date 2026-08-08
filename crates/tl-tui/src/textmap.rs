//! `TEXTMAP.EXE`, reproduced.
//!
//! TextMap was a third-party utility ("by The Public") that dumped a `.DAT` to
//! plain text so you could print it or paste it into a message. It is not the
//! ToneMap in letters — it is a different visualization entirely.
//!
//! The ToneMap is a 100 × 100 grid. TextMap is a **linear strip**: every number
//! in order, wrapped at a chosen width, each line labelled with the range it
//! covers. At the default 79 columns that is 69 numbers a line, so the
//! hundred-number blocks do not line up and banding is sheared across rows.
//! Which is precisely why the graphical map was the one people remembered.
//!
//! Reproduced faithfully here because it is an oracle: `TEXTMAP.EXE` still runs
//! under DOSBox, so its output can be diffed against ours character by
//! character. That includes reproducing its off-by-one — see [`render`].

use tl_core::{CELL_COUNT, CellClass, DatFile};

/// How TextMap was invoked (`TEXTMAP.C:93-159`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextMapOptions {
    /// Characters per line, including the 10-character range label (`-c`).
    pub columns: usize,
    /// Print the two-line key first (suppressed by `-k`).
    pub key: bool,
    /// First number to print (`-r`).
    pub begin: u16,
    /// Last number, **exclusive** — see [`render`].
    pub end: u16,
}

impl Default for TextMapOptions {
    fn default() -> Self {
        TextMapOptions {
            columns: 79,
            key: true,
            begin: 0,
            end: 9999,
        }
    }
}

impl TextMapOptions {
    /// Apply the original's own clamps (`TEXTMAP.C:145-147`): no negatives, a
    /// floor of 20 columns, and an end that never exceeds 9999.
    fn clamped(mut self) -> TextMapOptions {
        if self.columns < 20 {
            self.columns = 20;
        }
        if self.end as usize > CELL_COUNT - 1 {
            self.end = (CELL_COUNT - 1) as u16;
        }
        self
    }

    /// Numbers per line: the width less the ten-character label.
    fn per_line(&self) -> usize {
        self.columns - 10
    }
}

/// The two-line key TextMap prints above the map (`TEXTMAP.C:51-52`), verbatim.
pub const KEY: &str = "\
(B)usy (T)one (C)arrier (V)oice (A)borted (+) Timeout (R)ingout (D)ialed\n\
(u)ndialed (o)mitted (x) excluded (n)o dialtone (b)lacklisted  (*) Noted\n";

/// Render a `.DAT` exactly as `TEXTMAP.EXE` would.
///
/// **The last number is not printed.** The loop is `for (x=beginum; x<endnum; x++)`
/// with `endnum` defaulting to 9999 (`TEXTMAP.C:61`), so a default run emits
/// 0000-9998 and silently drops 9999. That is a bug in the original, and an
/// unlucky one: 9999 is exactly where loops tended to be parked — the manual's
/// own example is `836-9998/9999`.
///
/// It is reproduced rather than fixed because this function's purpose is to be
/// diffable against the real binary. [`omits_a_find`] exists so callers can
/// warn about it instead of quietly losing a carrier.
pub fn render(dat: &DatFile, options: TextMapOptions) -> String {
    let opt = options.clamped();
    let mut out = String::with_capacity(12 * 1024);

    if opt.key {
        out.push_str(KEY);
        out.push('\n');
    }

    let mut left = opt.begin as usize;
    let mut right = (left + opt.columns - 11).min(opt.end as usize);
    out.push_str(&format!("{left:04}-{right:04} "));

    let mut count = 0usize;
    for number in (opt.begin as usize)..(opt.end as usize) {
        if count == opt.per_line() {
            out.push('\n');
            left = right + 1;
            right = (left + opt.columns - 11).min(opt.end as usize);
            out.push_str(&format!("{left:04}-{right:04} "));
            count = 0;
        }
        count += 1;
        out.push(dat.cells()[number].textmap_char());
    }

    out
}

/// Whether the number the original drops is a tone or a carrier.
///
/// Lets a caller say "by the way, there is a carrier at 9999 that TextMap will
/// not show you" rather than reproducing the data loss in silence.
pub fn omits_a_find(dat: &DatFile, options: TextMapOptions) -> Option<(u16, CellClass)> {
    let opt = options.clamped();
    let dropped = opt.end;
    if dropped as usize >= CELL_COUNT || dropped < opt.begin {
        return None;
    }
    let cell = dat.get(dropped);
    cell.is_hit().then(|| (dropped, cell.class()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tl_core::CellClass;

    fn line_of(text: &str, n: usize) -> &str {
        text.lines().nth(n).expect("line exists")
    }

    #[test]
    fn default_width_matches_the_originals_79_columns() {
        let dat = DatFile::new();
        let text = render(
            &dat,
            TextMapOptions {
                key: false,
                ..Default::default()
            },
        );
        // 10-character label plus 69 cells.
        let first = line_of(&text, 0);
        assert_eq!(first.chars().count(), 79);
        assert!(first.starts_with("0000-0068 "));
    }

    #[test]
    fn range_labels_advance_by_the_line_length() {
        let dat = DatFile::new();
        let text = render(
            &dat,
            TextMapOptions {
                key: false,
                ..Default::default()
            },
        );
        assert!(line_of(&text, 0).starts_with("0000-0068 "));
        assert!(line_of(&text, 1).starts_with("0069-0137 "));
        assert!(line_of(&text, 2).starts_with("0138-0206 "));
    }

    #[test]
    fn the_original_never_prints_the_last_number() {
        // TEXTMAP.C:61 — `x < endnum`. Faithful, and a real data loss.
        let mut dat = DatFile::new();
        dat.set(9999, CellClass::Carrier.with_rings(0));
        dat.set(9998, CellClass::Tone.with_rings(0));

        let text = render(
            &dat,
            TextMapOptions {
                key: false,
                ..Default::default()
            },
        );
        let cells: String = text
            .lines()
            .map(|l| l.chars().skip(10).collect::<String>())
            .collect();

        assert_eq!(cells.chars().count(), 9999, "0000-9998 inclusive");
        assert!(cells.ends_with('T'), "9998's tone should be the last cell");
        assert!(
            !cells.contains('C'),
            "9999's carrier is dropped by the original"
        );
    }

    #[test]
    fn the_dropped_number_can_be_reported_rather_than_lost_silently() {
        let mut dat = DatFile::new();
        dat.set(9999, CellClass::Carrier.with_rings(0));
        assert_eq!(
            omits_a_find(&dat, TextMapOptions::default()),
            Some((9999, CellClass::Carrier))
        );

        // Nothing interesting there: nothing to warn about.
        let quiet = DatFile::new();
        assert_eq!(omits_a_find(&quiet, TextMapOptions::default()), None);
    }

    #[test]
    fn the_key_is_the_originals_two_lines_verbatim() {
        let dat = DatFile::new();
        let text = render(&dat, TextMapOptions::default());
        assert!(text.starts_with(KEY));
        // Key, then a blank line, then the map.
        assert_eq!(line_of(&text, 2), "");
        assert!(line_of(&text, 3).starts_with("0000-0068 "));
    }

    #[test]
    fn suppressing_the_key_starts_straight_at_the_map() {
        let dat = DatFile::new();
        let text = render(
            &dat,
            TextMapOptions {
                key: false,
                ..Default::default()
            },
        );
        assert!(text.starts_with("0000-0068 "));
    }

    #[test]
    fn narrow_widths_are_clamped_to_twenty_like_the_original() {
        let dat = DatFile::new();
        let text = render(
            &dat,
            TextMapOptions {
                columns: 5,
                key: false,
                ..Default::default()
            },
        );
        assert_eq!(line_of(&text, 0).chars().count(), 20);
    }

    #[test]
    fn a_hundred_and_ten_columns_lines_the_grid_up_by_column() {
        // 100 cells a line means each line is exactly one ToneMap column —
        // the text map is the grid's transpose.
        let mut dat = DatFile::new();
        // Column 1 is numbers 0100-0199; fill it with tones.
        for row in 0..100u16 {
            dat.set(100 + row, CellClass::Tone.with_rings(0));
        }
        let text = render(
            &dat,
            TextMapOptions {
                columns: 110,
                key: false,
                ..Default::default()
            },
        );
        let second = line_of(&text, 1);
        assert!(second.starts_with("0100-0199 "));
        assert!(
            second.chars().skip(10).all(|c| c == 'T'),
            "the whole line should be one filled ToneMap column"
        );
    }

    #[test]
    fn a_restricted_range_prints_only_that_range() {
        let dat = DatFile::new();
        let text = render(
            &dat,
            TextMapOptions {
                begin: 9000,
                end: 9100,
                key: false,
                ..Default::default()
            },
        );
        assert!(text.starts_with("9000-9068 "));
        let cells: usize = text.lines().map(|l| l.chars().skip(10).count()).sum();
        assert_eq!(cells, 100, "9000..9100 exclusive");
    }

    #[test]
    fn every_class_renders_as_the_originals_letter() {
        let mut dat = DatFile::new();
        for (i, class) in [
            CellClass::Busy,
            CellClass::Tone,
            CellClass::Carrier,
            CellClass::Voice,
            CellClass::Aborted,
            CellClass::Timeout,
            CellClass::Ringout,
            CellClass::Noted,
            CellClass::Blacklisted,
        ]
        .into_iter()
        .enumerate()
        {
            dat.set(i as u16, class.with_rings(0));
        }
        let text = render(
            &dat,
            TextMapOptions {
                key: false,
                ..Default::default()
            },
        );
        let cells: String = line_of(&text, 0).chars().skip(10).collect();
        assert!(cells.starts_with("BTCVA+R*b"));
    }
}
