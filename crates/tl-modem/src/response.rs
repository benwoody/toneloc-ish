//! AT result codes: what the modem said back, and what it meant.
//!
//! ToneLoc did not parse the AT protocol so much as look for substrings. The
//! whole of `check_response()` (`TONELOC.C`) is eight `strstr` calls in a
//! fixed priority order against strings the user could edit in `TLCFG`.
//! We keep both properties — substring matching, and that exact order —
//! because a scan's results depend on them.

use tl_core::{Cell, CellClass};

/// A classified modem reply (`M_*` in `TONELOC.H:18-27`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ModemResponse {
    /// `OK` — the modem heard a dialtone. **This is a tone.**
    Ok,
    /// `CONNECT` — **a carrier**.
    Connect,
    /// `RINGING` — one more ring; counts toward MaxRings.
    Ringing,
    Busy,
    Voice,
    /// `NO DIAL[TONE]` — our own line has no dialtone; ToneLoc retries.
    NoTone,
    NoCarrier,
    Fax,
    /// A blank line. Modems emit these constantly; ignored.
    Nothing,
    /// Something we have no rule for.
    Unknown,
}

/// The response strings to match against, and the order to try them in.
///
/// Defaults are the ones `TLCFG` ships (`TLCFG.C:1291-1298`). They are
/// configurable because 1990s modems disagreed about their own vocabulary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResponseStrings {
    pub tone: String,
    pub connect: String,
    pub ringing: String,
    pub busy: String,
    pub voice: String,
    pub no_tone: String,
    pub no_carrier: String,
    pub fax: String,
}

impl Default for ResponseStrings {
    fn default() -> Self {
        ResponseStrings {
            tone: "OK".into(),
            connect: "CONNECT".into(),
            ringing: "RINGING".into(),
            busy: "BUSY".into(),
            voice: "VOICE".into(),
            no_tone: "NO DIAL".into(),
            no_carrier: "NO CARRIER".into(),
            fax: "FAX".into(),
        }
    }
}

impl ResponseStrings {
    /// Classify one line from the modem.
    ///
    /// Order is load-bearing and matches `check_response()` exactly: tone,
    /// connect, ringing, busy, voice, no-tone, no-carrier, fax. Matching is
    /// by substring, so `CONNECT 2400` classifies as [`ModemResponse::Connect`]
    /// without needing to know about speeds.
    pub fn classify(&self, line: &str) -> ModemResponse {
        let s = line.trim();
        if s.is_empty() {
            return ModemResponse::Nothing;
        }
        // The original compared against upper-case strings from a config
        // written by hand; real modems answer in upper case.
        let s = s.to_ascii_uppercase();
        let has = |needle: &str| !needle.is_empty() && s.contains(&needle.to_ascii_uppercase());

        if has(&self.tone) {
            ModemResponse::Ok
        } else if has(&self.connect) {
            ModemResponse::Connect
        } else if has(&self.ringing) {
            ModemResponse::Ringing
        } else if has(&self.busy) {
            ModemResponse::Busy
        } else if has(&self.voice) {
            ModemResponse::Voice
        } else if has(&self.no_tone) {
            ModemResponse::NoTone
        } else if has(&self.no_carrier) {
            ModemResponse::NoCarrier
        } else if has(&self.fax) {
            ModemResponse::Fax
        } else {
            ModemResponse::Unknown
        }
    }
}

impl ModemResponse {
    /// The cell this response gets recorded as, given the ring count so far.
    ///
    /// Ported from the dial loop (`TONELOC.C:470-540`). Two mappings are worth
    /// pointing at because they are not what you would guess:
    ///
    /// - `NO CARRIER` records as **Timeout** (`70+rings`), not as its own state.
    ///   Nothing answered, so nothing is what gets recorded.
    /// - `FAX` records as `41` — a *note*, with no ring count.
    ///
    /// `RINGING` returns `None`: it advances the ring counter without settling
    /// the call. Only when the count reaches MaxRings does it become a
    /// [`CellClass::Ringout`] — see [`ModemResponse::ringout`].
    pub fn to_cell(self, rings: u8) -> Option<Cell> {
        Some(match self {
            ModemResponse::Ok => CellClass::Tone.with_rings(rings),
            ModemResponse::Connect => CellClass::Carrier.with_rings(rings),
            ModemResponse::Busy => CellClass::Busy.with_rings(rings),
            ModemResponse::Voice => CellClass::Voice.with_rings(rings),
            ModemResponse::NoTone => CellClass::NoDialtone.with_rings(rings),
            // Not a typo: see the doc comment above.
            ModemResponse::NoCarrier => CellClass::Timeout.with_rings(rings),
            ModemResponse::Fax => Cell(41),
            ModemResponse::Ringing | ModemResponse::Nothing | ModemResponse::Unknown => {
                return None;
            }
        })
    }

    /// The cell recorded when ringing hits MaxRings (`TONELOC.C:505`).
    pub fn ringout(rings: u8) -> Cell {
        CellClass::Ringout.with_rings(rings)
    }

    /// Whether this response ends the call.
    pub fn is_terminal(self) -> bool {
        !matches!(
            self,
            ModemResponse::Ringing | ModemResponse::Nothing | ModemResponse::Unknown
        )
    }

    /// Whether ToneLoc would retry the number. Only a missing dialtone on
    /// *our* end warrants that (`TONELOC.C:526`, `again=1`).
    pub fn should_retry(self) -> bool {
        matches!(self, ModemResponse::NoTone)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(line: &str) -> ModemResponse {
        ResponseStrings::default().classify(line)
    }

    #[test]
    fn classifies_the_standard_result_codes() {
        assert_eq!(c("OK"), ModemResponse::Ok);
        assert_eq!(c("CONNECT"), ModemResponse::Connect);
        assert_eq!(c("RINGING"), ModemResponse::Ringing);
        assert_eq!(c("BUSY"), ModemResponse::Busy);
        assert_eq!(c("VOICE"), ModemResponse::Voice);
        assert_eq!(c("NO DIALTONE"), ModemResponse::NoTone);
        assert_eq!(c("NO CARRIER"), ModemResponse::NoCarrier);
        assert_eq!(c("FAX"), ModemResponse::Fax);
    }

    #[test]
    fn matches_by_substring_so_speeds_and_noise_still_classify() {
        assert_eq!(c("CONNECT 2400"), ModemResponse::Connect);
        assert_eq!(c("CONNECT 14400/ARQ/V32/LAPM"), ModemResponse::Connect);
        assert_eq!(c("  NO CARRIER  "), ModemResponse::NoCarrier);
    }

    #[test]
    fn blank_lines_are_ignored_not_unknown() {
        assert_eq!(c(""), ModemResponse::Nothing);
        assert_eq!(c("   \r\n"), ModemResponse::Nothing);
        assert!(!ModemResponse::Nothing.is_terminal());
    }

    #[test]
    fn garbage_is_unknown_and_does_not_settle_the_call() {
        assert_eq!(c("~~\u{1}garbage"), ModemResponse::Unknown);
        assert!(!ModemResponse::Unknown.is_terminal());
        assert_eq!(ModemResponse::Unknown.to_cell(0), None);
    }

    #[test]
    fn responses_map_to_the_cells_the_original_wrote() {
        assert_eq!(ModemResponse::Ok.to_cell(0), Some(Cell(80)));
        assert_eq!(ModemResponse::Connect.to_cell(1), Some(Cell(91)));
        assert_eq!(ModemResponse::Busy.to_cell(0), Some(Cell(10)));
        assert_eq!(ModemResponse::Voice.to_cell(3), Some(Cell(23)));
        assert_eq!(ModemResponse::NoTone.to_cell(0), Some(Cell(30)));
        assert_eq!(ModemResponse::ringout(4), Cell(64));
    }

    #[test]
    fn no_carrier_is_recorded_as_a_timeout() {
        // The surprising one. TONELOC.C:531 — nothing answered, so the cell
        // says "nothing happened", not "no carrier".
        let cell = ModemResponse::NoCarrier.to_cell(2).unwrap();
        assert_eq!(cell, Cell(72));
        assert_eq!(cell.class(), CellClass::Timeout);
    }

    #[test]
    fn fax_is_a_note_and_carries_no_ring_count() {
        assert_eq!(ModemResponse::Fax.to_cell(7), Some(Cell(41)));
        assert_eq!(Cell(41).class(), CellClass::Noted);
    }

    #[test]
    fn ring_counts_clamp_at_nine() {
        assert_eq!(ModemResponse::Ok.to_cell(200), Some(Cell(89)));
    }

    #[test]
    fn only_a_missing_dialtone_triggers_a_retry() {
        assert!(ModemResponse::NoTone.should_retry());
        for r in [
            ModemResponse::Ok,
            ModemResponse::Connect,
            ModemResponse::Busy,
            ModemResponse::Voice,
            ModemResponse::NoCarrier,
        ] {
            assert!(!r.should_retry(), "{r:?} should not retry");
        }
    }

    #[test]
    fn priority_order_matches_check_response() {
        // "OK" is tested before everything else, so a modem that emits
        // "OK CONNECT" is read as a tone. Faithful, if odd.
        assert_eq!(c("OK CONNECT"), ModemResponse::Ok);
        // And an empty configured string never matches anything.
        let strings = ResponseStrings {
            tone: String::new(),
            ..Default::default()
        };
        assert_eq!(strings.classify("CONNECT"), ModemResponse::Connect);
    }
}
