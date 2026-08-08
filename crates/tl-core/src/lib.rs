//! `tl-core` — the pure, I/O-free heart of **toneloc-ish**.
//!
//! Everything here is plain data and plain functions: the `.DAT` scan-file
//! format, the per-number result codes, and the ToneMap legend. No files, no
//! sockets, no modem, no terminal. That is what lets the engine be built and
//! tested in full before any transport exists, and it is why the test suite
//! runs instantly.
//!
//! Fidelity is the point. Byte layouts, result codes and normalization rules
//! are ported from the original 1994 C source rather than inferred, and every
//! non-obvious decision cites the file and line it came from. See
//! `docs/dat-format.md` for the derivation.
//!
//! ```
//! use tl_core::{DatFile, CellClass};
//!
//! let mut dat = DatFile::new();
//! dat.set(9999, CellClass::Tone.with_rings(0));
//!
//! // The grid is column-major: 9999 is the bottom-right corner.
//! assert_eq!(dat.at(99, 99).class(), CellClass::Tone);
//! assert_eq!(dat.stats().tones, 1);
//!
//! // And it survives a trip through the on-disk format untouched.
//! assert_eq!(DatFile::parse(&dat.to_bytes()).unwrap(), dat);
//! ```

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

pub mod cell;
pub mod dat;
pub mod mask;
pub mod palette;
pub mod provenance;
pub mod sequence;
pub mod structure;

pub use cell::{Cell, CellClass, NoteKind};
pub use dat::{
    CELL_COUNT, DAT_LEN, DatError, DatFile, DatHeader, GRID_SIDE, HEADER_LEN, ScanStats,
};
pub use mask::{Mask, MaskError};
pub use palette::{DosColor, LegendEntry, cell_color, legend};
pub use provenance::Provenance;
pub use sequence::{DialOrder, ScanSequence};
pub use structure::{Character, Columns, Profile};
