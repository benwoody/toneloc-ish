//! ToneLoc's screen, rebuilt.
//!
//! This is the thing you actually stared at: three windows on an 80 × 25 DOS
//! text screen, running all night. Geometry, contents and colours are taken
//! from `TONELOC.C:1280-1322`, not invented.
//!
//! ```text
//!  cols 0-44                          46-79
//!  ┌──────────────────────────────┐  ┌────────────────┐  rows 0-8
//!  │ ┤ Activity Log ├             │  │ ┤ Modem ├      │
//!  │                              │  └────────────────┘
//!  │                              │  ┌────────────────┐  rows 9-22
//!  │                              │  │ ┤ Statistics ├ │
//!  └──────────────────────────────┘  └────────────────┘  rows 0-22 / 9-22
//!           ToneLoc 1.10 by Minor Threat & Mucho Maas     row 23
//! ```
//!
//! Everything here is a pure function of [`ScreenState`], so the scan engine
//! can drive it later without this module changing.

use std::fmt::Write as _;
use tl_core::DosColor;

/// Screen dimensions. DOS text mode, and not negotiable — the layout below is
/// full of coordinates like `numrows-16` that only work at 25 rows.
pub const COLS: usize = 80;
pub const ROWS: usize = 25;

/// Length of the progress meter (`TONELOC.C:67`, `METERLENGTH`).
pub const METER_LENGTH: usize = 30;

/// The version this screen reproduces (`TONELOC.C:62`).
pub const VERSION: &str = "1.10";

/// Sits under the original credit line, in a quieter colour. Not part of the
/// reproduction — the line above it is.
pub const RECONSTRUCTION_CREDIT: &str = "(toneloc-ish by bendoubleu)";

/// CP437 `0xDB`, the meter's filled cell (`TLCFG.C`, `cfg.meterfront`).
const METER_FULL: char = '█';
/// CP437 `0xB1`, the meter's empty cell (`cfg.meterback`).
const METER_EMPTY: char = '▒';

// Default colours, from `TLCFG.C:1255-1268`.
const ACT_WIN: DosColor = DosColor::Cyan; // cfg.act_win    = 3
const ACT_TEXT: DosColor = DosColor::LightBlue; // cfg.act_text   = 9
const MOD_WIN: DosColor = DosColor::White; // cfg.mod_win    = 15
const MOD_TEXT: DosColor = DosColor::Green; // cfg.mod_text   = 2
const STATS_WIN: DosColor = DosColor::LightCyan; // cfg.stats_win  = 11
const STATS_ITEMS: DosColor = DosColor::Cyan; // cfg.stats_items = 3
const STATS_TEXT: DosColor = DosColor::White; // cfg.stats_text = 15
const METER_BACK: DosColor = DosColor::Blue; // cfg.meter_back = 1
const METER_FORE: DosColor = DosColor::Yellow; // cfg.meter_fore = 14
const TONE_TEXT: DosColor = DosColor::LightGreen; // cfg.tone_text  = 10
const CARRIER_TEXT: DosColor = DosColor::LightRed; // cfg.carrier_text = 12

/// What ToneLoc was scanning for. Changes one label: the stats window says
/// `Tones` or `CD's` (`TONELOC.C:1312` — "haha, there's mud in yer eye!").
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ScanType {
    Tones,
    #[default]
    Carriers,
}

impl ScanType {
    fn label(self) -> &'static str {
        match self {
            ScanType::Tones => "Tones ",
            ScanType::Carriers => "CD's  ",
        }
    }
}

/// Everything the screen displays.
#[derive(Clone, Debug, Default)]
pub struct ScreenState {
    pub mask: String,
    pub scan_type: ScanType,
    /// `HH:MM:SS` when the scan began.
    pub started: String,
    /// `HH:MM:SS` now.
    pub current: String,
    pub max_dials: u32,
    pub dials_per_hour: u32,
    /// Estimated time to finish, e.g. `4:12`.
    pub eta: String,
    /// Rings so far on the current call, e.g. `2/4`.
    pub ring: String,
    /// Seconds spent on the current call.
    pub secs: u32,
    /// Tones or carriers found, per `scan_type`.
    pub found_count: u32,
    pub voice: u32,
    pub busy: u32,
    pub rings: u32,
    pub tried: u32,
    /// Numbers in the Found window, most recent last.
    pub found: Vec<String>,
    /// Activity log lines, most recent last.
    pub activity: Vec<String>,
    /// Modem window lines, most recent last.
    pub modem: Vec<String>,
}

/// One character cell.
#[derive(Clone, Copy)]
struct Cell {
    ch: char,
    fg: DosColor,
}

impl Default for Cell {
    fn default() -> Self {
        Cell {
            ch: ' ',
            fg: DosColor::LightGray,
        }
    }
}

struct Buffer {
    cells: Vec<Cell>,
    /// Cells drawn with a non-black background: only the meter.
    meter_bg: Vec<Option<DosColor>>,
}

impl Buffer {
    fn new() -> Buffer {
        Buffer {
            cells: vec![Cell::default(); COLS * ROWS],
            meter_bg: vec![None; COLS * ROWS],
        }
    }

    fn put(&mut self, row: usize, col: usize, ch: char, fg: DosColor) {
        if row < ROWS && col < COLS {
            self.cells[row * COLS + col] = Cell { ch, fg };
        }
    }

    fn text(&mut self, row: usize, col: usize, s: &str, fg: DosColor) {
        for (i, ch) in s.chars().enumerate() {
            self.put(row, col + i, ch, fg);
        }
    }

    /// A double-line box with a CXL-style `┤ title ├` centred on the top edge.
    fn window(
        &mut self,
        top: usize,
        left: usize,
        bottom: usize,
        right: usize,
        title: &str,
        fg: DosColor,
    ) {
        let width = right - left + 1;
        for c in left..=right {
            self.put(top, c, '═', fg);
            self.put(bottom, c, '═', fg);
        }
        for r in top + 1..bottom {
            self.put(r, left, '║', fg);
            self.put(r, right, '║', fg);
        }
        self.put(top, left, '╔', fg);
        self.put(top, right, '╗', fg);
        self.put(bottom, left, '╚', fg);
        self.put(bottom, right, '╝', fg);

        if !title.is_empty() {
            let label = format!("┤ {title} ├");
            let len = label.chars().count();
            let start = left + width.saturating_sub(len) / 2;
            self.text(top, start, &label, fg);
        }
    }

    fn hline(&mut self, row: usize, left: usize, right: usize, fg: DosColor) {
        for c in left..=right {
            self.put(row, c, '─', fg);
        }
        self.put(row, left, '╟', fg);
        self.put(row, right, '╢', fg);
    }

    fn render_rows(&self) -> Vec<String> {
        (0..ROWS)
            .map(|row| {
                let mut out = String::with_capacity(COLS * 4);
                let mut last: Option<(DosColor, Option<DosColor>)> = None;
                for col in 0..COLS {
                    let cell = self.cells[row * COLS + col];
                    let bg = self.meter_bg[row * COLS + col];
                    if last != Some((cell.fg, bg)) {
                        let (r, g, b) = cell.fg.rgb();
                        let _ = write!(out, "\x1b[38;2;{r};{g};{b}m");
                        match bg {
                            Some(c) => {
                                let (r, g, b) = c.rgb();
                                let _ = write!(out, "\x1b[48;2;{r};{g};{b}m");
                            }
                            None => out.push_str("\x1b[49m"),
                        }
                        last = Some((cell.fg, bg));
                    }
                    out.push(cell.ch);
                }
                out.push_str("\x1b[0m");
                out
            })
            .collect()
    }
}

/// Render the screen as 25 separate rows, each a string of ANSI escapes with
/// **no trailing newline**.
///
/// This is what a live redraw wants. Writing 80 characters into an 80-column
/// terminal leaves the cursor pending at the margin, and a newline after the
/// last row scrolls the whole display — so a live loop must position the
/// cursor per row and never emit a newline at all.
pub fn render_rows(state: &ScreenState) -> Vec<String> {
    build(state).render_rows()
}

/// Render the full 80 × 25 screen as one newline-separated string.
pub fn render(state: &ScreenState) -> String {
    let mut out = render_rows(state).join("\n");
    out.push('\n');
    out
}

fn build(state: &ScreenState) -> Buffer {
    let mut buf = Buffer::new();

    // --- Activity Log: rows 0-22, cols 0-44 (TONELOC.C:1280) ---------------
    buf.window(0, 0, 22, 44, "Activity Log", ACT_WIN);
    let act_rows = 21; // inner rows 1..=21
    let start = state.activity.len().saturating_sub(act_rows);
    for (i, line) in state.activity[start..].iter().enumerate() {
        buf.text(1 + i, 2, &clip(line, 42), ACT_TEXT);
    }

    // --- Modem: rows 0-8, cols 46-79 (TONELOC.C:1286) ----------------------
    buf.window(0, 46, 8, 79, "Modem", MOD_WIN);
    let mod_rows = 7;
    let start = state.modem.len().saturating_sub(mod_rows);
    for (i, line) in state.modem[start..].iter().enumerate() {
        buf.text(1 + i, 48, &clip(line, 31), MOD_TEXT);
    }

    // --- Statistics: rows 9-22, cols 46-79 (TONELOC.C:1290) ----------------
    buf.window(9, 46, 22, 79, "Statistics", STATS_WIN);

    // Four label/value lines (TONELOC.C:1299-1306).
    buf.text(10, 48, " Started:", STATS_ITEMS);
    buf.text(10, 58, &state.started, STATS_TEXT);
    buf.text(10, 68, "Ring:", STATS_ITEMS);
    buf.text(10, 74, &state.ring, STATS_TEXT);

    buf.text(11, 48, " Current:", STATS_ITEMS);
    buf.text(11, 58, &state.current, STATS_TEXT);
    buf.text(11, 68, "Secs:", STATS_ITEMS);
    buf.text(11, 74, &format!("{:>4}", state.secs), STATS_TEXT);

    buf.text(12, 48, " Max Dials:", STATS_ITEMS);
    buf.text(12, 60, &format!("{:>5}", state.max_dials), STATS_TEXT);

    buf.text(13, 48, " Dials/Hour:", STATS_ITEMS);
    buf.text(13, 60, &format!("{:>5}", state.dials_per_hour), STATS_TEXT);
    buf.text(13, 68, "ETA:", STATS_ITEMS);
    buf.text(13, 73, &state.eta, STATS_TEXT);

    // Divider with the Found heading (TONELOC.C:1292-1295). The original puts
    // the heading four columns right of the vertical rule, not two:
    //
    //     wvline(4,15,7,0,cfg.stats_win);
    //     wprints(4,19,cfg.stats_win,"┤ Found ├");
    //
    // and the rule is seven rows tall, spanning both horizontal dividers
    // rather than floating between them.
    buf.hline(14, 46, 79, STATS_WIN);
    buf.text(14, 67, "┤ Found ├", STATS_WIN);

    // Counters, left of the vertical rule at col 63 (w_stats / w_found).
    let counters = [
        (state.scan_type.label(), state.found_count),
        ("Voice ", state.voice),
        ("Busy  ", state.busy),
        ("Rings ", state.rings),
        ("Try # ", state.tried),
    ];
    for (i, (label, value)) in counters.iter().enumerate() {
        buf.text(15 + i, 48, label, STATS_ITEMS);
        buf.text(15 + i, 54, ":", STATS_ITEMS);
        buf.text(15 + i, 56, &format!("{value:>6}"), STATS_TEXT);
    }
    for r in 15..=19 {
        buf.put(r, 63, '║', STATS_WIN);
    }
    // Join the rule to the dividers above and below it. Drawn after both
    // hlines so the junctions survive.
    buf.put(14, 63, '╥', STATS_WIN);

    // Found numbers, colour-coded the way the original did (TLCFG.C:1259-1260).
    let found_color = match state.scan_type {
        ScanType::Tones => TONE_TEXT,
        ScanType::Carriers => CARRIER_TEXT,
    };
    let start = state.found.len().saturating_sub(5);
    for (i, number) in state.found[start..].iter().enumerate() {
        buf.text(15 + i, 65, &clip(number, 14), found_color);
    }

    buf.hline(20, 46, 79, STATS_WIN);
    buf.put(20, 63, '╨', STATS_WIN);

    // --- Meter: row 21, col 48, 30 cells (TONELOC.C:1320) ------------------
    let filled = meter_cells(state.tried, state.max_dials);
    for i in 0..METER_LENGTH {
        let (ch, fg, bg) = if i < filled {
            (METER_FULL, METER_FORE, METER_BACK)
        } else {
            (METER_EMPTY, METER_BACK, DosColor::Black)
        };
        buf.put(21, 48 + i, ch, fg);
        buf.meter_bg[21 * COLS + 48 + i] = Some(bg);
    }

    // --- Copyright: row 23, centred on column 40 (TONELOC.C:1322) ----------
    let copyright = format!("ToneLoc {VERSION} by Minor Threat & Mucho Maas");
    let col = 40usize.saturating_sub(copyright.chars().count() / 2);
    buf.text(23, col, &copyright, DosColor::LightMagenta);

    // Our own line, below theirs and deliberately quieter. The design on this
    // screen is theirs; this only says who rebuilt it.
    let col = 40usize.saturating_sub(RECONSTRUCTION_CREDIT.chars().count() / 2);
    buf.text(24, col, RECONSTRUCTION_CREDIT, DosColor::DarkGray);

    buf
}

/// How many meter cells are lit (`TONELOC.C:1526-1531`).
fn meter_cells(tried: u32, total: u32) -> usize {
    if total == 0 {
        return 0;
    }
    let filled = (tried as u64 * METER_LENGTH as u64 / total as u64) as usize;
    filled.min(METER_LENGTH)
}

fn clip(s: &str, width: usize) -> String {
    s.chars().take(width).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(state: &ScreenState) -> Vec<String> {
        let text = render(state);
        text.lines()
            .map(|l| {
                let mut out = String::new();
                let mut chars = l.chars();
                while let Some(c) = chars.next() {
                    if c == '\x1b' {
                        for c in chars.by_ref() {
                            if c == 'm' {
                                break;
                            }
                        }
                    } else {
                        out.push(c);
                    }
                }
                out
            })
            .collect()
    }

    fn demo() -> ScreenState {
        ScreenState {
            mask: "562XXXX".into(),
            scan_type: ScanType::Carriers,
            started: "22:50:05".into(),
            current: "23:41:17".into(),
            max_dials: 10_000,
            dials_per_hour: 265,
            eta: "4:12".into(),
            ring: "2/4".into(),
            secs: 12,
            found_count: 77,
            voice: 3425,
            busy: 1189,
            rings: 2923,
            tried: 5000,
            found: vec!["5620082".into(), "5620127".into()],
            activity: vec!["22:50:10 5629490 - Timeout (2)".into()],
            modem: vec!["ATDT5629490".into(), "NO CARRIER".into()],
        }
    }

    #[test]
    fn the_screen_is_exactly_eighty_by_twenty_five() {
        let lines = plain(&demo());
        assert_eq!(lines.len(), ROWS);
        for (i, l) in lines.iter().enumerate() {
            assert_eq!(
                l.chars().count(),
                COLS,
                "row {i} was {} wide",
                l.chars().count()
            );
        }
    }

    #[test]
    fn the_three_windows_sit_where_the_source_puts_them() {
        let lines = plain(&demo());

        // Activity Log spans cols 0-44, rows 0-22.
        assert_eq!(lines[0].chars().next(), Some('╔'));
        assert_eq!(lines[0].chars().nth(44), Some('╗'));
        assert_eq!(lines[22].chars().next(), Some('╚'));
        assert!(lines[0].contains("┤ Activity Log ├"));

        // Modem spans cols 46-79, rows 0-8.
        assert_eq!(lines[0].chars().nth(46), Some('╔'));
        assert_eq!(lines[0].chars().nth(79), Some('╗'));
        assert_eq!(lines[8].chars().nth(46), Some('╚'));
        assert!(lines[0].contains("┤ Modem ├"));

        // Statistics spans cols 46-79, rows 9-22.
        assert_eq!(lines[9].chars().nth(46), Some('╔'));
        assert_eq!(lines[22].chars().nth(79), Some('╝'));
        assert!(lines[9].contains("┤ Statistics ├"));
    }

    #[test]
    fn column_45_is_the_gap_between_the_panes() {
        // Only while the panes are on screen. Row 23 is the credit line,
        // centred on the whole 80 columns, and crosses the gap by design.
        for (i, line) in plain(&demo()).iter().enumerate().take(23) {
            assert_eq!(line.chars().nth(45), Some(' '), "row {i} closed the gap");
        }
    }

    #[test]
    fn the_stats_panel_shows_the_originals_labels() {
        let lines = plain(&demo());
        let stats = lines[9..=22].join("\n");
        for label in [
            "Started:",
            "Current:",
            "Max Dials:",
            "Dials/Hour:",
            "Ring:",
            "Secs:",
            "ETA:",
            "Voice",
            "Busy",
            "Rings",
            "Try #",
            "┤ Found ├",
        ] {
            assert!(stats.contains(label), "stats panel is missing {label:?}");
        }
    }

    #[test]
    fn the_found_rule_joins_its_dividers_and_the_heading_sits_right_of_it() {
        // TONELOC.C:1292-1295 draws wvline(4,15,7,...) and then
        // wprints(4,19,...,"┤ Found ├"): a rule seven rows tall, touching the
        // divider above and below, with the heading four columns to its right.
        let lines = plain(&demo());
        let at = |row: usize, col: usize| lines[row].chars().nth(col).unwrap();

        assert_eq!(at(14, 63), '╥', "rule should meet the divider above it");
        for r in 15..=19 {
            assert_eq!(at(r, 63), '║', "rule missing at row {r}");
        }
        assert_eq!(at(20, 63), '╨', "rule should meet the divider below it");

        let heading: String = lines[14].chars().skip(67).take(9).collect();
        assert_eq!(heading, "┤ Found ├", "heading is not four columns right");
    }

    #[test]
    fn a_carrier_scan_says_cds_and_a_tone_scan_says_tones() {
        // TONELOC.C:1312 switches this one label on the scan type.
        let carriers = plain(&demo()).join("\n");
        assert!(carriers.contains("CD's"));

        let tones = plain(&ScreenState {
            scan_type: ScanType::Tones,
            ..demo()
        })
        .join("\n");
        assert!(tones.contains("Tones"));
        assert!(!tones.contains("CD's"));
    }

    #[test]
    fn the_meter_is_thirty_cells_and_tracks_progress() {
        assert_eq!(meter_cells(0, 10_000), 0);
        assert_eq!(meter_cells(5_000, 10_000), 15);
        assert_eq!(meter_cells(10_000, 10_000), METER_LENGTH);
        // Never overruns, even on nonsense input (TONELOC.C:1528).
        assert_eq!(meter_cells(99_999, 10_000), METER_LENGTH);
        assert_eq!(meter_cells(5, 0), 0);

        let lines = plain(&demo());
        let meter: String = lines[21].chars().skip(48).take(METER_LENGTH).collect();
        assert_eq!(meter.chars().filter(|&c| c == '█').count(), 15);
        assert_eq!(meter.chars().filter(|&c| c == '▒').count(), 15);
    }

    #[test]
    fn the_credit_line_names_the_authors() {
        let lines = plain(&demo());
        assert!(lines[23].contains("ToneLoc 1.10 by Minor Threat & Mucho Maas"));
    }

    #[test]
    fn the_reconstruction_credit_sits_below_the_originals() {
        let lines = plain(&demo());
        assert!(lines[24].contains(RECONSTRUCTION_CREDIT));

        // Below, and centred on the same axis as the line it defers to.
        let centre = |s: &str| {
            let start = s.find('(').or_else(|| s.find('T')).unwrap_or(0);
            start + s.trim_end().len().saturating_sub(start) / 2
        };
        assert!(
            (centre(&lines[23]) as i32 - centre(&lines[24]) as i32).abs() <= 1,
            "the two credit lines are not aligned"
        );
    }

    #[test]
    fn long_lines_are_clipped_rather_than_breaking_the_layout() {
        let state = ScreenState {
            activity: vec!["x".repeat(300)],
            modem: vec!["y".repeat(300)],
            found: vec!["9".repeat(300)],
            ..demo()
        };
        let lines = plain(&state);
        for l in &lines {
            assert_eq!(l.chars().count(), COLS);
        }
        // The activity pane's right border survives.
        assert_eq!(lines[1].chars().nth(44), Some('║'));
        assert_eq!(lines[1].chars().nth(79), Some('║'));
    }

    #[test]
    fn panes_scroll_to_show_the_most_recent_lines() {
        let state = ScreenState {
            activity: (0..100).map(|i| format!("line {i}")).collect(),
            ..demo()
        };
        let lines = plain(&state);
        // 21 inner rows, so the last visible entry is line 99.
        assert!(lines[21].contains("line 99"), "{}", lines[21]);
        assert!(lines[1].contains("line 79"), "{}", lines[1]);
    }

    #[test]
    fn an_empty_state_still_renders_a_full_screen() {
        let lines = plain(&ScreenState::default());
        assert_eq!(lines.len(), ROWS);
        assert!(lines[0].contains("┤ Activity Log ├"));
    }
}
