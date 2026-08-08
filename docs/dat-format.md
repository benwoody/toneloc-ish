# The ToneLoc `.DAT` format

Derived from the original 1994 C source, not guessed at, and then checked
against all fourteen archival data files that ship with it. Every claim below
cites the file and line it came from, so it can be re-checked.

The brief this project started from carried a working hypothesis for the
format. It turned out to be right in every particular — 16-byte header, 10,000
one-byte cells, column-major — so this document is mostly confirmation, plus
the details the hypothesis did not reach: what the header bytes *mean*, the
full result-code table, and how ring counts are packed.

**Reference:** `steeve/ToneLoc` (cloned into the git-ignored `reference/`).

---

## File layout

Exactly **10016 bytes**, and the size is load-bearing: `TCONVERT.C:69-74`
identifies a file's era by its length alone.

| Offset | Size  | Field         | Notes                                    |
|-------:|------:|---------------|------------------------------------------|
| 0      | 2     | `ProductCode` | Always `"TL"`                            |
| 2      | 2     | `VersionID`   | `u16` little-endian; `0x0100` = v1.00    |
| 4      | 2     | `Minutes`     | `u16` little-endian; minutes spent scanning |
| 6      | 10    | `Extra`       | Reserved; zero in every file in the wild |
| 16     | 10000 | cells         | One byte per number `0000`–`9999`        |

Source: `struct _scan` in `TL.H:49-54` and `TONELOC.H:69-78`. The read and
write are plain `fread`/`fwrite` of the struct followed by the array
(`TONELOC.C:1820-1821`, `TONELOC.C:2144-2145`), so the on-disk layout is
Borland C's 16-bit little-endian struct packing with no padding — all fields
are naturally aligned at their `word` boundaries.

`VersionID` is read as hex-coded decimal: `0x0100` prints as `1.00`,
`0x0098` as `0.98`. ToneLoc 1.00 rejects any file whose `VersionID` is not
exactly `0x0100` and tells you to run `TCONVERT` (`TONELOC.C:1822-1830`).

`Minutes` accumulates across sessions (`TONELOC.C:2138`) and is displayed as
`hours:minutes` via `Minutes/60` and `Minutes%60` (`TONELOC.C:969`).

### The ten reserved bytes

`TL.H:37-39` explains them, and is worth quoting because it is the authors
inviting contributions to a format they were still designing:

> Also, there are about 10 bytes of extra space in the header, so let us know
> if there is something else you think should be kept there.

`TONELOC.H:75-77` shows what they had in mind — `startdate`, `lastdate`, and
the most recent mask — as a comment that was never implemented. **No shipped
version ever wrote anything but zeros here**, confirmed across all fourteen
sample files.

This matters for the project's provenance goal: dates and masks are *not* in
the `.DAT`. Scan provenance lives only in `TONE.LOG`, which is where the
31-Jan-93 dates come from.

### Earlier layouts

Same 10,000 cells, different header (`TONELOC.H:48-63`):

| Version | Size  | Header                                              |
|---------|------:|-----------------------------------------------------|
| 0.90    | 10010 | 5 × `int`: tones, rings, busys, voices, tried       |
| 0.95    | 10012 | 6 × `int`: tones, carriers, rings, busys, voices, tried |
| 0.98+   | 10016 | `struct _scan` above                                |

0.90 files carry no per-number detail worth keeping: `TCONVERT.C:79-81`
flattens every nonzero cell to `40`. From 0.98 on, size alone no longer
distinguishes versions — you must read `VersionID` (`TONELOC.H:65-67`).

---

## Cell ordering: column-major

Column `x` holds the hundred numbers `x00`–`x99`, running top to bottom.
`0000` is top-left, `9999` is bottom-right, and the grid is 100 × 100.

From the ToneMap plotting loop (`TONEMAP.C:131-137`):

```c
for (i=0; i < 10000; i++) {
   x = (i / 100);
   y = (i - (x * 100));
   x *= 2;  y *= 2;            /* 2 VGA pixels per cell */
   col = whatcolor(oldones[i]);
```

and its inverse, used for the mouse readout (`TONEMAP.C:683-691`):

```c
int whatnum(int x,int y) { return ((x/2) * 100) + (y/2); }
```

This is why the maps read the way they do: a contiguous DID range routed to a
PBX occupies consecutive numbers, which land in a **vertical band** on screen.

---

## Result codes

The byte is `class * 10 + rings`, with the ring count clamped to 9 by
`chopten()` (`TONELOC.C:2102-2107`). Documented in `TL.H:12-33` and again,
with one extra entry, in `TONELOC.C:12-37`.

| Byte  | State       | Written at             |
|------:|-------------|------------------------|
| `00`  | Undialed    | initial `memset`       |
| `1x`  | Busy        | `TONELOC.C:496`        |
| `2x`  | Voice       | `TONELOC.C:489`        |
| `3x`  | No Dialtone | `TONELOC.C:528`        |
| `40`  | Noted       | `TONELOC.C:621`, `639` |
| `41`  | Fax         | `TONELOC.C:524`, `628` |
| `42`  | Girl        | `TONELOC.C:631`        |
| `43`  | VMB         | `TONELOC.C:645`        |
| `44`  | Yelling Asshole | `TONELOC.C:648`    |
| `49`  | Person that sounds like Mucho | `TONELOC.C:651` |
| `5x`  | Aborted     | `TONELOC.C:662`        |
| `6x`  | Ringout     | `TONELOC.C:505`        |
| `7x`  | Timeout     | `TONELOC.C:533`, `680` |
| `8x`  | **Tone**    | `TONELOC.C:482`, `642` |
| `9x`  | **Carrier** | `TONELOC.C:520`, `624` |
| `100` | Excluded    | `TONELOC.C:1144`       |
| `110` | Omitted     |                        |
| `120` | Dialed      | generic; what 0.90 files convert to |
| `130` | Blacklisted | `TONELOC.C:453`        |

Note `49` — it appears in `TONELOC.C:12-37` but **not** in the `TL.H` table
that was published for third-party tool authors. `TL.H` is the older document.

### Answers to the open questions

**How is the ring count encoded?** In the low digit, for every class that has
one. `TEXTMAP.C:74` and `TONEMAP.C:whatcolor` both normalize with
`(v / 10) * 10` before switching, so `73` and `70` are the same state at
different ring counts. The VGA map used it directly for shading:
`pixcolor = oldval - 48` (`TONEMAP.C:642`) walked bytes 70–79 up a grey ramp,
which is the "lighter grey = more rings" the manual describes.

**Is the low digit always rings?** No — and this is the one place the
"`class * 10 + rings`" rule breaks. For class `40` the low digit is a *note
kind*, not a ring count; `TONEMAP.C:632-639` switches on the exact byte rather
than the rounded one. `41` (Fax) is likewise written with no ring count at all
(`TONELOC.C:524`).

**Cell-state bytes the ToneMap legend omits?** Four: No Dialtone (`3x`),
Excluded (`100`), Omitted (`110`) and Dialed (`120`). `TEXTMAP.C` has no arm
for any of them and prints `?`. Only three No-Dialtone bytes exist across all
fourteen archival files, and no Excluded, Omitted or Dialed bytes at all.

**Which bytes count as "tried"?** Everything except `0` and `100`
(`TCONVERT.C:126-134`, `TONELOC.C:1898`) — excluded numbers were never dialed,
so they do not count against the scan.

---

## Result-code mapping, and one surprise

The dial loop maps modem responses to cells (`TONELOC.C:470-540`):

| Modem said   | Recorded as        |
|--------------|--------------------|
| `OK`         | Tone `80+rings`    |
| `CONNECT`    | Carrier `90+rings` |
| `BUSY`       | Busy `10+rings`    |
| `VOICE`      | Voice `20+rings`   |
| `NO DIAL`    | No Dialtone `30+rings`, and retry |
| `NO CARRIER` | **Timeout `70+rings`** |
| `FAX`        | Note `41`          |
| `RINGING`    | nothing yet — increments the ring counter; becomes Ringout `60+rings` only on reaching MaxRings |

`NO CARRIER` recording as a **Timeout** is the non-obvious one. It is not a
bug: nothing answered, so the file records that nothing happened rather than
inventing a distinct state for it.

Responses are classified by `strstr` in a fixed priority order against
user-editable strings (`check_response()` in `TONELOC.C`; defaults at
`TLCFG.C:1291-1298`). The order is part of the behaviour — `OK` is tested
first, so a modem emitting `OK CONNECT` is recorded as a tone.

---

## Verification

`cargo test -p tl-core --test golden` reads every `.DAT` in `reference/` and
checks parse → serialize is byte-identical, and that every cell byte maps to a
documented state.

Result across all fourteen files: **every byte accounted for, no unknowns.**

```
file             size  ver     minutes
562XXXX.DAT     10016  0.99          0
SAMPLE1.DAT     10016  1.00       3363
SAMPLE2.DAT     10016  1.00       3037
SAMPLE3.DAT     10016  1.00          0
SAMPLE4.DAT     10016  1.00        887
SAMPLE5.DAT     10016  1.00       3411
SAMPLE6.DAT     10016  1.00       3407
SAMPLE7.DAT     10016  1.00       3253
SAMPLE8A.DAT    10016  1.00          0
SAMPLE8B.DAT    10016  1.00          0
SAMPLE9.DAT     10016  1.00       2861
SAMPLE10.DAT    10016  1.00       3028
SAMPLE11.DAT    10016  1.00       2776
SAMPLE12.DAT    10016  1.00       3229
```

Two findings worth recording:

- **`562XXXX.DAT` is a 0.99 file** (`VersionID` `0x0099`), a version between
  the two `TCONVERT` knows about. ToneLoc 1.00 would refuse to open it. We
  accept any `"TL"` file and report the version instead, so the oldest artifact
  in the set stays readable — `DatHeader::is_current()` exposes the original's
  stricter rule for anyone who wants it.
- **Distinct byte values observed: 28.** No file contains an Excluded,
  Omitted, Dialed, Blacklisted, or Aborted-past-2-rings byte, and the highest
  ring count recorded anywhere is 4. The format's range is far wider than what
  real scans exercised, which is worth knowing before treating the archival
  files as complete coverage for a round-trip test — hence the synthetic
  all-256-bytes case alongside them.
