//! What order to dial in.
//!
//! ToneLoc's default was **random**, not sequential (`cfg.DialMethod = 0`,
//! `TLCFG.C:1329`). The manual annotates its own examples that way — *"Dial
//! 1000 numbers, from 950-5000 to 950-5999 (randomly)"* — and says why:
//!
//! > Make sure you hack RANDOMLY - sequential hacking is always a good way to
//! > get noticed
//!
//! A sequential sweep through a prefix is an obvious pattern at the switch.
//! Scattered calls over fifty hours look like noise.
//!
//! ## The duplicate check
//!
//! Random dialing needs a "have I done this one?" set, and ToneLoc doesn't
//! have one — because it doesn't need one. The dial loop is
//! `do { ...pick... } while (checkdupe(xstring));` and `checkdupe` looks at the
//! `.DAT` cell: nonzero means dialed, so reroll. **The data file is the visited
//! set.** That is why the format is a flat array indexed by the number itself
//! rather than a list of results — it has to answer "is 5551234 done?" in one
//! lookup on a 386.
//!
//! We produce a shuffled permutation instead of rerolling. It yields the same
//! thing — every number once, in an unpredictable order — without the
//! coupon-collector tail, where the original spent an average of a hundred
//! rerolls to place its last hundred numbers. The observable sequence is what
//! matters; the stall was a consequence, not a design.

use crate::dat::CELL_COUNT;
use crate::mask::Mask;

/// The `DialMethod` settings (`TONELOC.C:1047-1093`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DialOrder {
    /// `DialMethod = 0`, the default. Seeded so a replay reproduces exactly.
    Random { seed: u64 },
    /// `DialMethod = 1`.
    Forward,
    /// `DialMethod = -1`.
    Backward,
}

impl Default for DialOrder {
    fn default() -> Self {
        DialOrder::Random { seed: 0x10C }
    }
}

/// A planned dial sequence: every number a mask covers, in dial order.
#[derive(Clone, Debug)]
pub struct ScanSequence {
    numbers: Vec<u16>,
}

impl ScanSequence {
    /// Plan a scan of `mask`, skipping numbers `already_done` reports as
    /// finished — the same job `checkdupe` did against the `.DAT`.
    pub fn plan(mask: &Mask, order: DialOrder, already_done: impl Fn(u16) -> bool) -> ScanSequence {
        let mut numbers: Vec<u16> = (0..mask.count())
            .map(|v| v as u16)
            .filter(|n| !already_done(*n))
            .collect();

        match order {
            DialOrder::Forward => {}
            DialOrder::Backward => numbers.reverse(),
            DialOrder::Random { seed } => shuffle(&mut numbers, seed),
        }

        ScanSequence { numbers }
    }

    /// Plan a scan of a whole 10,000-number prefix.
    pub fn plan_all(order: DialOrder, already_done: impl Fn(u16) -> bool) -> ScanSequence {
        let mut numbers: Vec<u16> = (0..CELL_COUNT as u16)
            .filter(|n| !already_done(*n))
            .collect();
        match order {
            DialOrder::Forward => {}
            DialOrder::Backward => numbers.reverse(),
            DialOrder::Random { seed } => shuffle(&mut numbers, seed),
        }
        ScanSequence { numbers }
    }

    /// The numbers to dial, in order.
    pub fn numbers(&self) -> &[u16] {
        &self.numbers
    }

    pub fn len(&self) -> usize {
        self.numbers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.numbers.is_empty()
    }
}

/// Fisher-Yates with a seeded SplitMix64 — no dependency, and reproducible so
/// a replayed scan runs the same way twice.
fn shuffle(numbers: &mut [u16], seed: u64) {
    let mut state = seed;
    let mut next = || {
        state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    };
    for i in (1..numbers.len()).rev() {
        let j = (next() % (i as u64 + 1)) as usize;
        numbers.swap(i, j);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn mask(s: &str) -> Mask {
        Mask::parse(s).unwrap()
    }

    #[test]
    fn a_mask_emits_exactly_its_numbers_once_each() {
        // The manual: a 555-1XXX mask is 1000 distinct numbers, no repeats.
        let seq = ScanSequence::plan(&mask("555-1XXX"), DialOrder::default(), |_| false);
        assert_eq!(seq.len(), 1000);
        let unique: BTreeSet<u16> = seq.numbers().iter().copied().collect();
        assert_eq!(unique.len(), 1000);
        assert_eq!(*unique.first().unwrap(), 0);
        assert_eq!(*unique.last().unwrap(), 999);
    }

    #[test]
    fn the_default_is_random_not_sequential() {
        // TLCFG.C:1329 — cfg.DialMethod = 0.
        assert!(matches!(DialOrder::default(), DialOrder::Random { .. }));

        let seq = ScanSequence::plan_all(DialOrder::default(), |_| false);
        let n = seq.numbers();
        let in_order = n.windows(2).filter(|w| w[1] == w[0] + 1).count();
        assert!(
            in_order < 100,
            "{in_order} consecutive pairs looks sequential, not random"
        );
    }

    #[test]
    fn sequential_orders_run_both_ways() {
        let forward = ScanSequence::plan(&mask("555-1XX"), DialOrder::Forward, |_| false);
        assert_eq!(&forward.numbers()[..3], &[0, 1, 2]);

        let backward = ScanSequence::plan(&mask("555-1XX"), DialOrder::Backward, |_| false);
        assert_eq!(&backward.numbers()[..3], &[99, 98, 97]);
    }

    #[test]
    fn a_seed_makes_the_order_reproducible() {
        let a = ScanSequence::plan_all(DialOrder::Random { seed: 7 }, |_| false);
        let b = ScanSequence::plan_all(DialOrder::Random { seed: 7 }, |_| false);
        let c = ScanSequence::plan_all(DialOrder::Random { seed: 8 }, |_| false);
        assert_eq!(a.numbers(), b.numbers());
        assert_ne!(a.numbers(), c.numbers());
    }

    #[test]
    fn already_dialed_numbers_are_skipped() {
        // What checkdupe did: the .DAT is the visited set.
        let seq = ScanSequence::plan_all(DialOrder::Forward, |n| n < 9_000);
        assert_eq!(seq.len(), 1_000);
        assert_eq!(seq.numbers()[0], 9_000);
    }

    #[test]
    fn a_finished_scan_plans_nothing() {
        let seq = ScanSequence::plan_all(DialOrder::default(), |_| true);
        assert!(seq.is_empty());
    }

    #[test]
    fn shuffling_never_loses_or_duplicates_a_number() {
        for seed in 0..16u64 {
            let seq = ScanSequence::plan_all(DialOrder::Random { seed }, |_| false);
            assert_eq!(seq.len(), CELL_COUNT);
            let unique: BTreeSet<u16> = seq.numbers().iter().copied().collect();
            assert_eq!(unique.len(), CELL_COUNT, "seed {seed} lost numbers");
        }
    }

    #[test]
    fn random_order_still_covers_the_whole_prefix_evenly() {
        // Not clumped into one region: each tenth of the run should touch
        // roughly every tenth of the number space.
        let seq = ScanSequence::plan_all(DialOrder::Random { seed: 3 }, |_| false);
        let first_tenth = &seq.numbers()[..CELL_COUNT / 10];
        let high = first_tenth.iter().filter(|&&n| n >= 5_000).count();
        assert!(
            (400..600).contains(&high),
            "expected ~500 high numbers in the first tenth, got {high}"
        );
    }
}
