//! The authors' claims about their own scans, as tests.
//!
//! `SAMPLES.DOC` describes each shipped scan in words. This file turns those
//! descriptions into assertions against the actual bytes. Every test names the
//! claim it is checking.
//!
//! Two things are being pinned at once. The obvious one is that we read the
//! data correctly — if column ordering or class mapping were wrong, "0-4000 is
//! residential, business from 4000-9999" would not reproduce. The less obvious
//! one is the measuring apparatus itself: `tl_core::structure` will later be
//! the yardstick for the synthetic exchange, and it is worth much more having
//! been calibrated against real scans with the authors' own labels before any
//! generator exists to be flattered by it.
//!
//! Skips when `reference/` is absent; see `golden.rs`.

use tl_core::structure::{Character, Columns, banding, profile, row_band_fraction};
use tl_core::{CellClass, DatFile};

fn load(name: &str) -> Option<DatFile> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()?
        .parent()?
        .join("reference")
        .join(name);
    let bytes = std::fs::read(path).ok()?;
    Some(DatFile::parse(&bytes).expect("archival file should parse"))
}

/// > A split exchange. 0-4000 is residential, business from 4000-9999.
///
/// The sharpest claim in the document, and the one that most directly tests
/// column ordering: it is a statement about *where* structure sits.
#[test]
fn sample2_is_residential_below_4000_and_business_above() {
    let Some(dat) = load("SAMPLE2.DAT") else {
        return;
    };

    let residential = profile(&dat, Columns::for_numbers(0, 4000));
    let business = profile(&dat, Columns::for_numbers(4000, 10000));

    assert_eq!(
        residential.character(),
        Character::Residential,
        "0-4000 measured {:.3} banding",
        residential.banding
    );
    assert_eq!(
        business.character(),
        Character::Business,
        "4000-9999 measured {:.3} banding",
        business.banding
    );

    // Not just either side of a threshold — a large, unambiguous gap.
    assert!(
        business.banding > residential.banding * 3.0,
        "expected the business half to be far more banded: {:.3} vs {:.3}",
        business.banding,
        residential.banding
    );
}

/// > A generic residential exchange. Few carriers.
#[test]
fn sample1_is_residential_with_few_carriers() {
    let Some(dat) = load("SAMPLE1.DAT") else {
        return;
    };
    let p = profile(&dat, Columns::ALL);

    assert_eq!(
        p.character(),
        Character::Residential,
        "banding {:.3}",
        p.banding
    );
    assert!(
        !p.has_carrier_band(),
        "a residential prefix has no carrier band"
    );

    // "Few" — well under one percent of the prefix.
    assert!(p.carrier < 0.01, "carrier fraction was {:.4}", p.carrier);
}

/// > An exchange with many carriers in one band.
///
/// Both halves matter: many, *and* concentrated.
#[test]
fn sample11_has_many_carriers_concentrated_in_one_band() {
    let (Some(many), Some(few)) = (load("SAMPLE11.DAT"), load("SAMPLE1.DAT")) else {
        return;
    };
    let p = profile(&many, Columns::ALL);
    let baseline = profile(&few, Columns::ALL);

    assert!(
        p.carrier > baseline.carrier * 5.0,
        "expected many more carriers than the residential sample: {:.4} vs {:.4}",
        p.carrier,
        baseline.carrier
    );

    assert!(
        p.has_carrier_band(),
        "carriers should clump; peak column was only {:.3}",
        p.peak_carrier_column
    );
    // In practice one column is almost entirely carriers.
    assert!(
        p.peak_carrier_column > 0.5,
        "peak carrier column was {:.3} at column {}",
        p.peak_carrier_column,
        p.peak_carrier_index
    );
}

/// > Here's an exchange with many carriers. This is what carrier logging is for.
#[test]
fn sample10_is_the_carrier_rich_one() {
    let Some(dat) = load("SAMPLE10.DAT") else {
        return;
    };
    let p = profile(&dat, Columns::ALL);

    // The richest of the whole archive by carrier fraction.
    let richest = ["SAMPLE1", "SAMPLE2", "SAMPLE4", "SAMPLE5", "SAMPLE11"]
        .iter()
        .filter_map(|n| load(&format!("{n}.DAT")))
        .map(|d| profile(&d, Columns::ALL).carrier)
        .fold(0.0f64, f64::max);

    assert!(
        p.carrier > richest,
        "SAMPLE10 should hold the most carriers: {:.4} vs {:.4}",
        p.carrier,
        richest
    );
}

/// > This is the same exchange scanned twice, first for tones, then for
/// > carriers. They look very different.
///
/// A tone scan asks "is there a dialtone?" and mostly hears nothing; a carrier
/// scan lets the line ring and mostly reaches people. Same numbers, opposite
/// distributions.
#[test]
fn sample8a_and_8b_are_the_same_exchange_and_look_nothing_alike() {
    let (Some(tone_scan), Some(carrier_scan)) = (load("SAMPLE8A.DAT"), load("SAMPLE8B.DAT")) else {
        return;
    };

    let a = profile(&tone_scan, Columns::ALL);
    let b = profile(&carrier_scan, Columns::ALL);

    // The tone scan is dominated by silence.
    assert!(
        a.timeout > 0.5,
        "tone scan timeout fraction {:.3}",
        a.timeout
    );
    assert!(
        a.voice < 0.05,
        "a tone scan barely hears voices: {:.3}",
        a.voice
    );

    // The carrier scan is dominated by people answering.
    assert!(b.voice > 0.5, "carrier scan voice fraction {:.3}", b.voice);
    assert!(
        b.voice > a.voice * 10.0,
        "the two should look nothing alike"
    );
}

/// > Notice how this exchange fades off towards the bottom in places. We've
/// > seen this a lot; perhaps low numbers are allocated first?
///
/// Read against SAMPLE4's note — "the voice ranges are unworking numbers" —
/// the fade should show up as more voice in the high rows, i.e. the last two
/// digits nearest 99.
#[test]
fn sample12_thins_out_toward_the_high_numbers() {
    let Some(dat) = load("SAMPLE12.DAT") else {
        return;
    };

    let low = row_band_fraction(&dat, 0..25, CellClass::Voice);
    let high = row_band_fraction(&dat, 75..100, CellClass::Voice);

    assert!(
        high > low,
        "expected the top of each block to be more allocated than the bottom: \
         rows 0-24 {:.3} vs rows 75-99 {:.3}",
        low,
        high
    );
    // A real gradient, not measurement noise.
    assert!(high - low > 0.05, "gradient was only {:.3}", high - low);
}

/// > Another mixed exchange, this time with wider bands.
/// > [SAMPLE5] is much like sample 4, but with a more typical blurring of
/// > boundaries between bands.
///
/// Both are banded; 4's bands are the crisper ones.
#[test]
fn sample4_is_more_sharply_banded_than_sample5() {
    let (Some(four), Some(five)) = (load("SAMPLE4.DAT"), load("SAMPLE5.DAT")) else {
        return;
    };

    let b4 = banding(&four, Columns::ALL);
    let b5 = banding(&five, Columns::ALL);

    assert!(b4 > b5, "SAMPLE4 {b4:.3} should exceed SAMPLE5 {b5:.3}");
    assert_eq!(
        profile(&four, Columns::ALL).character(),
        Character::Business
    );
}

/// The whole archive, sanity-checked in one place: the residential samples
/// really do band less than the business ones. If the measure ever stops
/// separating them, it has stopped measuring what it claims to.
#[test]
fn the_measure_separates_the_authors_two_categories() {
    let Some(residential) = load("SAMPLE1.DAT") else {
        return;
    };
    let residential = banding(&residential, Columns::ALL);

    for name in ["SAMPLE4.DAT", "SAMPLE5.DAT"] {
        let Some(dat) = load(name) else { return };
        let business = banding(&dat, Columns::ALL);
        assert!(
            business > residential,
            "{name} ({business:.3}) should band more than the residential \
             SAMPLE1 ({residential:.3})"
        );
    }
}

/// Every annotated file in the provenance table exists in the archive, and
/// every archival file is annotated. A missing entry means an artifact shipped
/// without its explanation.
#[test]
fn provenance_covers_the_archive_exactly() {
    let Some(dir) = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .map(|p| p.join("reference"))
        .filter(|p| p.is_dir())
    else {
        return;
    };

    let on_disk: std::collections::BTreeSet<String> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_uppercase())
        .filter(|n| n.ends_with(".DAT"))
        .collect();

    let annotated: std::collections::BTreeSet<String> = tl_core::provenance::ARCHIVE
        .iter()
        .map(|p| p.file.to_string())
        .collect();

    assert_eq!(
        on_disk, annotated,
        "the provenance table and the archive have drifted apart"
    );
}
