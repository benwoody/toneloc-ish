# toneloc-ish

*Loc-ed After Dark.*

**A historical simulator of the ToneLoc wardialer, relive the 1980s/90s
wardialing era, no phone line required.**

The original **ToneLoc** - "Tone Locator", and yes, named after the rapper - was
written for MS-DOS in 1994 by **Minor Threat & Mucho Maas**. It dialed every
number in a telephone prefix overnight and drew you a map of what answered.
This is a faithful Rust reconstruction, built for preservation. The `-ish` is
deliberate: it behaves like ToneLoc, it isn't ToneLoc.

```
╔═════════════┤ Activity Log ├══════════════╗ ╔═══════════┤ Modem ├════════════╗
║ 22:04:16 4576 - Ringout (4)               ║ ║ ATDT4363W;                     ║
║ 22:04:32 4190 - Ringout (4)               ║ ║ CONNECT 14400                  ║
║ 22:04:48 4363 - * CARRIER *               ║ ╚════════════════════════════════╝
║ 22:05:04 8571 - Voice   (0)               ║ ╔═════════┤ Statistics ├═════════╗
║ 22:05:20 5152 - Voice   (0)               ║ ║  Current: 22:05:20  Secs:   16 ║
║                                           ║ ║  Dials/Hour:  240   ETA: 44:21 ║
║                                           ║ ╟──────────────────┤ Found ├─────╢
║                                           ║ ║ CD's  :      1 ║ 4363          ║
║                                           ║ ║ Try # :     20 ║               ║
╚═══════════════════════════════════════════╝ ╚════════════════════════════════╝
 RUNNING  10 dials/sec  —  space pause · +/- speed · m mute · q quit
```

That's a real scan from 1993 running again. Every result on screen was recorded
by somebody, on a modem, thirty years ago.

## It does not dial anything

It can't. There's no modem in it and no code that could talk to one.

That's the point rather than a limitation. As a working wardialer this is a dead
use case: POTS lines are mostly gone; VoIP mangles carrier negotiation; and I
legally have to say scanning ranges you don't own is illegal-to-antisocial
regardless. But everything interesting about ToneLoc, the sequencing engine, the
mask arithmetic, the screen, and the patterns on the ToneMap, needs no telephony
at all. Take the phone line out and the good engineering problems all stay.

So where the modem used to be there's a simulation: **replay** a real scan and
watch the map fill in as it did the night it was made, or scan a procedurally
generated, period-accurate **synthetic exchange**.

## Try it

```sh
git clone https://github.com/benwoody/toneloc-ish
cd toneloc-ish
cargo build --release

# The archival scans stay out of this repo's history on purpose.
git clone https://github.com/steeve/ToneLoc reference

cargo run --release -- replay reference/SAMPLE11.DAT   # watch it run
cargo run --release -- replay reference/SAMPLE10.DAT --sound   # and hear it
cargo run --release -- tonemap reference/SAMPLE11.DAT  # the right-hand edge
cargo run --release -- info reference/562XXXX.DAT      # provenance, from 1994
cargo run --release -- listen carrier                  # the handshake, out loud
```

Ten thousand numbers land on one screen as a 100 × 100 grid, column-major, so a
PBX owning a contiguous range paints a *vertical band* and modems cluster. A
residential prefix is an even speckle with no pattern at all. You can tell which
you're looking at without reading a single number, which is what the authors
were proud of.

There are no audio files here. Every sound is generated from the frequency that
actually produced it, DTMF pairs per ITU-T Q.23, the Bell tone plan, a 2100 Hz
answer tone phase-reversed for V.32 echo cancellation, then carrier training.

Because they're generated rather than captured, a session's audio can be
reconstructed instead of taped off the speakers: `replay --record out.wav`
writes the exact soundtrack of a run, each sound placed at the moment it fired,
with no loopback device and no noise floor.

## How faithful is it?

Tested rather than asserted. Byte layouts and result codes are ported from the
1994 C source rather than inferred, citing file and line; see
[`docs/dat-format.md`](docs/dat-format.md).

The archival files are the oracle: all fourteen parse, read > serialize > read
is byte-identical, and every byte maps to a documented state. `SAMPLES.DOC` is a
second and better one, the authors annotating their own scans ("*a split
exchange, 0-4000 is residential, business from 4000-9999*"). Those are
falsifiable claims about real bytes, so they're tests, and they pass.

There is no `.DAT` writer, and won't be. This never conducts a real scan, so it
has nothing authoritative to save, and the archival files are primary sources no
code path should be able to open for writing.

## Status

Early, but it runs. The formats, the ToneMap, the screen, the replay engine and
the audio all work. Next: the `.CFG` model, `/M /R /X` mask options, and the
synthetic exchange.

```
crates/
  tl-core/    formats, cell states, masks, sequencing, analysis. No I/O.
  tl-modem/   ModemTransport, AT result codes, ReplayTransport.
  tl-audio/   procedural telephony synthesis. No audio files.
  tl-tui/     the three-window screen, ToneMap grid, TextMap.
  tl-cli/     the binary.
reference/    git-ignored clone of steeve/ToneLoc: the read-only oracle.
```

## Credit

**ToneLoc was created by Minor Threat and Mucho Maas**, who released it as
freeware in the early 1990s with a manual written in a voice this project has
tried not to sand down. The design being reconstructed here is entirely theirs.
If you haven't read `README.txt` from the original distribution, do.

*if you don't read the docs, you're a LAMER!*

Source referenced from the [`steeve/ToneLoc`](https://github.com/steeve/ToneLoc)
mirror. No original code is vendored here; the reconstruction is fresh Rust.

Reconstruction by BenDoubleU.

## Licence

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your
option. That covers *our* code only, the original was unlicensed freeware, and
its authors get credit above, not a licence we're in no position to grant.
