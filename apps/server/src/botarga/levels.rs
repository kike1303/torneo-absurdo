//! Levels and draws (021 "Niveles y sorteo", 016 "Sorteo de ataques"). Pure: the XP a fight gives
//! becomes levels here, and the options of a draw are taken from a pool with a random source the
//! caller passes (`OsRng` in the server, never the PRNG of the engine).

use engine::loadout::{DRAW_EVERY, LEVEL_MAX};

use super::xp_to_next;

/// Most options of a draw.
pub const DRAW_OPTIONS: usize = 3;

/// Where a botarga stands after earning some XP.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Gain {
    pub level: u8,
    /// The XP inside `level`: always under `xp_to_next(level)`, and 0 at the last level.
    pub xp: u32,
    /// The levels crossed that draw, in order.
    pub draw_levels: Vec<u8>,
}

/// `add` XP on a botarga of `level` with `xp` inside it. Every level crossed takes its
/// `xp_to_next`; at the last level the rest is dropped.
#[must_use]
pub fn gain(level: u8, xp: u32, add: u32) -> Gain {
    let mut level = level.clamp(1, LEVEL_MAX);
    let mut xp = xp.saturating_add(add);
    let mut draw_levels = Vec::new();
    while level < LEVEL_MAX {
        let need = xp_to_next(level);
        if xp < need {
            break;
        }
        xp -= need;
        level += 1;
        if level.is_multiple_of(DRAW_EVERY) {
            draw_levels.push(level);
        }
    }
    if level >= LEVEL_MAX {
        xp = 0;
    }
    Gain {
        level,
        xp,
        draw_levels,
    }
}

/// Up to [`DRAW_OPTIONS`] distinct ids of `pool`, each drawn with `uniform(n)` (a number in `0..n`)
/// among the ones left, in the order drawn. `pool` is everything the user may be offered; with
/// fewer ids than options, all of them. `None` when `uniform` fails (no entropy): a draw is never
/// faked.
pub fn pick(
    mut pool: Vec<String>,
    mut uniform: impl FnMut(u64) -> Option<u64>,
) -> Option<Vec<String>> {
    // Sorted and without repeats, so the same pool and the same draws give the same options.
    pool.sort_unstable();
    pool.dedup();
    let mut options = Vec::with_capacity(DRAW_OPTIONS);
    while options.len() < DRAW_OPTIONS && !pool.is_empty() {
        let n = u64::try_from(pool.len()).ok()?;
        let at = usize::try_from(uniform(n)?).ok()?;
        if at >= pool.len() {
            return None;
        }
        options.push(pool.swap_remove(at));
    }
    Some(options)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn xp_crosses_levels_and_keeps_the_rest() {
        // Level 1 needs 120, level 2 needs 140.
        assert_eq!(
            gain(1, 0, 100),
            Gain {
                level: 1,
                xp: 100,
                draw_levels: vec![]
            }
        );
        assert_eq!(gain(1, 100, 20).level, 2);
        assert_eq!(gain(1, 100, 20).xp, 0);
        assert_eq!(gain(1, 0, 120 + 140 + 5).level, 3);
        assert_eq!(gain(1, 0, 120 + 140 + 5).xp, 5);
        assert_eq!(gain(3, 0, 0), gain(3, 0, 0));
    }

    #[test]
    fn every_fifth_level_draws_once_and_in_order() {
        // From 4 with almost all of its XP to past 10.
        let to_ten: u32 = (4..10).map(xp_to_next).sum();
        let g = gain(4, 0, to_ten);
        assert_eq!(g.level, 10);
        assert_eq!(g.draw_levels, vec![5, 10]);
        assert!(gain(5, 0, 10).draw_levels.is_empty());
    }

    #[test]
    fn the_last_level_drops_the_xp() {
        let g = gain(99, 0, u32::MAX);
        assert_eq!(g.level, LEVEL_MAX);
        assert_eq!(g.xp, 0);
        assert_eq!(g.draw_levels, vec![100]);
        assert_eq!(gain(100, 0, 500).xp, 0);
        assert!(gain(100, 0, 500).draw_levels.is_empty());
        // From level 1 all the way: 20 draws, as 016 counts them.
        assert_eq!(gain(1, 0, u32::MAX).draw_levels.len(), 20);
    }

    fn ids(list: &[&str]) -> Vec<String> {
        list.iter().map(|id| (*id).to_owned()).collect()
    }

    #[test]
    fn pick_takes_three_distinct_or_all() {
        let first = |_| Some(0);
        assert_eq!(
            pick(ids(&["d", "c", "b", "a"]), first),
            Some(ids(&["a", "d", "c"]))
        );
        assert_eq!(pick(ids(&["b", "a"]), first).map(|o| o.len()), Some(2));
        assert_eq!(pick(ids(&["a", "a", "a"]), first), Some(ids(&["a"])));
        assert_eq!(pick(Vec::new(), first), Some(Vec::new()));
        assert_eq!(pick(ids(&["a"]), |_| None), None);
        // A source that answers out of range is not a draw.
        assert_eq!(pick(ids(&["a"]), Some), None);
        let mut drawn = pick(ids(&["e", "a", "c", "b", "d"]), crate::pvp::uniform).unwrap();
        assert_eq!(drawn.len(), 3);
        drawn.sort();
        drawn.dedup();
        assert_eq!(drawn.len(), 3);
    }
}
