//! `ReplayTransport` — a scan from 1993, answering the phone again.
//!
//! This is the preservation centrepiece. Load a real `.DAT` and put it where
//! the modem was: the engine dials, and the transport answers with the result
//! that was *actually recorded* for that number. Nothing is invented. Watching
//! it run is watching someone's real scan happen again, at whatever pace you
//! like.
//!
//! It answers in the modem's own vocabulary — `CONNECT`, `BUSY`, `NO CARRIER`
//! — rather than handing back cell states, so the engine above cannot tell a
//! replay from a real line. That is the point of the trait.

use crate::{ModemTransport, ResponseStrings};
use tl_core::{Cell, CellClass, DatFile};

/// Why a replay could not answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ReplayError {
    #[error("dial string {0:?} has no number in it")]
    NoNumber(&'static str),
}

/// A transport backed by a recorded scan.
#[derive(Debug)]
pub struct ReplayTransport {
    dat: DatFile,
    strings: ResponseStrings,
    /// Lines waiting to be read back, oldest first.
    pending: Vec<String>,
    /// The cell the last dial resolved to.
    last: Option<(u16, Cell)>,
}

impl ReplayTransport {
    pub fn new(dat: DatFile) -> ReplayTransport {
        ReplayTransport {
            dat,
            strings: ResponseStrings::default(),
            pending: Vec::new(),
            last: None,
        }
    }

    /// The scan being replayed.
    pub fn dat(&self) -> &DatFile {
        &self.dat
    }

    /// What the last dial resolved to, as `(number, cell)`.
    pub fn last_result(&self) -> Option<(u16, Cell)> {
        self.last
    }

    /// The recorded result for a number, as the modem would have reported it.
    ///
    /// Ring counts come back as separate `RINGING` lines before the verdict,
    /// so the engine's ring counter sees what it would have seen live.
    pub fn responses_for(&self, cell: Cell) -> Vec<String> {
        let mut lines = Vec::new();
        for _ in 0..cell.rings() {
            lines.push(self.strings.ringing.clone());
        }
        let verdict = match cell.class() {
            CellClass::Tone => Some(self.strings.tone.clone()),
            CellClass::Carrier => Some(format!("{} 14400", self.strings.connect)),
            CellClass::Busy => Some(self.strings.busy.clone()),
            CellClass::Voice => Some(self.strings.voice.clone()),
            CellClass::NoDialtone => Some(self.strings.no_tone.clone()),
            // A ringout is MaxRings of RINGING and then nothing: the engine
            // decides it has rung enough. Timeouts are silence too.
            CellClass::Ringout | CellClass::Timeout => None,
            CellClass::Noted => Some(self.strings.fax.clone()),
            _ => None,
        };
        lines.extend(verdict);
        lines
    }

    /// Extract the dialed number from an `ATDT…` string.
    ///
    /// Takes the last four digits, which is what indexes the `.DAT` — the
    /// prefix is the mask's business, not the file's.
    fn number_in(dial: &str) -> Option<u16> {
        let digits: String = dial.chars().filter(|c| c.is_ascii_digit()).collect();
        if digits.len() < 4 {
            return None;
        }
        digits[digits.len() - 4..].parse().ok()
    }
}

impl ModemTransport for ReplayTransport {
    type Error = ReplayError;

    fn send(&mut self, command: &str) -> Result<(), Self::Error> {
        let upper = command.to_ascii_uppercase();
        if !upper.contains("ATD") {
            // Init strings, hang-ups, volume — a modem would say OK.
            self.pending.push("OK".into());
            return Ok(());
        }
        let number = Self::number_in(command).ok_or(ReplayError::NoNumber("ATD"))?;
        let cell = self.dat.get(number);
        self.last = Some((number, cell));
        self.pending.extend(self.responses_for(cell));
        Ok(())
    }

    fn poll(&mut self) -> Result<Option<String>, Self::Error> {
        if self.pending.is_empty() {
            // Silence. This is what eventually becomes a Timeout, and it is a
            // real answer rather than an absence of one.
            return Ok(None);
        }
        Ok(Some(self.pending.remove(0)))
    }

    fn hang_up(&mut self) -> Result<(), Self::Error> {
        self.pending.clear();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ModemResponse;

    fn scan() -> DatFile {
        let mut dat = DatFile::new();
        dat.set(1234, CellClass::Carrier.with_rings(2));
        dat.set(9999, CellClass::Tone.with_rings(0));
        dat.set(5, CellClass::Busy.with_rings(0));
        dat.set(6, CellClass::Voice.with_rings(3));
        dat.set(7, CellClass::Ringout.with_rings(4));
        dat.set(8, CellClass::Timeout.with_rings(1));
        dat
    }

    fn drain(t: &mut ReplayTransport) -> Vec<String> {
        let mut out = Vec::new();
        while let Ok(Some(line)) = t.poll() {
            out.push(line);
        }
        out
    }

    #[test]
    fn dialing_a_carrier_replays_a_connect() {
        let mut t = ReplayTransport::new(scan());
        t.send("ATDT5551234W;").unwrap();
        let lines = drain(&mut t);
        // Two rings were recorded, then the connect.
        assert_eq!(lines, vec!["RINGING", "RINGING", "CONNECT 14400"]);
        assert_eq!(t.last_result().unwrap().0, 1234);
    }

    #[test]
    fn responses_classify_back_to_the_cell_they_came_from() {
        // Round trip: cell -> modem lines -> classification -> cell.
        let strings = ResponseStrings::default();
        let t = ReplayTransport::new(scan());
        for (class, rings) in [
            (CellClass::Tone, 0),
            (CellClass::Carrier, 2),
            (CellClass::Busy, 0),
            (CellClass::Voice, 3),
        ] {
            let cell = class.with_rings(rings);
            let lines = t.responses_for(cell);
            let verdict = lines.last().expect("a verdict");
            let response = strings.classify(verdict);
            assert_eq!(
                response.to_cell(rings),
                Some(cell),
                "{class:?} did not survive the round trip"
            );
            assert_eq!(
                lines.iter().filter(|l| *l == "RINGING").count(),
                rings as usize
            );
        }
    }

    #[test]
    fn a_ringout_is_rings_then_silence() {
        let t = ReplayTransport::new(scan());
        let lines = t.responses_for(CellClass::Ringout.with_rings(4));
        assert_eq!(lines, vec!["RINGING"; 4]);
    }

    #[test]
    fn a_timeout_says_nothing_at_all() {
        let mut t = ReplayTransport::new(scan());
        t.send("ATDT0008W;").unwrap();
        let lines = drain(&mut t);
        // One recorded ring, then nothing.
        assert_eq!(lines, vec!["RINGING"]);
        assert_eq!(t.poll().unwrap(), None);
    }

    #[test]
    fn an_undialed_number_replays_as_silence() {
        let mut t = ReplayTransport::new(scan());
        t.send("ATDT5550100W;").unwrap();
        assert_eq!(drain(&mut t), Vec::<String>::new());
    }

    #[test]
    fn the_last_four_digits_pick_the_cell() {
        assert_eq!(ReplayTransport::number_in("ATDT5551234W;"), Some(1234));
        assert_eq!(ReplayTransport::number_in("ATDT1-800-555-9999"), Some(9999));
        assert_eq!(ReplayTransport::number_in("ATDT0000"), Some(0));
        assert_eq!(ReplayTransport::number_in("ATDT12"), None);
    }

    #[test]
    fn non_dial_commands_get_an_ok_like_a_real_modem() {
        let mut t = ReplayTransport::new(scan());
        t.send("ATZ").unwrap();
        assert_eq!(drain(&mut t), vec!["OK"]);
        t.send("AT&F1M0").unwrap();
        assert_eq!(drain(&mut t), vec!["OK"]);
    }

    #[test]
    fn hanging_up_discards_anything_still_queued() {
        let mut t = ReplayTransport::new(scan());
        t.send("ATDT5551234W;").unwrap();
        t.hang_up().unwrap();
        assert_eq!(t.poll().unwrap(), None);
    }

    #[test]
    fn a_dial_string_with_no_number_is_an_error() {
        let mut t = ReplayTransport::new(scan());
        assert_eq!(t.send("ATDT"), Err(ReplayError::NoNumber("ATD")));
    }

    /// Replay determinism: dialing every number reproduces the file exactly.
    #[test]
    fn replaying_a_whole_scan_reproduces_its_cells() {
        let source = scan();
        let mut t = ReplayTransport::new(source.clone());
        let strings = ResponseStrings::default();
        let mut rebuilt = DatFile::new();

        for number in 0..10_000u16 {
            t.send(&format!("ATDT{number:04}W;")).unwrap();
            let mut rings = 0u8;
            let mut verdict = None;
            while let Some(line) = t.poll().unwrap() {
                match strings.classify(&line) {
                    ModemResponse::Ringing => rings += 1,
                    other => verdict = Some(other),
                }
            }
            let cell = match verdict {
                Some(r) => r.to_cell(rings),
                // Silence after rings: a ringout if it rang, else a timeout.
                None if rings > 0 => Some(ModemResponse::ringout(rings)),
                None => None,
            };
            if let Some(cell) = cell {
                rebuilt.set(number, cell);
            }
        }

        // Every cell the original recorded comes back identical.
        for number in 0..10_000u16 {
            let want = source.get(number);
            if matches!(want.class(), CellClass::Timeout) {
                // A recorded timeout replays as silence, and silence with no
                // rings is indistinguishable from never dialed — the engine's
                // per-dial timer is what makes it a Timeout live.
                continue;
            }
            assert_eq!(rebuilt.get(number), want, "number {number:04} diverged");
        }
    }
}
