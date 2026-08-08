//! `tl-tui` — ToneLoc's screen, rebuilt in a terminal.
//!
//! The original drove CXL text windows and a VGA mode-13h ToneMap. Both are
//! replaced here by direct ANSI rendering, but the layout they produced is
//! the thing being preserved: three windows (activity log, modem, stats with
//! its meter) and the ToneMap grid.
//!
//! Milestone 1 ships the map. See [`tonemap`].

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

pub mod screen;
pub mod textmap;
pub mod tonemap;

pub use screen::{ScanType, ScreenState};
pub use textmap::{KEY as TEXTMAP_KEY, TextMapOptions};
pub use tonemap::{MapStyle, render_ansi, render_key};
