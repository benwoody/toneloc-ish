//! Playback, behind the `playback` feature.
//!
//! Everything above this file is pure computation. This is the only place that
//! touches a sound card, so a machine without one (CI, a headless box, a
//! terminal over ssh) can still build, test and render to WAV.

use crate::synth::{SAMPLE_RATE, Samples};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::Duration;

/// Sounds allowed to be waiting at once. Beyond this, new ones are dropped.
const MAX_QUEUED: usize = 2;

/// How often the audio thread wakes to notice the mute switch. It is otherwise
/// blocked waiting for the next sound, which at the slowest pace is eighteen
/// seconds away, and `m` has to take effect while you are still holding it.
const MUTE_POLL: Duration = Duration::from_millis(50);

/// An audio output you can push buffers at.
///
/// Playback runs on its own thread with a bounded queue, so a scan never
/// blocks on the speaker: if audio falls behind, sounds are dropped rather
/// than stalling the dial loop. The scan is the thing that has to keep time,
/// and quitting must not mean sitting through a backlog of handshakes.
///
/// Dropping the player stops the speaker at once. It does not wait for a sound
/// to finish, because the only reason to drop it mid-scan is that someone
/// pressed `q` and wants their terminal back. A caller that does want the tail
/// of a sound (`listen` does, since the sound is the whole point) knows how
/// long it is and waits that long itself.
#[derive(Debug)]
pub struct Player {
    tx: Option<Sender<Samples>>,
    handle: Option<thread::JoinHandle<()>>,
    muted: Arc<AtomicBool>,
}

impl Player {
    /// Open the default output device.
    ///
    /// Returns `None` when there is no usable device — headless machines are
    /// normal here, and silence is a perfectly good fallback.
    pub fn new() -> Option<Player> {
        let (tx, rx): (Sender<Samples>, Receiver<Samples>) = mpsc::channel();
        let muted = Arc::new(AtomicBool::new(false));
        let thread_muted = Arc::clone(&muted);

        // Probe the device on the audio thread: rodio's stream handle is not
        // Send, so it has to be created and owned where it is used.
        let (ready_tx, ready_rx) = mpsc::channel();
        let handle = thread::Builder::new()
            .name("tl-audio".into())
            .spawn(move || {
                let mut stream = match rodio::OutputStreamBuilder::open_default_stream() {
                    Ok(s) => {
                        let _ = ready_tx.send(true);
                        s
                    }
                    Err(_) => {
                        let _ = ready_tx.send(false);
                        return;
                    }
                };
                // rodio writes a line to stderr when the stream drops. We own
                // the whole screen; it does not get a postscript on it.
                stream.log_on_drop(false);
                let sink = rodio::Sink::connect_new(stream.mixer());

                let mut speaker_off = false;
                loop {
                    let received = rx.recv_timeout(MUTE_POLL);

                    // Mute is the speaker switch, not a queue operation. Turn
                    // the volume down and leave the queue alone: what is
                    // playing goes quiet at once, and unmuting picks it back up
                    // wherever it has got to, which is what ATM0/ATM1 did.
                    //
                    // Emphatically not Sink::clear() here. That defers to a
                    // counter which is only decremented from a source's own
                    // periodic callback, so if the sound ends first the count
                    // survives and eats the *next* sound appended. Polling it
                    // the way this loop does made that near certain, and it
                    // presents as unmuting appearing to do nothing at all.
                    let want_off = thread_muted.load(Ordering::Relaxed);
                    if want_off != speaker_off {
                        sink.set_volume(if want_off { 0.0 } else { 1.0 });
                        speaker_off = want_off;
                    }

                    match received {
                        Ok(samples) => {
                            // Drop sounds when already behind rather than
                            // queueing them. A replay at 400 dials a second
                            // can generate minutes of handshake audio in
                            // seconds; queueing it all would leave the speaker
                            // running long after the scan ended.
                            if sink.len() >= MAX_QUEUED {
                                continue;
                            }
                            sink.append(rodio::buffer::SamplesBuffer::new(1, SAMPLE_RATE, samples));
                        }
                        Err(RecvTimeoutError::Timeout) => {}
                        Err(RecvTimeoutError::Disconnected) => break,
                    }
                }

                sink.stop();
            })
            .ok()?;

        match ready_rx.recv() {
            Ok(true) => Some(Player {
                tx: Some(tx),
                handle: Some(handle),
                muted,
            }),
            _ => None,
        }
    }

    /// Queue samples for playback. Cheap, and never blocks.
    ///
    /// Queues while muted too, and lets the speaker be the thing that is off.
    /// Dropping them here instead would mean unmuting bought you silence until
    /// the next find, which at the slowest pace is minutes away.
    pub fn play(&self, samples: Samples) {
        if samples.is_empty() {
            return;
        }
        if let Some(tx) = &self.tx {
            let _ = tx.send(samples);
        }
    }

    /// Silence the player without tearing down the device — the software
    /// equivalent of `ATM0`, which is what you did at 2am.
    pub fn set_muted(&self, muted: bool) {
        self.muted.store(muted, Ordering::Relaxed);
    }

    pub fn is_muted(&self) -> bool {
        self.muted.load(Ordering::Relaxed)
    }

    /// The mute switch on its own, so something that does not own the player
    /// can still silence it. The replay loop holds one of these: it knows a
    /// key was pressed, but nothing about sound cards.
    pub fn mute_switch(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.muted)
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        // Close the channel so the audio thread stops the sink and exits.
        self.tx.take();
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_switch_and_the_player_agree_and_sounds_queue_while_muted() {
        // No sound card (CI, a headless box) means nothing to check.
        let Some(player) = Player::new() else {
            return;
        };

        assert!(!player.is_muted());
        player.set_muted(true);
        assert!(player.is_muted());

        // Queued even while muted. Dropping them here would mean unmuting
        // bought silence until the next find rather than the sound in progress.
        player.play(vec![0.0; 256]);

        player.set_muted(false);
        assert!(!player.is_muted(), "mute must toggle back off");

        // The replay loop holds one of these rather than the player itself, so
        // the two have to be the same flag and not a copy of it.
        let switch = player.mute_switch();
        switch.store(true, Ordering::Relaxed);
        assert!(
            player.is_muted(),
            "the shared switch and the player have come apart"
        );
        switch.store(false, Ordering::Relaxed);
        assert!(!player.is_muted());
    }
}
