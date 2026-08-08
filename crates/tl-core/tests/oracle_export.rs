//! Dev-only export for diffing against the original binaries under DOSBox.
//!
//! The strongest fidelity check available is the original program judging our
//! output: write a `.DAT` we serialized, open it in the real `TONEMAP.EXE`, and
//! see whether it renders the same map. That last step is a human looking at
//! two screens, so it cannot run in CI — but the export half can be automated,
//! and this is it.
//!
//! Deliberately **not** a shipped capability. toneloc-ish has no `.DAT` writer:
//! it is a simulator, it never conducts a real scan, and the archival files are
//! primary sources that no code path should be able to open for writing. This
//! test writes only into a fresh temp directory, and only when asked.
//!
//! ```sh
//! cargo test -p tl-core --test oracle_export -- --ignored --nocapture
//! ```
//!
//! Then, in DOSBox, mount the printed directory and compare:
//!
//! ```text
//! TONEMAP SAMPLE5.DAT          # the original's rendering
//! TEXTMAP SAMPLE5 OUT.MAP      # ...or its text rendering, to diff by eye
//! ```
//!
//! against `toneloc-ish tonemap` and `toneloc-ish tonemap --text` on the same
//! file. Any structural divergence is a fidelity bug.

use std::path::PathBuf;
use tl_core::DatFile;

fn reference_dir() -> Option<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()?
        .parent()?
        .join("reference");
    dir.is_dir().then_some(dir)
}

#[test]
#[ignore = "dev workflow: writes files for manual comparison under DOSBox"]
fn export_round_tripped_samples_for_dosbox() {
    let Some(dir) = reference_dir() else {
        eprintln!("reference/ not present; nothing to export");
        return;
    };

    // A fresh directory under the OS temp dir. Never anywhere near the archive.
    let out = std::env::temp_dir().join("toneloc-ish-oracle");
    if out.exists() {
        std::fs::remove_dir_all(&out).expect("clearing the previous export");
    }
    std::fs::create_dir_all(&out).expect("creating the export directory");

    let mut exported = 0;
    for entry in std::fs::read_dir(&dir).expect("reading reference/") {
        let path = entry.expect("a directory entry").path();
        if !path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("DAT"))
        {
            continue;
        }

        let original = std::fs::read(&path).expect("reading an archival file");
        let dat = DatFile::parse(&original).expect("parsing an archival file");
        let written = dat.to_bytes();

        // If these ever differ, golden.rs has already failed and this export
        // would be exercising a known-broken serializer.
        assert_eq!(
            written,
            original,
            "{} does not round-trip; fix that before trusting a visual diff",
            path.display()
        );

        let name = path.file_name().expect("a file name");
        std::fs::write(out.join(name), &written).expect("writing the export");
        exported += 1;
    }

    // Also drop our own text rendering next to each file, so the comparison
    // against TEXTMAP.EXE is a plain diff rather than a squint.
    for entry in std::fs::read_dir(&out).expect("reading the export directory") {
        let path = entry.expect("a directory entry").path();
        if !path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("DAT"))
        {
            continue;
        }
        let dat = DatFile::parse(&std::fs::read(&path).expect("reading back")).expect("parsing");
        let mut text = String::with_capacity(10_200);
        for row in 0..tl_core::GRID_SIDE {
            for col in 0..tl_core::GRID_SIDE {
                text.push(dat.at(col, row).textmap_char());
            }
            text.push('\n');
        }
        std::fs::write(path.with_extension("OURS.MAP"), text).expect("writing our rendering");
    }

    eprintln!(
        "\nExported {exported} round-tripped .DAT files (plus our .OURS.MAP renderings) to:\n  \
         {}\n\nMount that directory in DOSBox alongside TONEMAP.EXE / TEXTMAP.EXE and compare.",
        out.display()
    );
    assert!(exported > 0, "found no .DAT files to export");
}
