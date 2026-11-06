//! The only randomness of the engine (003 RF6): ChaCha20 seeded with the [`Seed`], consumed
//! only through [`Rolls::roll`] with rejection sampling. `rand`'s distributions are not used
//! because their algorithms may change between versions.
use std::num::NonZeroU32;

use rand_chacha::ChaCha20Rng;
use rand_chacha::rand_core::{Rng, SeedableRng};

use crate::Seed;

/// A source of uniform rolls in `0..n`. The simulation is generic over it so the tests can
/// record or script the rolls (RF7).
pub(crate) trait Rolls {
    fn roll(&mut self, n: NonZeroU32) -> u32;
}

/// Rejection sampling over a 32-bit generator.
pub(crate) struct Roller<R> {
    rng: R,
}

impl Roller<ChaCha20Rng> {
    pub(crate) fn from_seed(seed: &Seed) -> Self {
        Self {
            rng: ChaCha20Rng::from_seed(seed.bytes()),
        }
    }
}

impl<R: Rng> Rolls for Roller<R> {
    /// Uniform in `0..n`: `zone = (2^32 / n) * n` computed in `u64`; `x >= zone` is discarded
    /// and `x mod n` returned.
    fn roll(&mut self, n: NonZeroU32) -> u32 {
        let n64 = u64::from(n.get());
        let zone = ((1_u64 << 32) / n64) * n64;
        loop {
            let x = self.rng.next_u32();
            if u64::from(x) < zone {
                return x % n;
            }
        }
    }
}

/// `n` as a roll bound; 0 becomes 1 (the `Ruleset` bounds test keeps every bound nonzero).
pub(crate) const fn bound(n: u32) -> NonZeroU32 {
    match NonZeroU32::new(n) {
        Some(n) => n,
        None => NonZeroU32::MIN,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use rand_chacha::rand_core::{Infallible, TryRng};

    use super::*;

    /// Returns the queued values in order (then zeros) and counts the draws.
    struct Fixed {
        values: VecDeque<u32>,
        draws: usize,
    }

    impl Fixed {
        fn new(values: &[u32]) -> Self {
            Self {
                values: values.iter().copied().collect(),
                draws: 0,
            }
        }
    }

    /// `Rng` comes from `TryRng` with an infallible error (rand_core 0.10).
    impl TryRng for Fixed {
        type Error = Infallible;
        fn try_next_u32(&mut self) -> Result<u32, Infallible> {
            self.draws += 1;
            Ok(self.values.pop_front().unwrap_or(0))
        }
        fn try_next_u64(&mut self) -> Result<u64, Infallible> {
            self.try_next_u32().map(u64::from)
        }
        fn try_fill_bytes(&mut self, dst: &mut [u8]) -> Result<(), Infallible> {
            dst.fill(0);
            Ok(())
        }
    }

    fn roller(values: &[u32]) -> Roller<Fixed> {
        Roller {
            rng: Fixed::new(values),
        }
    }

    #[test]
    fn n_one_returns_zero_without_rejecting() {
        let mut r = roller(&[u32::MAX, 7]);
        assert_eq!(r.roll(bound(1)), 0);
        assert_eq!(r.rng.draws, 1);
    }

    #[test]
    fn values_below_the_zone_are_reduced_mod_n() {
        let mut r = roller(&[10, 4_294_967_294]);
        assert_eq!(r.roll(bound(3)), 1);
        // zone for n = 3 is 4_294_967_295, so u32::MAX - 1 is still accepted.
        assert_eq!(r.roll(bound(3)), 4_294_967_294 % 3);
        assert_eq!(r.rng.draws, 2);
    }

    #[test]
    fn values_at_or_above_the_zone_are_rejected() {
        // n = 3: zone = 4_294_967_295, so only u32::MAX is rejected.
        let mut r = roller(&[u32::MAX, 5]);
        assert_eq!(r.roll(bound(3)), 2);
        assert_eq!(r.rng.draws, 2);
        // n = 2^31 + 1: zone = n, so the whole upper half is rejected.
        let n = (1_u32 << 31) + 1;
        let mut r = roller(&[n, u32::MAX, n - 1]);
        assert_eq!(r.roll(bound(n)), n - 1);
        assert_eq!(r.rng.draws, 3);
    }

    #[test]
    fn bound_never_returns_zero() {
        assert_eq!(bound(0).get(), 1);
        assert_eq!(bound(1000).get(), 1000);
    }

    /// Pins the ChaCha20 stream itself: the first words for the zero seed are the ChaCha20
    /// keystream of RFC 8439 with a zero key and nonce. A dependency bump that changes them would
    /// change every fight.
    #[test]
    fn the_chacha_stream_is_pinned() {
        let mut rng = ChaCha20Rng::from_seed([0; 32]);
        let words: Vec<u32> = (0..4).map(|_| rng.next_u32()).collect();
        assert_eq!(words, [0xade0_b876, 0x903d_f1a0, 0xe56a_5d40, 0x28bd_8653]);
    }

    #[test]
    fn the_same_seed_gives_the_same_rolls() {
        let roll_all = |byte| {
            let mut r = Roller::from_seed(&Seed::from_bytes([byte; 32]));
            (0..64).map(|_| r.roll(bound(1000))).collect::<Vec<_>>()
        };
        assert_eq!(roll_all(1), roll_all(1));
        assert_ne!(roll_all(1), roll_all(2));
    }
}
