//! What we know about the archival scans, from the people who made them.
//!
//! The fourteen `.DAT` files that shipped with ToneLoc are real recorded
//! scans, and the distribution included `SAMPLES.DOC` — the authors walking
//! through each one explaining what its patterns mean. That commentary is part
//! of the artifact. A ToneMap without it is a picture; with it, it is a
//! reading.
//!
//! The `.DAT` format has nowhere to put any of this. Its ten reserved header
//! bytes were earmarked for a start date, a last date and the mask
//! (`TONELOC.H:75-77`) but no released version ever wrote them, so every
//! archival file's header is zeros past `Minutes`. Provenance therefore lives
//! outside the file — in `SAMPLES.DOC`, and in `TONE.LOG` for the one scan
//! whose log survived.
//!
//! The annotations below are the authors' words, quoted rather than
//! paraphrased. Where they asked a question they never answered, the question
//! is kept.

/// What is known about one archival scan.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Provenance {
    /// Canonical file name, upper case, as distributed.
    pub file: &'static str,
    /// The authors' commentary from `SAMPLES.DOC`, verbatim.
    pub note: &'static str,
    /// Anything recovered from `TONE.LOG`, where a log survived.
    pub log: Option<&'static str>,
}

/// The authors' commentary on each shipped sample, from `SAMPLES.DOC`.
pub const ARCHIVE: &[Provenance] = &[
    Provenance {
        file: "SAMPLE1.DAT",
        note: "A generic residential exchange. Few carriers. Note the faint pattern from \
               6900-7400.",
        log: None,
    },
    Provenance {
        file: "SAMPLE2.DAT",
        note: "A split exchange. 0-4000 is residential, business from 4000-9999.",
        log: None,
    },
    Provenance {
        file: "SAMPLE3.DAT",
        note: "A double exposure, this is a merge of a tone and carrier scan. A dozen odd \
               PBX's are sandwiched between residential or mixed use. Note the tones and \
               carriers at the bottom of the PBX DID ranges.",
        log: None,
    },
    Provenance {
        file: "SAMPLE4.DAT",
        note: "Another mixed exchange, this time with wider bands. The voice ranges are \
               unworking numbers. The busy bands are unused numbers in commercial DID groups.",
        log: None,
    },
    Provenance {
        file: "SAMPLE5.DAT",
        note: "This is much like sample 4, but with a more typical blurring of boundaries \
               between bands.",
        log: None,
    },
    Provenance {
        file: "SAMPLE6.DAT",
        note: "More mixed exchanges, with even less distinction between bands. Here hunt and \
               DID groups do not fill even bands of 100. This one comes from a large city \
               where phone numbers are at a premium.",
        log: None,
    },
    Provenance {
        file: "SAMPLE7.DAT",
        note: "More mixed exchanges, with even less distinction between bands. Here hunt and \
               DID groups do not fill even bands of 100. This one comes from a large city \
               where phone numbers are at a premium.",
        log: None,
    },
    Provenance {
        file: "SAMPLE8A.DAT",
        note: "This is the same exchange scanned twice, first for tones, then for carriers. \
               They look very different. Can someone explain the \"grid\" pattern in the \
               carrier scan 8000-9999?",
        log: None,
    },
    Provenance {
        file: "SAMPLE8B.DAT",
        note: "This is the same exchange scanned twice, first for tones, then for carriers. \
               They look very different. Can someone explain the \"grid\" pattern in the \
               carrier scan 8000-9999?",
        log: None,
    },
    Provenance {
        file: "SAMPLE9.DAT",
        note: "Tone scanning doesn't always work well, even with the right kind of modem. Any \
               real tones are here obscured by false responses.",
        log: None,
    },
    Provenance {
        file: "SAMPLE10.DAT",
        note: "Here's an exchange with many carriers. This is what carrier logging is for.",
        log: None,
    },
    Provenance {
        file: "SAMPLE11.DAT",
        note: "An exchange with many carriers in one band.",
        log: None,
    },
    Provenance {
        file: "SAMPLE12.DAT",
        note: "Notice how this exchange fades off towards the bottom in places. We've seen \
               this a lot; perhaps low numbers are allocated first?",
        log: None,
    },
    Provenance {
        file: "562XXXX.DAT",
        note: "Not part of the annotated sample set: a working file left in the distribution. \
               Written by ToneLoc 0.99, which the released 1.00 would itself refuse to open.",
        // TONE.LOG in the distribution is this scan's log, and the only
        // surviving provenance record of an actual ToneLoc session.
        log: Some(
            "TONE.LOG, 07-Jun-94: started on COM1 (16450 UART), modem init failed three \
             times, exited with errorlevel 1. Restarted on COM2 (16550A UART) at 22:50, \
             mask 562XXXX, scanning for Carriers, quiet mode with the speaker off. One \
             number dialed — 5629490 — and escaped after twelve seconds.",
        ),
    },
];

/// Look up what is known about a scan file, by name.
///
/// Matching ignores case and any directory part, so a path from the command
/// line works directly.
pub fn lookup(path: &str) -> Option<&'static Provenance> {
    let name = path
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(path)
        .to_ascii_uppercase();
    ARCHIVE.iter().find(|p| p.file == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_shipped_sample_is_annotated() {
        // Twelve numbered samples, with 8 split in two, plus the stray file.
        assert_eq!(ARCHIVE.len(), 14);
        for p in ARCHIVE {
            assert!(p.file.ends_with(".DAT"), "{} is not a .DAT", p.file);
            assert!(!p.note.is_empty(), "{} has no note", p.file);
        }
    }

    #[test]
    fn lookup_handles_paths_and_casing() {
        assert_eq!(lookup("SAMPLE5.DAT").unwrap().file, "SAMPLE5.DAT");
        assert_eq!(lookup("sample5.dat").unwrap().file, "SAMPLE5.DAT");
        assert_eq!(lookup("reference/sample5.dat").unwrap().file, "SAMPLE5.DAT");
        assert_eq!(
            lookup("C:\\TONELOC\\SAMPLE5.DAT").unwrap().file,
            "SAMPLE5.DAT"
        );
        assert!(lookup("MYSCAN.DAT").is_none());
    }

    #[test]
    fn the_one_surviving_session_log_is_attached_to_its_scan() {
        let p = lookup("562XXXX.DAT").unwrap();
        let log = p.log.expect("562XXXX has a surviving log");
        assert!(log.contains("07-Jun-94"));
        assert!(log.contains("5629490"));
        // No other file claims a log, because no other log survived.
        assert_eq!(ARCHIVE.iter().filter(|p| p.log.is_some()).count(), 1);
    }

    #[test]
    fn the_authors_unanswered_question_is_preserved() {
        // SAMPLES.DOC asks something nobody ever answered. Keeping the
        // question is part of keeping the artifact.
        let p = lookup("SAMPLE8A.DAT").unwrap();
        assert!(p.note.contains("Can someone explain"));
    }
}
