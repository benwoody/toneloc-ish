# toneloc-ish — Agent Brief

**Project name:** `toneloc-ish` (lowercase, hyphenated). The `-ish` is a deliberate wink — it keeps the original's rapper pun ("Tone Locator" → the rapper Tone Lōc) intact while signaling this is a work-alike, not the real binary. It reads as *homage that knows it's a homage*: behaves like ToneLoc, isn't ToneLoc.

**Mission:** Build a faithful, cross-platform Rust reconstruction of the 1990s MS-DOS wardialer *ToneLoc* (by Minor Threat & Mucho Maas) — as a **historical simulator and preservation piece**, not a working wardialer. The goal is to make a computing/phreaking practice that no longer exists runnable, legible, and accurate again: the scan engine, the TUI, the result-code logic, and above all ToneMap, driven by simulated telephony rather than a real modem.

This is a **port of the logic**, not a clean-room guess: the original C+asm source exists and is the reference oracle. Read it, translate the engine faithfully, discard the DOS hardware glue, and put a simulation where the modem used to be.

## Why a simulator (project identity)

As a *working* wardialer this is a dead use case — POTS lines are largely gone, VoIP mangles carrier negotiation, and scanning ranges you don't own is illegal-to-antisocial regardless. That's not what this is. This is preservation: a faithful, hardware-free, immediately-runnable reconstruction of what ToneLoc did and what scanning a telephone prefix revealed in ~1993. Think DOSBox-for-a-practice, or a flight simulator for a retired aircraft. Everything interesting about the original — the engine, the three-window UI, and the emergent ToneMap patterns the authors were genuinely proud of — needs zero telephony. The frustrating, pointless, legally-murky parts fall away and every good engineering problem stays.

Consequences of taking preservation seriously (these drive the design):
- **The sample `.DAT` files are primary sources, not fixtures.** The twelve `SAMPLE*.DAT` + `562XXXX.DAT` in the repo are real recorded scans. Treat them as archival: preserve and surface their provenance (dates in `TONE.LOG` go back to 31-Jan-93), never mutate them, and make them *playable*.
- **Fidelity to original behavior is a requirement, not a nice-to-have.** Messages, result codes, `.DAT`/`.CFG` byte layouts, mask math, and ToneMap colors match the original. Diff against the real thing in DOSBox.
- **Preserve context, not just code.** The manual's voice and the world it describes (what a "Tone" vs "Carrier" meant, blacklists "for areas with Caller ID and ex-girlfriends," why loops mattered) are part of the artifact. Capture them in docs and, as a stretch, an in-app "read this map" annotation layer.

## Prior art & positioning

No Rust port or simulator of ToneLoc exists, in any language — this is unclaimed ground. The only ToneLoc code in the wild is the original C (`steeve/ToneLoc`). The closest descendant is **WarVOX** (Ruby, VoIP-based, unmaintained) — a spiritual successor, not a port. The **ToneMap** visualization was later borrowed by runZero's subnet-grid report, but the program itself was never reimplemented. The name `toneloc-ish` was clear on both crates.io and GitHub at project start. Publish under `toneloc-ish` on both the repo and crates.io — one name across the board. Position the project accordingly in the README: the first faithful, hardware-free reconstruction, built for preservation rather than scanning.

## Reference material

- **Source (unofficial mirror):** `https://github.com/steeve/ToneLoc` — C/asm source, sample `.DAT` files, and the full v0.98 user manual (in `README.txt`). The manual is the behavioral spec; when in doubt, it wins.
- **v1.10 source zip:** `https://web.archive.org/web/20221101100011/https://www.oldskoolphreak.com/etc/TL110SRC.ZIP` (later revision — cross-check).
- Original toolchain: Borland C++ 3.1 + Turbo Assembler; windowing via CXL v5.2; serial routines adapted from Mark Goodwin, *Serial Communications in C and C++*.

**First action:** clone the mirror into `reference/` (git-ignored, read-only oracle). Running the shipped `TONELOC.EXE` + `TONEMAP.EXE` in DOSBox against the sample `.DAT` files is enough of an oracle to start; a full Turbo C build is nice but optional.

## Goal & non-goals

**In scope (v1):**
- The scan engine: mask/range/exclude parsing, randomized-vs-sequential sequencing, blacklist, time windows, autosave.
- Binary-compatible `.DAT` and `.CFG` formats (read *and* write original files).
- The AT dialing protocol + result-code interpretation — as the thing the *simulation* answers, and (optionally, later) real hardware.
- A `ratatui` TUI reproducing the three-window layout (Activity Log / Modem window / Stats + meter) and the ToneMap grid.
- **Two simulated transports as the primary product** (see below): replay and synthetic exchange.

**Optional "hard mode" (post-v1, design for but don't build):**
- `SerialTransport` (real modem) and `SipTransport` — a physical/VoIP backend for anyone who wants the modem to actually chirp. Keep the transport trait honest so these slot in, but they are dessert, not the meal.

**Out of scope (v1):**
- The VGA graphics path (`TLVGA.ASM`, Fastgraph). Reimplement ToneMap as a TUI grid; keep text-mode `TEXTMAP.C` as the behavioral reference.
- The `.DAT` merge/mirror utilities (`MERGE.C`, `TMERGE.C`, `MIRROR.C`, …) — port later as small subcommands once the format is nailed.

## The two transports (the heart of the sim)

Both sit behind one `ModemTransport` trait; the engine can't tell which is underneath.

**1. Replay (`ReplayTransport`) — the centerpiece.** Load a real `.DAT` file and re-run the scan in real time: "dial" each number and reveal the result that was *actually recorded* in the 90s, watching the grid fill in and tones/carriers surface as they did. Zero fabrication — you're replaying someone's real scan as living history. This is what makes the archival sample files playable. Support pacing controls (real-time / fast / step) and show the run's provenance.

**2. Synthetic exchange (`SimulatedExchange`) — the sandbox.** Procedurally model a period-accurate telephone prefix and let the engine scan it fresh, producing patterns you can actually read on the ToneMap. Historical accuracy is the whole point here — see below.

## Historical accuracy of the synthetic exchange (research task)

The synthetic exchange must produce the *kinds* of patterns real scanners saw, or it's just colored noise. This is a research problem — derive the model from the manual's described phenomenology plus period sources; do **not** hardcode guessed allocation rules. Things to get right, verified against sources rather than assumed:
- NANP structure (NPA-NXX-XXXX); within a 10k-block prefix, residential areas trend toward fuller, more even assignment while business exchanges show structure.
- Business exchanges: contiguous **DID ranges** routed to a PBX (show up as bands of ringout/timeout), clusters of modems (carriers), the occasional PBX indial or loop.
- Period specials: `950-XXXX` carrier-access dial-ups, loops often parked at the high end of a prefix (manual's 836-9998/9999), permanently-busy columns, pager/answering-service ranges.
- The manual explicitly contrasts residential ("even distribution, no pattern") vs business ("strings or clusters of modems") — that contrast is the success criterion. A good synthetic map should be visually distinguishable as one or the other.

Make the exchange model data-driven (a scenario file) so multiple historically-flavored exchanges can ship as presets.

## Source map: port vs. replace

**Port faithfully (the soul):**
- `TONELOC.C`, `TONELOC.H`, `TLOC85.C` — main dial loop, sequencing, keypresses, stats.
- `TLCFG.C`, `TLCFG.H`, `TL.H`, `TL.CFG` — config model + file format.
- `TLOG.C` — logging + exact log-line formats (manual enumerates every message).
- `GETOPT.C/.H` — CLI parsing → replace impl with `clap`, but preserve exact option semantics (`/M /R /X /D /C /S /E /H /T /K`).
- `TCONVERT.C` — old→new `.DAT` upgrade logic; documents the format's evolution.

**Replace wholesale (DOS hardware/UI glue):**
- `SERIAL.C/.H`, `SERCPP.H`, `SERASM.ASM` — direct 16550 UART + IRQ → the transport layer.
- `FOSSIL.C/.H/.ASM`, `FOSASM.ASM`, `FOS.H` — FOSSIL support. (Note: a FOSSIL driver was DOS's "let a driver own the port" abstraction — conceptually the same job our `ModemTransport` trait does now.)
- `CXL*.H`, `CXL-TL.LIB`, `CXLTCS.LIB` — CXL text windows → `ratatui` + `crossterm`.
- `TLVGA.ASM`, `FASTGRAF.H`, `FGS.LIB`, `TONEMAP.C/.H` — VGA ToneMap → TUI grid widget.

**Reference (behavior only):**
- `README.txt` — the manual / source of truth.
- `TEXTMAP.C` — text-mode ToneMap; model for our grid renderer.
- `SAMPLE1.DAT`…`SAMPLE12.DAT`, `562XXXX.DAT` — archival primary sources + golden tests.

## Target architecture

```
toneloc-rs/
├── crates/
│   ├── tl-core/      # pure, no I/O: masks, ranges, exclude sets, .DAT & .CFG
│   │                 # models, scan sequencing, result-code + cell-state enums.
│   │                 # Fully unit-testable. No transport, no TUI, no async.
│   ├── tl-modem/     # ModemTransport trait + AT protocol layer.
│   │                 # impls (v1):  ReplayTransport, SimulatedExchange
│   │                 # impls (later): SerialTransport, SipTransport
│   │                 # AT dialer: build "ATDT <n> W;", parse result codes.
│   ├── tl-tui/       # ratatui UI: activity log, modem window, stats, meter,
│   │                 # ToneMap grid + .DAT viewer + replay controls.
│   └── tl-cli/       # clap entrypoint; wires config + transport + engine + TUI.
└── reference/        # git-ignored clone of steeve/ToneLoc (read-only oracle).
```

Design rules:
- **`tl-core` never imports an I/O crate.** Sequencing is pure functions over state — which is what lets the whole engine be built and tested before any transport exists.
- Model result codes and `.DAT` cell states as **enums**, with a lossless byte↔enum mapping so files stay binary-compatible.
- The dial/read loop is `async` (`tokio`) with the transport behind the trait; a scan is a state machine driven by transport events + a per-dial timeout. A simulated transport just resolves those events instantly (or on a chosen clock) instead of over a wire.
- Watch the original's **16-bit `int` assumptions** (the "never use 5 X's" rule is a 16-bit overflow at 100000). Use `u32`/`usize` and the limit vanishes, but preserve/validate the documented mask behavior so results match.

## Data formats to reverse-engineer first

Nail these before any UI, sim, or hardware work — everything depends on them, and we have golden fixtures.

**`.DAT` file — exactly 10016 bytes** (per manual). Working hypothesis: small fixed header (~16 bytes: version, mask/range metadata, counters) + 10000 one-byte cells, one per number `0000`–`9999`, column-major (each column = 100 numbers; top-left `0000`, bottom-right `9999`). **Verify against source — don't trust the hypothesis.** Derive exact layout from `TCONVERT.C` (converts old→new, so it spells out both) and the read/write code in `TONELOC.C`.

**Cell states / result codes** (canonical enum, from manual + ToneMap legend):

| State | ToneMap color | Meaning |
|---|---|---|
| Undialed | Black | not yet dialed |
| Timeout | Grey (lighter = more rings) | dialed, nothing before WaitDelay |
| Busy | Orange/Red | busy signal |
| Blacklisted | Dark blue | in blacklist, skipped |
| RingOut | Dark green | MaxRings reached |
| Tone | Light green | **found a tone** (PBX/loop/LD carrier) |
| Carrier | Light yellow | **found a modem carrier** |
| Noted | Cyan | operator pressed `N` |
| Aborted | Dark red | operator pressed space |
| (also) No Dialtone, Voice | — | see manual for handling |

**`.CFG` file:** COM port, baud, colors, dial/init string, WaitDelay, MaxRings, NoToneAbort, autosave interval, ToneResponse, log filename, etc. Derive struct from `TLCFG.C/.H`; keep `TL.CFG` as a parse fixture. Read the legacy binary `.CFG` for authenticity, and also offer a modern human-readable (TOML) config mapping onto the same model.

**Blacklist file:** up to 1000 numbers, one per line, exact-match, `;` starts a comment.

**AT dial protocol** (port precisely; it's what the sim answers): tone scan dials `ATDT <number> W;` — `W` waits for dialtone, `;` returns to command line; `OK` means a tone was heard. Carrier scan looks for `CONNECT`; `NO CARRIER`/`BUSY`/`VOICE`/`RINGING` classify per config. PBX-hack masks embed nested `W` commands (e.g. `555-9999Wxxx`) — the mask engine passes these into the dial string verbatim.

## Testing & fidelity

Tests here are the proof of faithfulness, not just correctness — the original binary and its data files are the oracle, and a green suite is what backs the claim "this really is ToneLoc." Build the suite alongside the code, not after. `tl-core` tests stay I/O-free so they run instantly and pin engine behavior independent of transport or UI.

- **Golden `.DAT` round-trip (the load-bearing test).** For all 12 sample files: read → write → read must be byte-identical. Property-test (`proptest`) arbitrary in-memory grids through the same round-trip. Everything else assumes the format is exact, so this comes first.
- **Oracle diff against DOSBox.** Render each sample's ToneMap and compare structurally against `TONEMAP`/`TEXTMAP` output from the original in DOSBox; any divergence is a fidelity bug. The format is stable enough to treat as a fixed target (runZero even loaded non-phone data into the unmodified ToneMap).
- **The manual's worked examples become the test corpus.** The v0.98 manual gives exact command lines and their meaning — turn each into an assertion. E.g. `474-XXXX /R:9000-9999 /X:91XX` dials 9000–9999 except 9100–9199; a `555-1XXX` mask emits exactly 1000 distinct numbers with no repeats; blacklist `555-1212` matches only that literal, not `1-555-1212` or `5551212`. Mask/range/exclude/blacklist logic is fully unit-testable this way with zero I/O.
- **Transport tests via a mock / `VirtualModem`.** Script AT exchanges and assert result-code → `CellState` for every case (OK→Tone, CONNECT→Carrier, BUSY→Busy, NO CARRIER, VOICE, RINGING, silence→Timeout, No Dialtone→retry up to NoToneAbort). State-machine tests: per-dial timeout, ring counting, autosave interval, time-window start/stop.
- **Replay determinism.** Replaying a `.DAT` must reproduce exactly the results recorded in it — assert the played sequence equals the file's cells. It's an exact-equality test and a guard against sequencing bugs.
- **Synthetic-exchange characterization.** Procedural, so assert invariants rather than exact output: a "residential" preset yields an even distribution with no strong structure; a "business" preset yields detectable clusters/bands. Seed the RNG so scenarios reproduce.
- **TUI snapshots (optional).** `insta` snapshots of rendered frames to catch layout regressions.
- **CI.** GitHub Actions across Linux/macOS/Windows (cross-platform is a project goal): `fmt`, `clippy -D warnings`, `test`; gate merges on green.

Crates: `proptest` (round-trip/property), `insta` (snapshots), std `#[test]` for the rest.

## Milestones (in execution order)

0. **Bootstrap:** clone reference into `reference/`; scaffold workspace + crates; get the original running in DOSBox against a sample `.DAT` as the oracle.
1. **`.DAT` reader + ToneMap-in-terminal:** parse all 12 sample files; render the grid; visually diff against `TONEMAP`/`TEXTMAP` in DOSBox. Read-only, immediate payoff, validates the format.
2. **`.DAT` writer + round-trip tests:** write byte-identical files; property-test read→write→read.
3. **Config:** parse legacy `.CFG` + new TOML into one `Config` model.
4. **Mask/range/exclude engine:** parse `/M /R /X /D`; generate dial sequence (random + sequential); honor blacklist and mask math. Pure-unit-tested against the manual's examples.
5. **`ModemTransport` trait + `ReplayTransport`:** feed results from a real `.DAT`; wire the scan state machine + TUI so a historical scan replays live with pacing controls. **This is the first end-to-end "it lives" moment — the preservation centerpiece.**
6. **AT dialer + full scan state machine:** dial-string construction, result-code→cell-state mapping, per-dial timeout, autosave, time windows, live keypresses (S/N/R/P/J/space/Esc/0-9/X).
7. **`SimulatedExchange` transport + scenario files:** period-accurate synthetic exchanges (the research task above); ship a few presets (residential, business, mixed).
8. **Full TUI polish:** three-window layout + meter + live stats + ToneMap tab + provenance/annotation display.
9. **Optional hard mode:** `SerialTransport` against a lab PBX / owned line; then `SipTransport`. Init string + result mapping fully config-driven.
10. **Extras:** deferred `.DAT` utilities as subcommands; preservation docs; the manual's context surfaced in-app.

## Repo setup & first commit (do this before Milestone 0)

This is a **public GitHub project**. Get the identity and hygiene right at `git init` time so nothing has to be retrofitted after people start using it.

- **Layout:** initialize as a Cargo **workspace** from the first commit, not a flat single crate — the architecture above depends on the `tl-core` / `tl-modem` / `tl-tui` / `tl-cli` split, and starting flat means moving files and rewriting import paths later for nothing. Hand-write the workspace `Cargo.toml` (or `cargo new` each member crate) so the first commit already has the structure. Publish under the single name `toneloc-ish` (repo and crate).
- **Portable by default — no OS-specific assumptions.** This is a public project for others, not a personal setup. Write plain, portable Rust; primary targets are **Linux and macOS** (and anywhere else Rust + the deps run — Windows where it comes for free). Do not add Nix/NixOS-specific tooling, flakes, or environment assumptions to the repo; a standard `cargo build` on a stock toolchain must be all anyone needs. Any contributor's personal dev environment stays out of the tree.
- **`.gitignore` from commit one:** at minimum `/target` and `/reference/`. The `reference/` clone of `steeve/ToneLoc` lives on disk as the oracle but must **never** enter history — keeps the repo clean and avoids tangling the original's provenance into ours.
- **Do not vendor the original C source.** Reference it, link it, keep it in the gitignored clone. Our tree contains only fresh Rust.
- **License (finalize before first push — annoying to change later):** the original ToneLoc was unlicensed freeware, so we're not bound by it; the Rust code is ours to license. Recommended: the Rust-ecosystem default of **dual MIT / Apache-2.0**. The license covers *our* code only; credit for the original design goes in the README, not by adopting a license we don't have. *(Operator to confirm the final choice.)*
- **README first screen** is where the whole identity lands in five seconds. It should carry, in order: the name and its wink; a one-liner — *"a historical simulator of the ToneLoc wardialer — relive the 1980s/90s wardialing era, no phone line required"*; optional flavor line *"Loc-ed After Dark"* (nod to Tone Lōc's debut album + the fact that real scans ran overnight); and an up-front credit to **Minor Threat & Mucho Maas** for the original ToneLoc and the design being reconstructed. Writing in the manual's own self-aware register ("if you don't read the docs, you're a LAMER!") fits the tone. *(Fill the GitHub credit/handle line at setup — operator to supply.)*

## Your first task

Do **repo setup, then Milestones 0 and 1**, and stop for review:

1. `git init` a public repo named `toneloc-ish`; add `.gitignore` (`/target`, `/reference/`); lay out the Cargo workspace with the four member crates; add the chosen license file(s) and a README with the first-screen identity above. Make this the clean first commit.
2. Clone `steeve/ToneLoc` into the (gitignored) `reference/`.
3. Read `TONELOC.C`, `TCONVERT.C`, and `TL.H` closely; write `reference/NOTES-dat-format.md` documenting the *actual* `.DAT` header + cell layout from the source (correct the hypothesis above where wrong — cite file + line).
4. In `tl-core`, implement the `.DAT` **reader** + the `CellState` enum with a lossless byte mapping, plus a `tl-cli` subcommand `tonemap <file.dat>` that renders the grid using the legend above.
5. Load all 12 sample files as fixtures; assert they parse and every byte maps to a known state.

Report back with the format notes and a rendered sample map before proceeding to the writer.

## Open questions to resolve from the source (not from guesswork)

- Exact `.DAT` header layout and the meaning of all ~16 header bytes.
- Cell-state byte values, including ones the ToneMap legend omits (No Dialtone, Voice, Fax/Girl/VMB/custom notes).
- Column-major vs row-major cell ordering (manual implies column-major; confirm).
- How ring count is encoded ("Timeout (3)", "lighter grey = more rings" — packed into the timeout byte?).
- `.CFG` binary layout and default values.
- Provenance metadata available in the sample `.DAT`/`.LOG` files (dates, masks, original operator notes) worth surfacing in replay mode.
