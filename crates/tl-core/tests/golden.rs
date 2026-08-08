//! Golden tests against the archival scan files.
//!
//! The fourteen `.DAT` files shipped with the original are real recorded scans
//! from 1993. They are the oracle for the format, so these tests are the proof
//! that "this really does read ToneLoc data files" rather than "this reads
//! files that resemble them".
//!
//! The files live in `reference/`, a git-ignored clone of `steeve/ToneLoc`.
//! Keeping them out of our history is deliberate — it avoids tangling the
//! original's provenance into ours. When the clone is missing these tests skip
//! with instructions rather than failing, so a fresh checkout still runs green.
//!
//!     git clone https://github.com/steeve/ToneLoc reference

use std::path::PathBuf;
use tl_core::{Cell, CellClass, DAT_LEN, DatFile};

fn reference_dir() -> Option<PathBuf> {
    // CARGO_MANIFEST_DIR is crates/tl-core.
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()?
        .parent()?
        .join("reference");
    dir.is_dir().then_some(dir)
}

fn sample_files() -> Vec<PathBuf> {
    let Some(dir) = reference_dir() else {
        eprintln!(
            "skipping: reference/ not present. \
             Run: git clone https://github.com/steeve/ToneLoc reference"
        );
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("reading reference/")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("DAT")))
        .collect();
    files.sort();
    assert!(
        !files.is_empty(),
        "reference/ exists but contains no .DAT files"
    );
    files
}

#[test]
fn every_archival_file_parses() {
    let files = sample_files();
    if files.is_empty() {
        return;
    }
    for path in &files {
        let bytes = std::fs::read(path).unwrap();
        assert_eq!(
            bytes.len(),
            DAT_LEN,
            "{} is not 10016 bytes",
            path.display()
        );
        let dat = DatFile::parse(&bytes)
            .unwrap_or_else(|e| panic!("{} failed to parse: {e}", path.display()));
        assert_eq!(dat.header.product_code, *b"TL");
    }
    eprintln!("parsed {} archival scan files", files.len());
}

/// The load-bearing test: read → write → read must be byte-identical.
/// Everything else in the project assumes the format is exact.
#[test]
fn round_trip_is_byte_identical() {
    for path in sample_files() {
        let original = std::fs::read(&path).unwrap();
        let dat = DatFile::parse(&original).unwrap();
        let written = dat.to_bytes();

        assert_eq!(
            written.len(),
            original.len(),
            "{}: length changed",
            path.display()
        );
        if written != original {
            let first = written
                .iter()
                .zip(&original)
                .position(|(a, b)| a != b)
                .unwrap();
            panic!(
                "{}: byte {first} changed from {:#04x} to {:#04x}",
                path.display(),
                original[first],
                written[first]
            );
        }

        // ...and parsing what we wrote gives back an equal value.
        assert_eq!(DatFile::parse(&written).unwrap(), dat, "{}", path.display());
    }
}

/// Every byte in every archival file must map to a state we know about.
/// An `Unknown` here means the format has something we have not accounted for.
#[test]
fn every_byte_maps_to_a_documented_state() {
    let mut seen = std::collections::BTreeSet::new();
    for path in sample_files() {
        let dat = DatFile::parse(&std::fs::read(&path).unwrap()).unwrap();
        for (number, cell) in dat.cells().iter().enumerate() {
            assert_ne!(
                cell.class(),
                CellClass::Unknown,
                "{}: number {number:04} has undocumented byte {}",
                path.display(),
                cell.raw()
            );
            seen.insert(cell.raw());
        }
    }
    if seen.is_empty() {
        return;
    }
    eprintln!("distinct cell bytes across the archive: {}", seen.len());

    // Sanity-check a few we know are in there, so this test would notice if it
    // silently started reading empty files.
    for expected in [0u8, 10, 21, 63, 72, 80, 90] {
        assert!(
            seen.contains(&expected),
            "expected byte {expected} somewhere"
        );
    }
}

/// Cross-check our reading of the header against known values.
#[test]
fn headers_match_what_the_source_says_they_should_be() {
    let files = sample_files();
    if files.is_empty() {
        return;
    }
    for path in &files {
        let dat = DatFile::parse(&std::fs::read(path).unwrap()).unwrap();
        let name = path.file_name().unwrap().to_string_lossy();

        // No shipped version ever wrote anything but zeros to Extra.
        assert_eq!(dat.header.extra, [0; 10], "{name} has nonzero Extra bytes");

        // Version is either 1.00, or 0.99 for the one older file.
        let v = dat.header.version_id;
        assert!(
            v == tl_core::dat::VERSION_1_00 || v == tl_core::dat::VERSION_0_99,
            "{name} has unexpected VersionID {v:#06x}"
        );

        if name.eq_ignore_ascii_case("562XXXX.DAT") {
            // The oldest artifact in the set: a 0.99 file that ToneLoc 1.00
            // itself would have refused to open.
            assert_eq!(v, tl_core::dat::VERSION_0_99);
            assert!(!dat.header.is_current());
        }
    }
}

/// A completed scan should look like a completed scan.
#[test]
fn a_full_scan_has_the_shape_of_real_data() {
    let Some(dir) = reference_dir() else {
        return;
    };
    let path = dir.join("SAMPLE5.DAT");
    if !path.exists() {
        return;
    }
    let dat = DatFile::parse(&std::fs::read(&path).unwrap()).unwrap();
    let stats = dat.stats();

    assert_eq!(stats.tried, 10_000, "SAMPLE5 is a complete 10k scan");
    assert!(stats.carriers > 0 && stats.tones > 0, "it found things");
    // Voices and ringouts dominate any real prefix; finds are rare.
    assert!(
        stats.carriers + stats.tones < stats.voices,
        "finds should be far rarer than people answering"
    );
    // 56 hours of dialing, which is what an overnight scan across a week cost.
    let (hours, _) = dat.header.time_spent();
    assert!(hours > 24, "expected a multi-day scan, got {hours}h");

    // hits() agrees with the tally.
    assert_eq!(
        dat.hits().count() as u32,
        stats.tones + stats.carriers,
        "hits() and stats() disagree"
    );
}

/// The archive does not exercise the whole byte range, so pin round-tripping
/// on a synthetic file that does. Without this, coverage of the format is only
/// as wide as what people happened to dial in 1993.
#[test]
fn synthetic_file_covering_all_256_bytes_round_trips() {
    let mut dat = DatFile::new();
    for n in 0..tl_core::CELL_COUNT {
        dat.set(n as u16, Cell((n % 256) as u8));
    }
    dat.header.minutes = u16::MAX;
    dat.header.extra = [0xff; 10];

    let bytes = dat.to_bytes();
    assert_eq!(bytes.len(), DAT_LEN);
    assert_eq!(DatFile::parse(&bytes).unwrap(), dat);
    assert_eq!(DatFile::parse(&bytes).unwrap().to_bytes(), bytes);
}
