//! `tl-modem` — where the modem used to be.
//!
//! ToneLoc talked to a 16550 UART through interrupt handlers and, optionally,
//! a FOSSIL driver. Both are gone; a [`ModemTransport`] sits in their place.
//! The scan engine cannot tell what is underneath it, which is the whole
//! trick: a replayed 1993 scan and a synthetic exchange drive the identical
//! state machine.
//!
//! (A FOSSIL driver was DOS's "let a driver own the serial port" abstraction —
//! conceptually the same job this trait does now.)

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

pub mod replay;
pub mod response;

pub use replay::{ReplayError, ReplayTransport};
pub use response::{ModemResponse, ResponseStrings};

/// What a transport can be asked to do, and what it says back.
///
/// Kept deliberately small. Everything above it — ring counting, timeouts,
/// autosave, the ToneMap — is engine logic in `tl-core` and works the same
/// whichever transport is plugged in.
pub trait ModemTransport {
    type Error: std::error::Error;

    /// Send a command line to the modem (`ATDT5551234W;` and friends).
    fn send(&mut self, command: &str) -> Result<(), Self::Error>;

    /// Read the next line the modem produced, if one is ready.
    ///
    /// Returns `Ok(None)` when nothing has arrived yet — silence is a real
    /// answer here, and is what eventually becomes a Timeout.
    fn poll(&mut self) -> Result<Option<String>, Self::Error>;

    /// Drop the line (`ATH0`, or DTR).
    fn hang_up(&mut self) -> Result<(), Self::Error>;
}

/// Build the dial string ToneLoc would send for a number.
///
/// Tone scans dial `ATDT<number>W;` — `W` waits for a dialtone and `;` returns
/// the modem to command mode, so `OK` comes back exactly when a tone was
/// heard. Carrier scans omit both and wait for `CONNECT`.
///
/// The number is passed through verbatim, which is what makes PBX-hack masks
/// like `555-9999Wxxx` work: the embedded `W` reaches the modem untouched.
pub fn dial_string(prefix: &str, number: &str, suffix: &str, tone_scan: bool) -> String {
    let tail = if tone_scan { "W;" } else { "" };
    format!("{prefix}{number}{suffix}{tail}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tone_scans_wait_for_dialtone_and_return_to_command_mode() {
        assert_eq!(dial_string("ATDT", "5551234", "", true), "ATDT5551234W;");
    }

    #[test]
    fn carrier_scans_just_dial_and_listen() {
        assert_eq!(dial_string("ATDT", "5551234", "", false), "ATDT5551234");
    }

    #[test]
    fn pbx_hack_masks_pass_through_verbatim() {
        // The nested W is part of the number, not something we generate.
        assert_eq!(
            dial_string("ATDT", "5559999W123", "", true),
            "ATDT5559999W123W;"
        );
    }

    #[test]
    fn suffix_lands_before_the_wait_sequence() {
        assert_eq!(dial_string("ATDT", "1234", ",,", true), "ATDT1234,,W;");
    }
}
