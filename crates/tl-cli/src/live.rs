//! Watching a scan run.
//!
//! The engine dials through a [`ReplayTransport`], so every result on screen is
//! one that was really recorded — this is a 1993 scan happening again, not a
//! simulation of one. The screen is ToneLoc's own, redrawn on each dial.
//!
//! The pacing is ours. A real scan at 216 dials an hour took two days; the
//! speed control exists so you can watch it in a minute or sit with it at
//! something close to the original crawl.

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::{cursor, execute, terminal};
use std::fmt::Write as _;
use std::io::Write;
use std::time::{Duration, Instant};
use tl_core::{Cell, CellClass, DatFile, DialOrder, Mask, ScanSequence};
use tl_modem::{ModemResponse, ModemTransport, ReplayTransport, ResponseStrings};
use tl_tui::{ScanType, ScreenState};

/// Called when a scan turns up a tone or a carrier, so it can be heard.
pub type FoundSound = Box<dyn FnMut(&str, Cell)>;

/// The speaker's mute switch, shared with whatever is doing the playing.
///
/// This loop knows a key was pressed and nothing whatever about sound cards,
/// so it flips a flag and lets the audio side act on it. Muting silences the
/// speaker only: a `--record` capture is a rendering rather than a recording
/// of the speaker, so it stays complete either way.
pub type MuteSwitch = std::sync::Arc<std::sync::atomic::AtomicBool>;

/// Fewest terminal rows the screen can live in.
///
/// Twenty-four: the full screen is 25 rows, but its last row is blank, so at
/// 24 the status line simply takes the place of the credit line.
const MIN_ROWS: usize = 24;

/// Dials per second the replay can run at.
const SPEEDS: [f32; 8] = [1.0, 2.0, 5.0, 10.0, 25.0, 50.0, 100.0, 400.0];
const DEFAULT_SPEED: usize = 3;

/// How a run ended.
pub enum Outcome {
    Finished,
    Quit,
}

pub struct Replay {
    mask: Option<Mask>,
    sequence: ScanSequence,
    transport: ReplayTransport,
    strings: ResponseStrings,
    state: ScreenState,
    /// What has been dialed so far — the grid as it fills in.
    progress: DatFile,
    position: usize,
    speed: usize,
    paused: bool,
    started: Instant,
    /// Simulated clock, in seconds since the scan began.
    scan_clock: u32,
    seconds_per_dial: u32,
}

impl Replay {
    pub fn new(dat: DatFile, path: &str, order: DialOrder) -> Replay {
        let mask = Mask::from_filename(path);

        // Only replay numbers the original actually dialed. Undialed cells
        // were never called, and inventing calls for them would be fabrication.
        let dat_for_plan = dat.clone();
        let sequence = ScanSequence::plan_all(order, move |n| !dat_for_plan.get(n).is_tried());

        let stats = dat.stats();
        let scan_type = if stats.carriers >= stats.tones {
            ScanType::Carriers
        } else {
            ScanType::Tones
        };

        // The recorded duration, spread over the numbers actually dialed.
        let total_seconds = dat.header.minutes as u32 * 60;
        let seconds_per_dial = if !sequence.is_empty() {
            (total_seconds / sequence.len() as u32).max(1)
        } else {
            14
        };

        Replay {
            transport: ReplayTransport::new(dat.clone()),
            state: ScreenState {
                mask: mask.as_ref().map(|m| m.to_string()).unwrap_or_default(),
                scan_type,
                started: "22:00:00".into(),
                current: "22:00:00".into(),
                max_dials: sequence.len() as u32,
                ..Default::default()
            },
            progress: DatFile::new(),
            mask,
            sequence,
            strings: ResponseStrings::default(),
            position: 0,
            speed: DEFAULT_SPEED,
            paused: false,
            started: Instant::now(),
            scan_clock: 0,
            seconds_per_dial,
        }
    }

    fn render_number(&self, n: u16) -> String {
        match &self.mask {
            Some(m) => m.apply(n as u32),
            None => format!("{n:04}"),
        }
    }

    fn clock(&self, seconds: u32) -> String {
        format!(
            "{:02}:{:02}:{:02}",
            (22 + seconds / 3600) % 24,
            (seconds / 60) % 60,
            seconds % 60
        )
    }

    /// Dial one number: send, drain the responses, record what came back.
    ///
    /// This is the scan state machine in miniature — count `RINGING`, take the
    /// first verdict, and fall back to a ringout or timeout on silence.
    fn dial_one(&mut self) -> Option<(u16, Cell)> {
        let number = *self.sequence.numbers().get(self.position)?;
        self.position += 1;

        let dialed = self.render_number(number);
        self.transport.send(&format!("ATDT{dialed}W;")).ok()?;

        let mut rings = 0u8;
        let mut verdict = None;
        while let Ok(Some(line)) = self.transport.poll() {
            match self.strings.classify(&line) {
                ModemResponse::Ringing => rings += 1,
                other => verdict = Some(other),
            }
        }

        let cell = match verdict {
            Some(r) => r.to_cell(rings).unwrap_or(Cell::UNDIALED),
            None if rings > 0 => ModemResponse::ringout(rings),
            None => CellClass::Timeout.with_rings(0),
        };

        self.progress.set(number, cell);
        self.scan_clock += self.seconds_per_dial;

        // Counters.
        self.state.tried += 1;
        match cell.class() {
            CellClass::Voice => self.state.voice += 1,
            CellClass::Busy => self.state.busy += 1,
            CellClass::Ringout => self.state.rings += 1,
            CellClass::Carrier => {
                if self.state.scan_type == ScanType::Carriers {
                    self.state.found_count += 1;
                }
                self.state.found.push(dialed.clone());
            }
            CellClass::Tone => {
                if self.state.scan_type == ScanType::Tones {
                    self.state.found_count += 1;
                }
                self.state.found.push(dialed.clone());
            }
            _ => {}
        }

        // The activity log, in the original's message formats.
        let outcome = match cell.class() {
            CellClass::Carrier => "* CARRIER *".to_string(),
            CellClass::Tone => "** TONE **".to_string(),
            CellClass::Busy => "Busy".to_string(),
            CellClass::Voice => format!("Voice   ({rings})"),
            CellClass::Ringout => format!("Ringout ({rings})"),
            CellClass::Timeout => format!("Timeout ({rings})"),
            other => other.to_string(),
        };
        self.state.activity.push(format!(
            "{} {dialed} - {outcome}",
            self.clock(self.scan_clock)
        ));
        if self.state.activity.len() > 64 {
            self.state.activity.drain(..32);
        }

        // The modem window shows the exchange.
        self.state.modem = vec![
            format!("ATDT{dialed}W;"),
            String::new(),
            match cell.class() {
                CellClass::Carrier => "CONNECT 14400".into(),
                CellClass::Tone => "OK".into(),
                CellClass::Busy => "BUSY".into(),
                CellClass::Voice => "VOICE".into(),
                _ => "NO CARRIER".into(),
            },
        ];

        self.state.current = self.clock(self.scan_clock);
        self.state.ring = format!("{rings}/4");
        self.state.secs = self.seconds_per_dial;
        let elapsed_minutes = (self.scan_clock / 60).max(1);
        self.state.dials_per_hour = self.state.tried * 60 / elapsed_minutes;
        let remaining = (self.sequence.len() - self.position) as u32 * self.seconds_per_dial / 60;
        self.state.eta = format!("{}:{:02}", remaining / 60, remaining % 60);

        Some((number, cell))
    }

    fn status_line(&self) -> String {
        let pct = if !self.sequence.is_empty() {
            self.position * 100 / self.sequence.len()
        } else {
            100
        };
        format!(
            " {}  {:.0} dials/sec  {}%  —  space pause · +/- speed · m mute · q quit ",
            if self.paused { "PAUSED " } else { "RUNNING" },
            SPEEDS[self.speed],
            pct,
        )
    }
}

/// Run the replay until it finishes or the operator quits.
///
/// Returns how it ended, plus the grid as it was filled in — so the caller can
/// show the ToneMap of what just happened.
pub fn run(
    dat: DatFile,
    path: &str,
    order: DialOrder,
    mut sound: Option<FoundSound>,
    mute: Option<MuteSwitch>,
) -> Result<(Outcome, Replay)> {
    let mut replay = Replay::new(dat, path, order);
    let mut stdout = std::io::stdout();

    // ToneLoc's layout is 80 x 25 and cannot be compressed — every coordinate
    // in it is relative to those dimensions. Say so plainly rather than
    // scribbling a scrolling mess over a terminal that is too small.
    let (cols, rows) = terminal::size().unwrap_or((80, 24));
    if (cols as usize) < tl_tui::screen::COLS {
        anyhow::bail!(
            "the ToneLoc screen is {} columns wide; this terminal is {cols}. \n\
             Widen the window and try again.",
            tl_tui::screen::COLS
        );
    }
    if (rows as usize) < MIN_ROWS {
        let short = MIN_ROWS - rows as usize;
        anyhow::bail!(
            "the ToneLoc screen needs {MIN_ROWS} rows; this terminal has {rows}.\n\
             Make the window {short} row{} taller and try again.",
            if short == 1 { "" } else { "s" }
        );
    }

    terminal::enable_raw_mode()?;
    execute!(stdout, terminal::EnterAlternateScreen, cursor::Hide)?;

    let result = run_loop(&mut replay, &mut stdout, &mut sound, mute.as_ref());

    // Always restore the terminal, even if the loop failed. Leaving someone in
    // raw mode with a hidden cursor is a rude way to crash.
    let _ = execute!(stdout, cursor::Show, terminal::LeaveAlternateScreen);
    let _ = terminal::disable_raw_mode();

    result.map(|outcome| (outcome, replay))
}

fn run_loop(
    replay: &mut Replay,
    stdout: &mut std::io::Stdout,
    sound: &mut Option<FoundSound>,
    mute: Option<&MuteSwitch>,
) -> Result<Outcome> {
    let mut next_dial = Instant::now();
    let mut last_draw = Instant::now() - Duration::from_secs(1);

    loop {
        // --- input ---------------------------------------------------------
        while event::poll(Duration::from_millis(0))? {
            // Read each event exactly once. Reading twice per poll blocks on
            // the second call, because there is nothing left to read.
            match event::read()? {
                Event::Key(key) => match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => return Ok(Outcome::Quit),
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        return Ok(Outcome::Quit);
                    }
                    KeyCode::Char(' ') => replay.paused = !replay.paused,
                    KeyCode::Char('+') | KeyCode::Char('=') => {
                        replay.speed = (replay.speed + 1).min(SPEEDS.len() - 1)
                    }
                    KeyCode::Char('-') | KeyCode::Char('_') => {
                        replay.speed = replay.speed.saturating_sub(1)
                    }
                    KeyCode::Char('m') => {
                        if let Some(sw) = mute {
                            use std::sync::atomic::Ordering;
                            sw.store(!sw.load(Ordering::Relaxed), Ordering::Relaxed);
                        }
                    }
                    _ => {}
                },
                Event::Resize(..) => {
                    // Wipe the old frame; shrinking leaves debris behind.
                    let _ = execute!(stdout, terminal::Clear(terminal::ClearType::All));
                    last_draw = Instant::now() - Duration::from_secs(1);
                    draw(replay, stdout, false)?;
                }
                _ => {}
            }
        }

        // --- dial ----------------------------------------------------------
        let mut dialed_this_tick = false;
        if !replay.paused {
            let interval = Duration::from_secs_f32(1.0 / SPEEDS[replay.speed]);
            while Instant::now() >= next_dial {
                match replay.dial_one() {
                    Some((number, cell)) => {
                        dialed_this_tick = true;
                        // Always hand a find to the sound side, muted or not.
                        // It decides whether the speaker hears it, and a
                        // `--record` capture stays whole either way.
                        if cell.is_hit() {
                            if let Some(play) = sound.as_mut() {
                                play(&replay.render_number(number), cell);
                            }
                        }
                    }
                    None => {
                        draw(replay, stdout, true)?;
                        // Let the finished screen sit until a key is pressed.
                        wait_for_key()?;
                        return Ok(Outcome::Finished);
                    }
                }
                next_dial += interval;
                // Don't let a slow frame turn into a burst of catch-up dials.
                if Instant::now().duration_since(next_dial) > Duration::from_millis(250) {
                    next_dial = Instant::now();
                    break;
                }
            }
        } else {
            next_dial = Instant::now();
        }

        // --- draw, capped at ~30fps ---------------------------------------
        if dialed_this_tick && last_draw.elapsed() >= Duration::from_millis(33) {
            draw(replay, stdout, false)?;
            last_draw = Instant::now();
        }

        std::thread::sleep(Duration::from_millis(2));
    }
}

fn draw(replay: &Replay, stdout: &mut std::io::Stdout, finished: bool) -> Result<()> {
    let (cols, rows) = terminal::size().unwrap_or((80, 24));
    let rows = rows as usize;

    let mut frame = String::with_capacity(32 * 1024);

    // Position every row explicitly and never emit a newline.
    //
    // Two hazards otherwise, and both were biting: an 80-character row written
    // into an 80-column terminal leaves the cursor pending at the right margin,
    // and a newline after the final row scrolls the entire display. Together
    // they shifted the screen by a line on every frame.
    let screen = tl_tui::screen::render_rows(&replay.state);
    let visible = rows.min(tl_tui::screen::ROWS);
    for (i, line) in screen.iter().take(visible).enumerate() {
        let _ = write!(frame, "\x1b[{};1H{line}", i + 1);
    }

    let status = if finished {
        format!(
            " FINISHED  {} dialed, {} found  —  press any key ",
            replay.state.tried, replay.state.found_count
        )
    } else {
        replay.status_line()
    };

    // The status line goes on the row after the screen, or over the credit
    // line when the terminal is exactly tall enough and no taller. Padding
    // stops one short of the last column: writing into the very last cell of
    // the last row scrolls some terminals.
    let status_row = rows.min(tl_tui::screen::ROWS + 1);
    let width = (cols as usize).saturating_sub(1);
    let _ = write!(
        frame,
        "\x1b[{status_row};1H\x1b[7m{:<width$}\x1b[0m",
        truncate(&status, width)
    );

    stdout.write_all(frame.as_bytes())?;
    stdout.flush()?;
    Ok(())
}

fn truncate(s: &str, width: usize) -> String {
    s.chars().take(width).collect()
}

fn wait_for_key() -> Result<()> {
    loop {
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(_) = event::read()? {
                return Ok(());
            }
        }
    }
}

impl Replay {
    /// The grid as filled in so far — for a ToneMap after the run.
    pub fn filled(&self) -> &DatFile {
        &self.progress
    }

    /// Wall-clock time the replay itself took.
    pub fn elapsed(&self) -> Duration {
        self.started.elapsed()
    }

    /// Numbers dialed, and finds, for the closing summary.
    pub fn summary(&self) -> (u32, u32) {
        (self.state.tried, self.state.found_count)
    }
}
