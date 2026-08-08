//! Playback, behind the `playback` feature.
//!
//! Everything above this file is pure computation. This is the only place that
//! touches a sound card, so a machine without one (CI, a headless box, a
//! terminal over ssh) can still build, test and render to WAV.

use crate::synth::{SAMPLE_RATE, Samples};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;

/// Sounds allowed to be waiting at once. Beyond this, new ones are dropped.
const MAX_QUEUED: usize = 2;

/// Longest we will wait on exit for playback to finish.
const DRAIN_LIMIT: Duration = Duration::from_secs(3);

/// An audio output you can push buffers at.
///
/// Playback runs on its own thread with a bounded queue, so a scan never
/// blocks on the speaker: if audio falls behind, sounds are dropped rather
/// than stalling the dial loop. The scan is the thing that has to keep time,
/// and quitting must not mean sitting through a backlog of handshakes.
#[derive(Debug)]
pub struct Player {
    tx: Option<Sender<Samples>>,
    handle: Option<thread::JoinHandle<()>>,
    muted: bool,
}

impl Player {
    /// Open the default output device.
    ///
    /// Returns `None` when there is no usable device — headless machines are
    /// normal here, and silence is a perfectly good fallback.
    pub fn new() -> Option<Player> {
        let (tx, rx): (Sender<Samples>, Receiver<Samples>) = mpsc::channel();

        // Probe the device on the audio thread: rodio's stream handle is not
        // Send, so it has to be created and owned where it is used.
        let (ready_tx, ready_rx) = mpsc::channel();
        let handle = thread::Builder::new()
            .name("tl-audio".into())
            .spawn(move || {
                let stream = match rodio::OutputStreamBuilder::open_default_stream() {
                    Ok(s) => {
                        let _ = ready_tx.send(true);
                        s
                    }
                    Err(_) => {
                        let _ = ready_tx.send(false);
                        return;
                    }
                };
                let sink = rodio::Sink::connect_new(stream.mixer());

                while let Ok(samples) = rx.recv() {
                    // Drop sounds when already behind rather than queueing
                    // them. A replay at 400 dials a second can generate
                    // minutes of handshake audio in seconds; queueing it all
                    // would leave the speaker running long after the scan
                    // ended, and make quitting wait for every last screech.
                    if sink.len() >= MAX_QUEUED {
                        continue;
                    }
                    sink.append(rodio::buffer::SamplesBuffer::new(1, SAMPLE_RATE, samples));
                }

                // The channel closed, so the session is over. Let whatever is
                // playing finish, but never wait indefinitely.
                let deadline = std::time::Instant::now() + DRAIN_LIMIT;
                while !sink.empty() && std::time::Instant::now() < deadline {
                    std::thread::sleep(std::time::Duration::from_millis(20));
                }
                sink.stop();
            })
            .ok()?;

        match ready_rx.recv() {
            Ok(true) => Some(Player {
                tx: Some(tx),
                handle: Some(handle),
                muted: false,
            }),
            _ => None,
        }
    }

    /// Queue samples for playback. Cheap, and never blocks.
    pub fn play(&self, samples: Samples) {
        if self.muted || samples.is_empty() {
            return;
        }
        if let Some(tx) = &self.tx {
            let _ = tx.send(samples);
        }
    }

    /// Silence the player without tearing down the device — the software
    /// equivalent of `ATM0`, which is what you did at 2am.
    pub fn set_muted(&mut self, muted: bool) {
        self.muted = muted;
    }

    pub fn is_muted(&self) -> bool {
        self.muted
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        // Close the channel so the audio thread finishes its queue and exits.
        self.tx.take();
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}
