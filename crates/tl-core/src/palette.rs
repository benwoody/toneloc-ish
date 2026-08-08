//! The ToneMap legend: which colour a result got on screen.
//!
//! The original VGA ToneMap (`TONEMAP.C:617-651`, `whatcolor()`) returned
//! indices into a custom 256-colour DAC palette that shipped inside the
//! executable — values like 43, 76 and 111 that mean nothing without it.
//! What survives and is documented is the *legend*: the manual describes each
//! result's colour in plain words, and `TEXTMAP.C` gives the letter key.
//!
//! We render to the 16-colour DOS/CGA palette those descriptions name. That is
//! a deliberate choice: it is the palette the era actually looked like, it maps
//! cleanly onto every terminal, and it keeps the legend legible rather than
//! guessing at DAC entries we cannot recover.
//!
//! One nuance is preserved exactly. Timeouts shaded lighter with more rings —
//! `pixcolor = oldval - 48` walked bytes 70-79 up a grey ramp. With sixteen
//! colours there are two greys, so the ramp splits at five rings.

use crate::cell::{Cell, CellClass};

/// The sixteen colours of the IBM CGA/EGA text palette, in their canonical
/// attribute order (0-7 dark, 8-15 bright).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum DosColor {
    Black = 0,
    Blue = 1,
    Green = 2,
    Cyan = 3,
    Red = 4,
    Magenta = 5,
    Brown = 6,
    LightGray = 7,
    DarkGray = 8,
    LightBlue = 9,
    LightGreen = 10,
    LightCyan = 11,
    LightRed = 12,
    LightMagenta = 13,
    Yellow = 14,
    White = 15,
}

impl DosColor {
    /// The palette index (0-15), as a DOS text attribute would carry it.
    #[inline]
    pub const fn index(self) -> u8 {
        self as u8
    }

    /// 24-bit RGB, using the standard IBM CGA values.
    pub const fn rgb(self) -> (u8, u8, u8) {
        match self {
            DosColor::Black => (0x00, 0x00, 0x00),
            DosColor::Blue => (0x00, 0x00, 0xaa),
            DosColor::Green => (0x00, 0xaa, 0x00),
            DosColor::Cyan => (0x00, 0xaa, 0xaa),
            DosColor::Red => (0xaa, 0x00, 0x00),
            DosColor::Magenta => (0xaa, 0x00, 0xaa),
            DosColor::Brown => (0xaa, 0x55, 0x00),
            DosColor::LightGray => (0xaa, 0xaa, 0xaa),
            DosColor::DarkGray => (0x55, 0x55, 0x55),
            DosColor::LightBlue => (0x55, 0x55, 0xff),
            DosColor::LightGreen => (0x55, 0xff, 0x55),
            DosColor::LightCyan => (0x55, 0xff, 0xff),
            DosColor::LightRed => (0xff, 0x55, 0x55),
            DosColor::LightMagenta => (0xff, 0x55, 0xff),
            DosColor::Yellow => (0xff, 0xff, 0x55),
            DosColor::White => (0xff, 0xff, 0xff),
        }
    }
}

/// The colour a cell gets on the ToneMap.
pub fn cell_color(cell: Cell) -> DosColor {
    match cell.class() {
        CellClass::Undialed => DosColor::Black,
        CellClass::Busy => DosColor::Red,
        CellClass::Voice => DosColor::Magenta,
        CellClass::NoDialtone => DosColor::Brown,
        CellClass::Noted => DosColor::LightCyan,
        CellClass::Aborted => DosColor::Blue,
        CellClass::Ringout => DosColor::Green,
        // "lighter grey = more rings" (TONEMAP.C:642).
        CellClass::Timeout => {
            if cell.rings() >= 5 {
                DosColor::LightGray
            } else {
                DosColor::DarkGray
            }
        }
        CellClass::Tone => DosColor::LightGreen,
        CellClass::Carrier => DosColor::Yellow,
        CellClass::Excluded => DosColor::DarkGray,
        CellClass::Omitted => DosColor::DarkGray,
        CellClass::Dialed => DosColor::LightBlue,
        CellClass::Blacklisted => DosColor::LightMagenta,
        CellClass::Unknown => DosColor::White,
    }
}

/// One row of the on-screen legend: colour, key letter, and label.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LegendEntry {
    pub class: CellClass,
    pub color: DosColor,
    pub key: char,
    pub label: &'static str,
}

/// The legend, in the order the ToneMap key lists it. Classes the original
/// never actually wrote to a file are left out.
pub fn legend() -> Vec<LegendEntry> {
    [
        CellClass::Carrier,
        CellClass::Tone,
        CellClass::Ringout,
        CellClass::Timeout,
        CellClass::Busy,
        CellClass::Voice,
        CellClass::Noted,
        CellClass::Aborted,
        CellClass::NoDialtone,
        CellClass::Blacklisted,
        CellClass::Undialed,
    ]
    .into_iter()
    .map(|class| {
        let probe = Cell(class.base_byte());
        LegendEntry {
            class,
            color: cell_color(probe),
            key: probe.textmap_char(),
            label: match class {
                CellClass::Carrier => "Carrier",
                CellClass::Tone => "Tone",
                CellClass::Ringout => "Ringout",
                CellClass::Timeout => "Timeout",
                CellClass::Busy => "Busy",
                CellClass::Voice => "Voice",
                CellClass::Noted => "Noted",
                CellClass::Aborted => "Aborted",
                CellClass::NoDialtone => "No Dialtone",
                CellClass::Blacklisted => "Blacklisted",
                CellClass::Undialed => "Undialed",
                _ => "",
            },
        }
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_two_finds_are_the_two_brightest_colors() {
        // Tones and carriers had to jump off a 100x100 grid at 3am.
        assert_eq!(cell_color(Cell(80)), DosColor::LightGreen);
        assert_eq!(cell_color(Cell(90)), DosColor::Yellow);
    }

    #[test]
    fn undialed_is_black_so_an_unfinished_scan_reads_as_empty() {
        assert_eq!(cell_color(Cell::UNDIALED), DosColor::Black);
    }

    #[test]
    fn timeout_shades_lighter_with_more_rings() {
        assert_eq!(cell_color(Cell(70)), DosColor::DarkGray);
        assert_eq!(cell_color(Cell(74)), DosColor::DarkGray);
        assert_eq!(cell_color(Cell(75)), DosColor::LightGray);
        assert_eq!(cell_color(Cell(79)), DosColor::LightGray);
    }

    #[test]
    fn legend_is_complete_and_labelled() {
        let l = legend();
        assert_eq!(l.len(), 11);
        assert!(l.iter().all(|e| !e.label.is_empty()));
        assert_eq!(l[0].class, CellClass::Carrier);
        assert_eq!(l[0].key, 'C');
    }

    #[test]
    fn dos_color_indices_are_the_cga_attribute_order() {
        assert_eq!(DosColor::Black.index(), 0);
        assert_eq!(DosColor::LightGray.index(), 7);
        assert_eq!(DosColor::DarkGray.index(), 8);
        assert_eq!(DosColor::White.index(), 15);
    }
}
