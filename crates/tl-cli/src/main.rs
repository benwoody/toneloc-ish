//! toneloc-ish — command line entry point.

mod live;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use std::path::{Path, PathBuf};
use tl_core::structure::{self, Columns};
use tl_core::{CellClass, DatFile, Mask, provenance};
use tl_tui::{MapStyle, ScanType, ScreenState, TextMapOptions, render_ansi, textmap};

#[derive(Parser, Debug)]
#[command(
    name = "toneloc-ish",
    version,
    about = "A historical simulator of the ToneLoc wardialer.",
    long_about = "toneloc-ish — a historical simulator of ToneLoc, the 1990s MS-DOS \
                  wardialer by Minor Threat & Mucho Maas.\n\n\
                  Relive the era: read and render real recorded scans, hear what a \
                  phone line sounded like, and watch a ToneMap fill in. No modem, no \
                  phone line, nothing dialed."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Render a .DAT scan file as a ToneMap.
    Tonemap {
        /// The .DAT file to read.
        file: PathBuf,

        /// Pack two columns per character. Halves the width, at the cost of
        /// horizontal detail. Chosen automatically if the terminal is tiny.
        #[arg(short, long)]
        narrow: bool,

        /// Draw the full 107-column map even if the terminal is narrower.
        /// It will wrap; useful when piping somewhere wider.
        #[arg(short, long, conflicts_with = "narrow")]
        wide: bool,

        /// Draw only the grid: no box, key, or rulers.
        #[arg(short, long)]
        bare: bool,

        /// Reproduce TEXTMAP.EXE's plain-text dump instead of the colour map.
        /// A linear strip of every number, not the grid.
        #[arg(short, long)]
        text: bool,

        /// Characters per line for --text, as TEXTMAP's -c. 110 lines the
        /// output up one ToneMap column per row.
        #[arg(long, default_value_t = 79, requires = "text")]
        columns: usize,
    },

    /// Watch a recorded scan run again, live, on ToneLoc's own screen.
    Replay {
        /// The .DAT scan to replay.
        file: PathBuf,

        /// Dial in number order instead of randomly, as /S did.
        #[arg(long)]
        sequential: bool,

        /// Seed for the random dial order, so a run reproduces exactly.
        #[arg(long, default_value_t = 0x10C, conflicts_with = "sequential")]
        seed: u64,

        /// Play the handshake when a tone or carrier is found.
        #[arg(long)]
        sound: bool,

        /// Write the session's audio to a WAV, placed at the moment each
        /// sound fired. Mux it onto a silent screen recording; no loopback
        /// device needed. Implies --sound.
        #[arg(long, value_name = "FILE")]
        record: Option<PathBuf>,
    },

    /// Show ToneLoc's screen as it looked mid-scan, driven by a real .DAT.
    Screen {
        /// The .DAT file to reconstruct a session from.
        file: PathBuf,

        /// How far into the scan to show, 0-100%.
        #[arg(long, default_value_t = 50, value_parser = clap::value_parser!(u8).range(0..=100))]
        progress: u8,
    },

    /// Show a scan file's header, provenance and findings.
    Info {
        /// The .DAT file to read.
        file: PathBuf,

        /// List every tone and carrier found.
        #[arg(short = 'H', long)]
        hits: bool,

        /// Prefix to print findings under, e.g. `555` for 555-XXXX.
        /// Inferred from the filename when it is itself a mask.
        #[arg(short, long)]
        prefix: Option<String>,
    },

    /// Play what a call sounded like. Every sound is synthesized from its
    /// real frequencies; there are no audio files in this project.
    Listen {
        /// The result to render the sound of.
        #[arg(value_enum, default_value_t = Outcome::Carrier)]
        outcome: Outcome,

        /// The number to dial, in DTMF.
        #[arg(short, long, default_value = "5559999")]
        number: String,

        /// Modulation standard for the handshake.
        #[arg(short, long, value_enum, default_value_t = Modulation::V32bis)]
        standard: Modulation,

        /// Write a WAV file instead of playing (works without a sound card).
        #[arg(short, long)]
        wav: Option<PathBuf>,
    },
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
enum Outcome {
    /// A modem answers: the full handshake.
    Carrier,
    /// A steady dialtone at the far end — a PBX or a loop.
    Tone,
    /// A busy signal.
    Busy,
    /// A person picked up.
    Voice,
    /// Rang until MaxRings.
    Ringout,
    /// Silence until WaitDelay expired.
    Timeout,
}

impl Outcome {
    fn cell(self) -> tl_core::Cell {
        match self {
            Outcome::Carrier => CellClass::Carrier.with_rings(1),
            Outcome::Tone => CellClass::Tone.with_rings(0),
            Outcome::Busy => CellClass::Busy.with_rings(0),
            Outcome::Voice => CellClass::Voice.with_rings(2),
            Outcome::Ringout => CellClass::Ringout.with_rings(4),
            Outcome::Timeout => CellClass::Timeout.with_rings(2),
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
enum Modulation {
    /// 2400 bps, 1984. Short and businesslike.
    V22bis,
    /// 14400 bps, 1991. The long screech everybody remembers.
    V32bis,
}

/// Write to stdout, treating a closed pipe as a clean exit.
///
/// Rust ignores SIGPIPE, so `toneloc-ish tonemap ... | head` makes every
/// subsequent `println!` panic with "failed printing to stdout". For a program
/// whose main output is thousands of lines, piping into `head` or quitting
/// `less` early is normal use, not an error.
fn emit(text: &str) -> Result<()> {
    use std::io::Write;
    let mut out = std::io::stdout().lock();
    match out.write_all(text.as_bytes()).and_then(|_| out.flush()) {
        Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => std::process::exit(0),
        r => r.map_err(Into::into),
    }
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Tonemap {
            file,
            narrow,
            wide,
            bare,
            text,
            columns,
        } => tonemap(&file, narrow, wide, bare, text, columns),
        Command::Replay {
            file,
            sequential,
            seed,
            sound,
            record,
        } => replay(&file, sequential, seed, sound, record.as_deref()),
        Command::Screen { file, progress } => screen(&file, progress),
        Command::Info { file, hits, prefix } => info(&file, hits, prefix.as_deref()),
        Command::Listen {
            outcome,
            number,
            standard,
            wav,
        } => listen(outcome, &number, standard, wav.as_deref()),
    }
}

/// Break text onto lines of at most `width` characters, at word boundaries.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        if !current.is_empty() && current.chars().count() + 1 + word.chars().count() > width {
            lines.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

/// How to get the archival scans, for someone who has just cloned this repo
/// and has nothing to point the program at.
const ARCHIVE_HINT: &str = "\
The archival scans are not bundled with toneloc-ish. They belong to the original
ToneLoc distribution, and keeping them out of this repository's history is
deliberate — it avoids tangling the original's provenance into ours.

Fetch them alongside this checkout:

    git clone https://github.com/steeve/ToneLoc reference

then try:

    toneloc-ish tonemap reference/SAMPLE5.DAT";

/// `.DAT` files sitting next to a path that was not found, for a typo hint.
fn siblings_of(path: &Path) -> Vec<String> {
    let dir = match path.parent() {
        Some(p) if p.as_os_str().is_empty() => Path::new("."),
        Some(p) => p,
        None => Path::new("."),
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.to_ascii_uppercase().ends_with(".DAT"))
        .collect();
    names.sort();
    names
}

/// Read and parse a `.DAT`, with errors that say what to do about it.
///
/// "No such file or directory" is a true statement and a useless one. Someone
/// who has just cloned this repository has no `.DAT` files at all and no way
/// to guess that they live in a separate, deliberately un-vendored checkout.
fn load(path: &Path) -> Result<DatFile> {
    if path.is_dir() {
        anyhow::bail!(
            "{} is a directory. Point me at a .DAT scan file inside it.",
            path.display()
        );
    }

    if !path.exists() {
        let siblings = siblings_of(path);
        if !siblings.is_empty() {
            // The directory is there and has scans — this is a typo.
            let mut listed = siblings
                .iter()
                .take(8)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ");
            if siblings.len() > 8 {
                listed.push_str(&format!(", and {} more", siblings.len() - 8));
            }
            anyhow::bail!(
                "{} does not exist.\n\nScan files in that directory: {listed}",
                path.display()
            );
        }
        anyhow::bail!("{} does not exist.\n\n{ARCHIVE_HINT}", path.display());
    }

    let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    DatFile::parse(&bytes).with_context(|| format!("parsing {}", path.display()))
}

fn tonemap(
    path: &Path,
    narrow: bool,
    wide: bool,
    bare: bool,
    text: bool,
    columns: usize,
) -> Result<()> {
    let dat = load(path)?;
    let title = path
        .file_name()
        .map(|s| s.to_string_lossy().to_uppercase())
        .unwrap_or_else(|| "SCAN".into());

    if text {
        let options = TextMapOptions {
            columns,
            key: !bare,
            ..Default::default()
        };
        emit(&textmap::render(&dat, options))?;
        emit("\n")?;

        // The original's loop stops one short. Say so rather than let a
        // carrier vanish quietly.
        if let Some((number, class)) = textmap::omits_a_find(&dat, options) {
            eprintln!(
                "note: TEXTMAP stops at 9998, so the {class} at {number:04} is not shown above. \
                 It is in the colour map."
            );
        }
        return Ok(());
    }

    let requested = MapStyle {
        chrome: !bare,
        key: !bare,
        rulers: !bare,
        narrow,
    };
    // Fit to the terminal unless told otherwise. A wrapped ToneMap is not a
    // smaller ToneMap; it is no ToneMap at all.
    let style = if wide {
        requested
    } else {
        requested.fit_to_terminal()
    };

    if !wide && !narrow && style.narrow {
        eprintln!(
            "note: terminal is under {} columns, so the map is at half width. \
             Widen the window, or use --wide to render full size anyway.",
            MapStyle::default().width()
        );
    }

    emit(&render_ansi(&dat, &title, style))
}

/// Watch a recorded scan run again.
fn replay(
    path: &Path,
    sequential: bool,
    seed: u64,
    sound: bool,
    record: Option<&Path>,
) -> Result<()> {
    let dat = load(path)?;
    let order = if sequential {
        tl_core::DialOrder::Forward
    } else {
        tl_core::DialOrder::Random { seed }
    };

    let (handler, recording) = make_sound_handler(sound || record.is_some(), record.is_some());
    let started = std::time::Instant::now();
    let (outcome, replay) = live::run(dat, &path.to_string_lossy(), order, handler)?;

    // Show what the run produced: the map, filled in by the replay itself.
    let title = path
        .file_name()
        .map(|s| s.to_string_lossy().to_uppercase())
        .unwrap_or_else(|| "SCAN".into());
    let style = MapStyle::default().fit_to_terminal();
    emit(&render_ansi(replay.filled(), &title, style))?;

    let (dialed, found) = replay.summary();
    let seconds = replay.elapsed().as_secs_f32();
    let verb = match outcome {
        live::Outcome::Finished => "Scan complete",
        live::Outcome::Quit => "Stopped",
    };
    emit(&format!(
        "\n{verb}: {dialed} numbers dialed, {found} found, in {seconds:.0}s of replay.\n"
    ))?;

    if let (Some(path), Some(recording)) = (record, recording) {
        write_recording(path, recording, started.elapsed().as_secs_f32())?;
    }
    Ok(())
}

/// Sounds to play, and optionally a timeline capturing them.
///
/// Recording does not need a sound card: the audio is generated either way, so
/// a headless machine can still produce the soundtrack.
#[cfg(feature = "audio")]
fn make_sound_handler(
    enabled: bool,
    record: bool,
) -> (Option<live::FoundSound>, Option<SharedRecorder>) {
    use std::sync::{Arc, Mutex};

    if !enabled {
        return (None, None);
    }

    let player = tl_audio::Player::new();
    let recording: Option<SharedRecorder> =
        record.then(|| Arc::new(Mutex::new(tl_audio::Recorder::new())));
    let sink = recording.clone();
    let start = std::time::Instant::now();

    let handler: live::FoundSound = Box::new(move |number: &str, cell: tl_core::Cell| {
        // Only finds get a sound. Playing every call would be a wall of noise
        // and would fall hopelessly behind a fast replay.
        let sounds = tl_audio::sound_for(number, cell, tl_audio::Standard::V32bis);
        let samples = tl_audio::render_all(&sounds);

        if let Some(sink) = &sink {
            if let Ok(mut sink) = sink.lock() {
                sink.add_at(start.elapsed().as_secs_f32(), &samples);
            }
        }
        if let Some(player) = &player {
            player.play(samples);
        }
    });

    (Some(handler), recording)
}

#[cfg(feature = "audio")]
type SharedRecorder = std::sync::Arc<std::sync::Mutex<tl_audio::Recorder>>;

#[cfg(feature = "audio")]
fn write_recording(path: &Path, recording: SharedRecorder, seconds: f32) -> Result<()> {
    let mut recorder = match std::sync::Arc::try_unwrap(recording) {
        Ok(lock) => lock.into_inner().unwrap_or_default(),
        // The handler outlived us; take a copy of what it has.
        Err(shared) => std::mem::take(&mut *shared.lock().unwrap()),
    };
    // Run the audio to the full length of the session so it lines up with a
    // screen recording of the same run.
    recorder.pad_to(seconds);
    let samples = recorder.finish();

    tl_audio::wav::write(path, &samples).with_context(|| format!("writing {}", path.display()))?;
    emit(&format!(
        "Wrote {} ({:.0}s).\n  Mux onto a silent capture with:\n    \
         ffmpeg -i screen.mp4 -i {} -c:v copy -shortest out.mp4\n",
        path.display(),
        samples.len() as f32 / tl_audio::SAMPLE_RATE as f32,
        path.display(),
    ))
}

#[cfg(not(feature = "audio"))]
type SharedRecorder = ();

#[cfg(not(feature = "audio"))]
fn make_sound_handler(_: bool, _: bool) -> (Option<live::FoundSound>, Option<SharedRecorder>) {
    (None, None)
}

#[cfg(not(feature = "audio"))]
fn write_recording(_: &Path, _: SharedRecorder, _: f32) -> Result<()> {
    anyhow::bail!("this build was compiled without the `audio` feature")
}

/// Reconstruct ToneLoc's screen from a recorded scan.
///
/// The `.DAT` stores results but not the order they were dialed in — ToneLoc
/// dialed randomly, and nothing recorded the sequence. So this walks the file
/// in number order and treats the first `progress`% as "done so far". The
/// counters, the finds and the meter are all real data from the file; only the
/// ordering is a reconstruction.
fn screen(path: &Path, progress: u8) -> Result<()> {
    let dat = load(path)?;
    let mask = Mask::from_filename(&path.to_string_lossy());
    let render_number = |n: u16| match &mask {
        Some(m) => m.apply(n as u32),
        None => format!("{n:04}"),
    };

    let cutoff = (tl_core::CELL_COUNT as u32 * progress as u32 / 100) as u16;

    // Timings come from the file: `Minutes` is the real recorded scan duration,
    // so the rate, the clock and the ETA are all derived from how long this
    // scan actually took rather than invented.
    let total_minutes = dat.header.minutes.max(1) as u32;
    let elapsed_minutes = total_minutes * progress as u32 / 100;
    let dialed_so_far = tl_core::CELL_COUNT as u32 * progress as u32 / 100;
    let rate = if elapsed_minutes > 0 {
        dialed_so_far * 60 / elapsed_minutes
    } else {
        0
    };
    // Scans ran overnight; 22:00 is as good a start as any.
    const START_HOUR: u32 = 22;
    let clock = |minutes: u32, seconds: u32| {
        let total = minutes * 60 + seconds;
        format!(
            "{:02}:{:02}:{:02}",
            (START_HOUR + total / 3600) % 24,
            (total / 60) % 60,
            total % 60
        )
    };
    let remaining = total_minutes.saturating_sub(elapsed_minutes);

    let mut state = ScreenState {
        mask: mask.as_ref().map(|m| m.to_string()).unwrap_or_default(),
        // Carriers dominate these files; the label follows the data.
        scan_type: if dat.stats().carriers >= dat.stats().tones {
            ScanType::Carriers
        } else {
            ScanType::Tones
        },
        started: clock(0, 0),
        current: clock(elapsed_minutes, 0),
        max_dials: tl_core::CELL_COUNT as u32,
        dials_per_hour: rate,
        eta: format!("{}:{:02}", remaining / 60, remaining % 60),
        ring: "2/4".into(),
        secs: 12,
        ..Default::default()
    };

    let mut recent = Vec::new();
    for number in 0..cutoff {
        let cell = dat.get(number);
        if !cell.is_tried() {
            continue;
        }
        state.tried += 1;
        match cell.class() {
            CellClass::Carrier => {
                state.found.push(render_number(number));
                if state.scan_type == ScanType::Carriers {
                    state.found_count += 1;
                }
            }
            CellClass::Tone => {
                state.found.push(render_number(number));
                if state.scan_type == ScanType::Tones {
                    state.found_count += 1;
                }
            }
            CellClass::Voice => state.voice += 1,
            CellClass::Busy => state.busy += 1,
            CellClass::Ringout => state.rings += 1,
            _ => {}
        }
        recent.push((number, cell));
    }

    // The activity log, in the original's own message formats.
    let tail = recent.len().saturating_sub(21);
    // Back-date the visible lines from "now" at the scan's own pace, so the
    // log agrees with the Started/Current clock beside it.
    let seconds_per_dial = if rate > 0 { 3600 / rate } else { 14 };
    for (i, (number, cell)) in recent[tail..].iter().enumerate() {
        let back = (recent.len() - tail - i) as u32 * seconds_per_dial;
        let stamp = clock(
            elapsed_minutes.saturating_sub(back / 60),
            59u32.saturating_sub(back % 60),
        );
        let outcome = match cell.class() {
            CellClass::Carrier => "* CARRIER *".to_string(),
            CellClass::Tone => "** TONE **".to_string(),
            CellClass::Busy => "Busy".to_string(),
            CellClass::Voice => format!("Voice   ({})", cell.rings()),
            CellClass::Ringout => format!("Ringout ({})", cell.rings()),
            CellClass::Timeout => format!("Timeout ({})", cell.rings()),
            other => other.to_string(),
        };
        state
            .activity
            .push(format!("{stamp} {} - {outcome}", render_number(*number)));
    }

    // The modem window shows the last exchange.
    if let Some((number, cell)) = recent.last() {
        state.modem.push(format!("ATDT{}", render_number(*number)));
        state.modem.push(String::new());
        state.modem.push(match cell.class() {
            CellClass::Carrier => "CONNECT 14400".into(),
            CellClass::Tone => "OK".into(),
            CellClass::Busy => "BUSY".into(),
            CellClass::Voice => "VOICE".into(),
            CellClass::Ringout | CellClass::Timeout => "NO CARRIER".into(),
            _ => String::new(),
        });
    }

    emit(&tl_tui::screen::render(&state))?;
    eprintln!(
        "note: the .DAT records results but not dial order — ToneLoc dialed randomly \
         and nothing logged the sequence. Counters and finds are real; the ordering \
         is a reconstruction."
    );
    Ok(())
}

fn info(path: &Path, list_hits: bool, prefix: Option<&str>) -> Result<()> {
    use std::fmt::Write as _;
    let mut o = String::new();
    let dat = load(path)?;
    let stats = dat.stats();
    let (hours, minutes) = dat.header.time_spent();

    // The .DAT header has nowhere to record what was dialed, so the filename
    // is the only surviving record — ToneLoc used it as the mask when you
    // gave it nothing else.
    let mask = Mask::from_filename(&path.to_string_lossy());
    let prefix = match (prefix, &mask) {
        (Some(explicit), _) => explicit.to_string(),
        (None, Some(m)) => m.prefix().to_string(),
        (None, None) => String::new(),
    };

    writeln!(o, "{}", path.display()).ok();
    writeln!(
        o,
        "  format        ToneLoc {} data file",
        dat.header.version_string()
    )
    .ok();
    if !dat.header.is_current() {
        writeln!(
            o,
            "                (ToneLoc 1.00 itself would ask you to run TCONVERT on this)"
        )
        .ok();
    }
    if let Some(m) = &mask {
        writeln!(
            o,
            "  mask          {m}  ({}-{})",
            m.apply(0),
            m.apply(m.count() - 1)
        )
        .ok();
    }
    writeln!(o, "  scan time     {hours}:{minutes:02}").ok();
    writeln!(o, "  dialed        {} of 10000", stats.tried).ok();
    writeln!(o, "  carriers      {}", stats.carriers).ok();
    writeln!(o, "  tones         {}", stats.tones).ok();
    writeln!(o, "  ringouts      {}", stats.rings).ok();
    writeln!(o, "  busys         {}", stats.busys).ok();
    writeln!(o, "  voices        {}", stats.voices).ok();

    if dat.header.extra != [0; 10] {
        writeln!(o, "  header extra  {:02x?}", dat.header.extra).ok();
    }

    // What the map looks like, measured rather than eyeballed.
    let whole = structure::profile(&dat, Columns::ALL);
    writeln!(o, "\n  structure").ok();
    match whole.character() {
        structure::Character::Unscanned => writeln!(
            o,
            "    character   too little dialed to say ({:.0}% of the prefix)",
            whole.coverage * 100.0
        ),
        character => writeln!(
            o,
            "    character   {character:?} (banding {:.3})",
            whole.banding
        ),
    }
    .ok();
    if whole.has_carrier_band() {
        let col = whole.peak_carrier_index;
        writeln!(
            o,
            "    carriers    banded at {:04}-{:04} ({:.0}% of that block)",
            col * 100,
            col * 100 + 99,
            whole.peak_carrier_column * 100.0
        )
        .ok();
    }

    // The authors' own commentary, where this is one of the shipped scans.
    if let Some(p) = provenance::lookup(&path.to_string_lossy()) {
        writeln!(o, "\n  from SAMPLES.DOC, by Minor Threat & Mucho Maas").ok();
        for line in wrap(p.note, 66) {
            writeln!(o, "    {line}").ok();
        }
        if let Some(log) = p.log {
            writeln!(o, "\n  surviving session log").ok();
            for line in wrap(log, 66) {
                writeln!(o, "    {line}").ok();
            }
        }
    }

    if list_hits {
        writeln!(o, "\n  findings").ok();
        let mut any = false;
        for (number, cell) in dat.hits() {
            any = true;
            // Render through the mask when we have one: a 3-wildcard mask
            // covers a thousand numbers, so a fixed four digits would be
            // wrong. Falls back to the bare cell index otherwise.
            let dialed = match (&mask, prefix.is_empty()) {
                (Some(m), true) => m.apply(number as u32),
                _ => format!("{prefix}{number:04}"),
            };
            let rings = match cell.rings() {
                0 => String::new(),
                1 => "  (1 ring)".to_string(),
                n => format!("  ({n} rings)"),
            };
            writeln!(o, "    {dialed}   {}{rings}", cell.class()).ok();
        }
        if !any {
            writeln!(o, "    none").ok();
        }
    }

    emit(&o)
}

#[cfg(feature = "audio")]
fn listen(outcome: Outcome, number: &str, standard: Modulation, wav: Option<&Path>) -> Result<()> {
    use std::fmt::Write as _;
    use tl_audio::{Standard, sound_for};
    let mut o = String::new();

    let standard = match standard {
        Modulation::V22bis => Standard::V22bis,
        Modulation::V32bis => Standard::V32bis,
    };
    // The dial string reaches the synthesizer verbatim, and anything that is
    // not a keypad character comes out as silence — so garbage plays as a
    // string of pauses rather than failing. Say so instead.
    //
    // Legal beyond the keypad: W (wait for dialtone), comma (pause),
    // ; (return to command mode), ! (flash), @ (wait for answer), and the
    // separators people write numbers with.
    let undialable: Vec<char> = number
        .chars()
        .filter(|c| {
            tl_audio::synth::dtmf_pair(*c).is_none()
                && !matches!(
                    c.to_ascii_uppercase(),
                    'W' | ',' | ';' | '!' | '@' | 'P' | 'T' | '-' | ' ' | '(' | ')' | '.'
                )
        })
        .collect();
    if !undialable.is_empty() {
        let list: String = undialable.iter().collect();
        anyhow::bail!(
            "{number:?} is not dialable: {list:?} are not keypad characters.\n\
             Use digits, A-D, * and #, plus W for a wait or a comma for a pause."
        );
    }

    let cell = outcome.cell();
    let sounds = sound_for(number, cell, standard);

    // Render each step separately so the timeline can show when each one
    // starts, then concatenate. Same samples either way.
    let rendered: Vec<_> = sounds.iter().map(|s| s.render()).collect();
    let samples: tl_audio::Samples = rendered.iter().flatten().copied().collect();
    let seconds = samples.len() as f32 / tl_audio::SAMPLE_RATE as f32;

    let rings = match cell.rings() {
        0 => String::new(),
        1 => ", 1 ring".into(),
        n => format!(", {n} rings"),
    };
    writeln!(
        o,
        "Dialing {number} — {}{rings}   {seconds:.1}s\n",
        cell.class()
    )
    .ok();

    let mut at = 0.0f32;
    for (sound, buffer) in sounds.iter().zip(&rendered) {
        writeln!(o, "{at:>6.1}s  {:<28} {}", sound.label(), sound.detail()).ok();
        at += buffer.len() as f32 / tl_audio::SAMPLE_RATE as f32;
    }

    emit(&o)?;

    if let Some(path) = wav {
        tl_audio::wav::write(path, &samples)
            .with_context(|| format!("writing {}", path.display()))?;
        writeln!(o, "\nWrote {}", path.display()).ok();
        return Ok(());
    }

    match tl_audio::Player::new() {
        Some(player) => {
            player.play(samples);
            // Dropping the player drains the queue before returning.
            drop(player);
        }
        None => {
            eprintln!(
                "\nNo audio device available. Re-run with --wav <file> to render \
                 the sound to a file instead."
            );
        }
    }
    Ok(())
}

#[cfg(not(feature = "audio"))]
fn listen(_: Outcome, _: &str, _: Modulation, _: Option<&Path>) -> Result<()> {
    anyhow::bail!("this build was compiled without the `audio` feature")
}
