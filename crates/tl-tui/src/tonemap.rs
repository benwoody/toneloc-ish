//! The ToneMap: ten thousand numbers on one screen.
//!
//! This is the part of ToneLoc its authors were proudest of, and the reason
//! the program is remembered. A scan produces a 100 × 100 grid — one cell per
//! number, column-major, `0000` top-left and `9999` bottom-right — and the
//! patterns that emerge are readable at a glance. Residential prefixes are
//! evenly speckled with no structure. Business exchanges show bands where a
//! PBX owns a contiguous DID range, and clusters of carriers where the modems
//! live. You can tell which you are looking at without reading a single number.
//!
//! The original drew it in VGA at two pixels per cell. We draw it with the
//! upper-half-block character `▀`: foreground paints the top row, background
//! the bottom, so one terminal cell carries two grid rows and the whole map
//! lands in 100 × 50 characters at roughly the right aspect ratio.

use std::fmt::Write as _;
use tl_core::{Cell, CellClass, DatFile, DosColor, GRID_SIDE, cell_color, legend};

/// The character that does the work: top half foreground, bottom half background.
const HALF: char = '▀';

/// How to draw the map.
#[derive(Clone, Copy, Debug)]
pub struct MapStyle {
    /// Draw the surrounding double-line box and headings.
    pub chrome: bool,
    /// Draw the colour key beneath the map.
    pub key: bool,
    /// Draw the `0`-`9` rulers along the top and left edges.
    pub rulers: bool,
    /// Halve the width by packing two grid columns into one character.
    /// Loses horizontal detail; fits an 80-column terminal.
    pub narrow: bool,
}

impl Default for MapStyle {
    fn default() -> Self {
        MapStyle {
            chrome: true,
            key: true,
            rulers: true,
            narrow: false,
        }
    }
}

impl MapStyle {
    /// Total columns the rendered map needs, including chrome.
    pub fn width(&self) -> usize {
        let grid = if self.narrow {
            GRID_SIDE / 2
        } else {
            GRID_SIDE
        };
        let gutter = if self.rulers { 5 } else { 0 };
        let border = if self.chrome { 2 } else { 0 };
        grid + gutter + border
    }

    /// Shrink until this fits `available` columns.
    ///
    /// A ToneMap that wraps is not a degraded ToneMap, it is a destroyed one:
    /// every row spills onto the next line and the grid stops being a grid. So
    /// fitting comes before anything else.
    ///
    /// Candidates are tried in preference order rather than degraded one
    /// property at a time, because the two axes are not independent. Full
    /// resolution needs 100 columns and there is no size between that and 50,
    /// so an 80-column terminal must halve the grid — and once it has, there
    /// is room for the box and rulers again. Stepping down blindly would drop
    /// them and leave thirty columns empty.
    pub fn fit(self, available: usize) -> MapStyle {
        let candidates = [
            // Full resolution first: detail matters more than decoration.
            (false, true, true),
            (false, true, false),
            (false, false, false),
            // Then half width, buying the chrome back.
            (true, true, true),
            (true, true, false),
            (true, false, false),
        ];

        for (narrow, chrome, rulers) in candidates {
            let candidate = MapStyle {
                narrow,
                // Never turn things on that were not asked for.
                chrome: chrome && self.chrome,
                rulers: rulers && self.rulers,
                key: self.key,
            };
            if candidate.width() <= available {
                return candidate;
            }
        }

        // Narrower than 50 columns. Nothing fits; give the smallest we have.
        MapStyle {
            narrow: true,
            chrome: false,
            rulers: false,
            key: self.key,
        }
    }

    /// Fit to the current terminal.
    ///
    /// Left alone when stdout is not a terminal — piping to a file or a pager
    /// should give the full-resolution map, not one cropped to whatever
    /// happened to be attached.
    pub fn fit_to_terminal(self) -> MapStyle {
        match terminal_width() {
            Some(w) => self.fit(w),
            None => self,
        }
    }
}

/// The terminal's width, or `None` when there isn't one to speak of.
///
/// `COLUMNS` wins when set, which is both the usual convention and what makes
/// this testable. Otherwise the terminal is asked directly, and only if stdout
/// is actually a terminal — piping to a file has no width to respect.
///
/// A reported width of zero means "don't know", not "no space". Some pseudo
/// terminals (`script`, some CI runners) report exactly that, and treating it
/// literally would silently strip the map down to its smallest form.
pub fn terminal_width() -> Option<usize> {
    if let Some(w) = std::env::var("COLUMNS")
        .ok()
        .and_then(|v| v.trim().parse::<usize>().ok())
        .filter(|&w| w > 0)
    {
        return Some(w);
    }

    use crossterm::tty::IsTty;
    if !std::io::stdout().is_tty() {
        return None;
    }
    crossterm::terminal::size()
        .ok()
        .map(|(w, _)| w as usize)
        .filter(|&w| w > 0)
}

// --- ANSI plumbing ---------------------------------------------------------

fn fg(c: DosColor) -> String {
    let (r, g, b) = c.rgb();
    format!("\x1b[38;2;{r};{g};{b}m")
}

fn bg(c: DosColor) -> String {
    let (r, g, b) = c.rgb();
    format!("\x1b[48;2;{r};{g};{b}m")
}

const RESET: &str = "\x1b[0m";

/// Pick the colour for a half-block cell, blending when two grid columns share
/// one character: a find always wins, because missing a carrier in the grid
/// defeats the point of drawing one.
fn merge(a: Cell, b: Cell) -> Cell {
    fn rank(c: Cell) -> u8 {
        match c.class() {
            CellClass::Carrier => 5,
            CellClass::Tone => 4,
            CellClass::Noted => 3,
            CellClass::Ringout | CellClass::Busy | CellClass::Voice => 2,
            CellClass::Timeout => 1,
            _ => 0,
        }
    }
    if rank(b) > rank(a) { b } else { a }
}

/// Render the grid to a string of ANSI escapes, ready to print.
pub fn render_ansi(dat: &DatFile, title: &str, style: MapStyle) -> String {
    let mut out = String::with_capacity(64 * 1024);
    let cols = if style.narrow {
        GRID_SIDE / 2
    } else {
        GRID_SIDE
    };
    let gutter = if style.rulers { "     " } else { "" };
    let inner = gutter.len() + cols;

    // Box drawing, in the CXL double-line style ToneLoc's own windows used.
    let (edge, edge_end) = if style.chrome {
        (
            format!("{}║{}", fg(DosColor::LightCyan), RESET),
            format!("{}║{}", fg(DosColor::LightCyan), RESET),
        )
    } else {
        (String::new(), String::new())
    };

    if style.chrome {
        let stats = dat.stats();
        let (h, m) = dat.header.time_spent();
        writeln!(
            out,
            "{}{}",
            fg(DosColor::LightCyan),
            heading(title, inner + 2)
        )
        .ok();

        // Two phrasings, so the narrow map does not truncate away the
        // carrier count — which is the number anyone came here for.
        let long = format!(
            " ToneLoc {} data file    {} dialed    {} tones    {} carriers    {}:{:02} scanning",
            dat.header.version_string(),
            stats.tried,
            stats.tones,
            stats.carriers,
            h,
            m,
        );
        let summary = if long.chars().count() <= inner {
            long
        } else {
            format!(
                " {}  {} dialed  {}T  {}C  {}:{:02}",
                dat.header.version_string(),
                stats.tried,
                stats.tones,
                stats.carriers,
                h,
                m,
            )
        };
        writeln!(
            out,
            "{edge}{}{:<inner$}{RESET}{edge_end}",
            fg(DosColor::LightGray),
            truncate(&summary, inner),
        )
        .ok();
        writeln!(
            out,
            "{}╟{}╢{RESET}",
            fg(DosColor::LightCyan),
            "─".repeat(inner)
        )
        .ok();
    }

    // Column ruler: the hundreds digit of each column, so you can find 0100.
    if style.rulers {
        out.push_str(&edge);
        out.push_str(&fg(DosColor::DarkGray));
        out.push_str(gutter);
        for c in 0..cols {
            let real_col = if style.narrow { c * 2 } else { c };
            out.push(if real_col % 10 == 0 {
                char::from_digit((real_col / 10) as u32 % 10, 10).unwrap_or('.')
            } else {
                '·'
            });
        }
        out.push_str(RESET);
        out.push_str(&edge_end);
        out.push('\n');
    }

    // Two grid rows per terminal row.
    for pair in 0..GRID_SIDE / 2 {
        let top_row = pair * 2;
        let bottom_row = top_row + 1;

        out.push_str(&edge);

        if style.rulers {
            // Label every tenth line with the row's offset inside the column.
            let label = if top_row % 10 == 0 {
                format!("{top_row:>3}  ")
            } else {
                "     ".to_string()
            };
            out.push_str(&fg(DosColor::DarkGray));
            out.push_str(&label);
            out.push_str(RESET);
        }

        let mut last: Option<(DosColor, DosColor)> = None;
        for c in 0..cols {
            let (top, bottom) = if style.narrow {
                (
                    merge(dat.at(c * 2, top_row), dat.at(c * 2 + 1, top_row)),
                    merge(dat.at(c * 2, bottom_row), dat.at(c * 2 + 1, bottom_row)),
                )
            } else {
                (dat.at(c, top_row), dat.at(c, bottom_row))
            };
            let (tc, bc) = (cell_color(top), cell_color(bottom));
            // Only emit escapes when the colour actually changes; a naive
            // renderer emits 40 bytes per cell and 400 KB per frame.
            if last != Some((tc, bc)) {
                out.push_str(&fg(tc));
                out.push_str(&bg(bc));
                last = Some((tc, bc));
            }
            out.push(HALF);
        }
        out.push_str(RESET);
        out.push_str(&edge_end);
        out.push('\n');
    }

    if style.chrome {
        writeln!(
            out,
            "{}╚{}╝{RESET}",
            fg(DosColor::LightCyan),
            "═".repeat(inner)
        )
        .ok();
    }

    if style.key {
        out.push('\n');
        out.push_str(&render_key(inner));
    }

    out
}

/// Cut a string to a character budget (not a byte budget — the chrome is
/// box-drawing characters).
fn truncate(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

/// `╔═╡ TITLE ╞════...════╗`, spanning exactly `width` characters.
fn heading(title: &str, width: usize) -> String {
    let inner = width.saturating_sub(2); // the two corner characters
    let label = format!("╡ {title} ╞");
    let label_len = title.chars().count() + 4;

    if label_len + 2 > inner {
        return format!("╔{}╗", "═".repeat(inner));
    }
    let rest = inner - label_len;
    let left = 1.max(rest / 2);
    let right = rest - left;
    format!("╔{}{}{}╗", "═".repeat(left), label, "═".repeat(right))
}

/// The colour key, in the manual's own terms.
pub fn render_key(width: usize) -> String {
    let mut out = String::new();
    let entries = legend();
    let per_line = (width / 18).max(1);
    for (i, e) in entries.iter().enumerate() {
        if i % per_line == 0 && i > 0 {
            out.push('\n');
        }
        // A block in the colour, then the TextMap letter, then the label.
        let _ = write!(
            out,
            "{}██{} {}{} {:<13}{}",
            fg(e.color),
            RESET,
            fg(DosColor::LightGray),
            e.key,
            e.label,
            RESET
        );
    }
    out.push('\n');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use tl_core::CellClass;

    fn strip_ansi(s: &str) -> String {
        let mut out = String::new();
        let mut chars = s.chars();
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
    }

    #[test]
    fn renders_fifty_grid_lines_for_a_hundred_rows() {
        let dat = DatFile::new();
        let style = MapStyle {
            chrome: false,
            key: false,
            rulers: false,
            narrow: false,
        };
        let text = strip_ansi(&render_ansi(&dat, "TEST", style));
        let lines: Vec<_> = text.lines().filter(|l| !l.is_empty()).collect();
        assert_eq!(lines.len(), GRID_SIDE / 2);
        assert!(lines.iter().all(|l| l.chars().count() == GRID_SIDE));
    }

    #[test]
    fn a_carrier_shows_up_in_the_right_place() {
        let mut dat = DatFile::new();
        dat.set(0, CellClass::Carrier.with_rings(0)); // top-left

        let style = MapStyle {
            chrome: false,
            key: false,
            rulers: false,
            narrow: false,
        };
        let text = render_ansi(&dat, "T", style);
        let first_line = text.lines().next().unwrap();
        // Number 0000 is the top half of the first character of the first line,
        // so the first foreground colour emitted must be the carrier yellow.
        assert!(
            first_line.starts_with(&fg(DosColor::Yellow)),
            "expected carrier yellow at 0000"
        );
    }

    #[test]
    fn narrow_mode_halves_the_width_without_losing_finds() {
        let mut dat = DatFile::new();
        // A lone carrier in an odd column would vanish under naive downsampling.
        // 0100 sits in column 1, row 0 — an odd column, so a renderer that
        // simply skipped every second column would lose it.
        dat.set(100, CellClass::Carrier.with_rings(0));

        let style = MapStyle {
            chrome: false,
            key: false,
            rulers: false,
            narrow: true,
        };
        let text = render_ansi(&dat, "T", style);
        let plain = strip_ansi(&text);
        assert!(
            plain
                .lines()
                .filter(|l| !l.is_empty())
                .all(|l| l.chars().count() == GRID_SIDE / 2)
        );
        assert!(
            text.contains(&fg(DosColor::Yellow)),
            "a carrier in an odd column must survive narrow rendering"
        );
    }

    #[test]
    fn merge_prefers_the_find_over_the_background() {
        let carrier = CellClass::Carrier.with_rings(0);
        let timeout = CellClass::Timeout.with_rings(2);
        assert_eq!(merge(timeout, carrier), carrier);
        assert_eq!(merge(carrier, timeout), carrier);
        // Tones beat everything except carriers.
        let tone = CellClass::Tone.with_rings(0);
        assert_eq!(merge(tone, timeout), tone);
        assert_eq!(merge(tone, carrier), carrier);
    }

    #[test]
    fn escape_codes_are_emitted_only_on_change() {
        // An all-undialed map is one solid colour: it should set the colour
        // once per line, not ten thousand times.
        let dat = DatFile::new();
        let style = MapStyle {
            chrome: false,
            key: false,
            rulers: false,
            narrow: false,
        };
        let text = render_ansi(&dat, "T", style);
        let escapes = text.matches("\x1b[38").count();
        assert!(escapes <= GRID_SIDE / 2, "emitted {escapes} colour changes");
    }

    #[test]
    fn the_box_closes_and_every_line_is_the_same_width() {
        let dat = DatFile::new();
        let style = MapStyle {
            key: false,
            ..MapStyle::default()
        };
        let text = strip_ansi(&render_ansi(&dat, "SAMPLE5.DAT", style));
        let lines: Vec<&str> = text.lines().collect();

        let widths: std::collections::BTreeSet<usize> =
            lines.iter().map(|l| l.chars().count()).collect();
        assert_eq!(widths.len(), 1, "ragged box: line widths were {widths:?}");
        assert_eq!(*widths.iter().next().unwrap(), style.width());

        assert!(lines[0].starts_with('╔') && lines[0].ends_with('╗'));
        assert!(lines[0].contains("SAMPLE5.DAT"));
        let last = lines.last().unwrap();
        assert!(last.starts_with('╚') && last.ends_with('╝'));
        // Grid rows sit inside the vertical rules.
        assert!(lines[4].starts_with('║') && lines[4].ends_with('║'));
    }

    #[test]
    fn an_over_long_title_does_not_break_the_box() {
        let dat = DatFile::new();
        let style = MapStyle {
            key: false,
            narrow: true,
            ..MapStyle::default()
        };
        let long = "A".repeat(200);
        let text = strip_ansi(&render_ansi(&dat, &long, style));
        let widths: std::collections::BTreeSet<usize> =
            text.lines().map(|l| l.chars().count()).collect();
        assert_eq!(widths.len(), 1, "line widths were {widths:?}");
    }

    #[test]
    fn a_standard_80_column_terminal_keeps_its_chrome() {
        // 80 cannot hold a 100-wide grid, so the grid halves — and once it
        // has, the box and rulers fit again inside 80. Degrading one property
        // at a time would have thrown them away and left 30 columns unused.
        let fitted = MapStyle::default().fit(80);
        assert!(fitted.narrow);
        assert!(
            fitted.chrome && fitted.rulers,
            "room for chrome went unused"
        );
        assert_eq!(fitted.width(), 57);
    }

    #[test]
    fn a_100_column_terminal_keeps_full_resolution_over_decoration() {
        // Exactly enough for the grid itself, and detail beats decoration.
        let fitted = MapStyle::default().fit(100);
        assert!(!fitted.narrow, "100 columns should not cost detail");
        assert!(!fitted.chrome && !fitted.rulers);
        assert_eq!(fitted.width(), 100);
    }

    #[test]
    fn fitting_never_switches_on_what_was_not_asked_for() {
        let bare = MapStyle {
            chrome: false,
            key: false,
            rulers: false,
            narrow: false,
        };
        let fitted = bare.fit(80);
        assert!(!fitted.chrome && !fitted.rulers && !fitted.key);
    }

    #[test]
    fn the_narrow_header_keeps_the_carrier_count() {
        let mut dat = DatFile::new();
        for n in 0..214u16 {
            dat.set(n, CellClass::Carrier.with_rings(0));
        }
        let style = MapStyle {
            key: false,
            ..MapStyle::default()
        }
        .fit(80);

        let text = strip_ansi(&render_ansi(&dat, "SAMPLE11.DAT", style));
        let header = text.lines().nth(1).expect("a summary line");
        assert!(
            header.contains("214"),
            "the carrier count was truncated away: {header:?}"
        );
        assert!(header.chars().count() <= style.width());
    }

    #[test]
    fn an_absurdly_narrow_terminal_still_produces_something() {
        let fitted = MapStyle::default().fit(10);
        assert!(fitted.narrow && !fitted.chrome && !fitted.rulers);
        assert_eq!(fitted.width(), 50);
    }

    #[test]
    fn a_wide_terminal_keeps_everything() {
        let full = MapStyle::default().fit(120);
        assert_eq!(full.width(), MapStyle::default().width());
        assert!(full.rulers && full.chrome && !full.narrow);
    }

    #[test]
    fn fitted_output_never_exceeds_the_terminal() {
        let dat = DatFile::new();
        for available in [50usize, 60, 80, 100, 107, 200] {
            let style = MapStyle {
                key: false,
                ..MapStyle::default()
            }
            .fit(available);
            let text = strip_ansi(&render_ansi(&dat, "T", style));
            let widest = text.lines().map(|l| l.chars().count()).max().unwrap_or(0);
            assert!(
                widest <= available.max(style.width()),
                "at {available} columns the map rendered {widest} wide"
            );
            if available >= 50 {
                assert!(widest <= available, "wrapped at {available} columns");
            }
        }
    }

    #[test]
    fn style_width_accounts_for_rulers_and_chrome() {
        assert_eq!(MapStyle::default().width(), 100 + 5 + 2);
        let bare = MapStyle {
            chrome: false,
            key: false,
            rulers: false,
            narrow: true,
        };
        assert_eq!(bare.width(), 50);
    }

    #[test]
    fn the_key_lists_every_legend_entry() {
        let key = strip_ansi(&render_key(120));
        for entry in legend() {
            assert!(key.contains(entry.label), "key is missing {}", entry.label);
        }
    }
}
