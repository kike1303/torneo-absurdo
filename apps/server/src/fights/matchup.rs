//! The choice of each fight's pair (RF20, RF21). Pure: the runner reads the context from the
//! database and draws `roll` from `OsRng`, never from the PRNG of the engine nor from a seed, and
//! the pair is fixed before the seed of its fight exists.

use sha2::{Digest, Sha256};

/// An ordered pair of a round.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Matchup<'a> {
    pub a: &'a str,
    pub b: &'a str,
    /// No pair was left in the round, so this one starts a new one.
    pub new_round: bool,
}

fn same_cross(x: (&str, &str), y: (&str, &str)) -> bool {
    (x.0 == y.0 && x.1 == y.1) || (x.0 == y.1 && x.1 == y.0)
}

fn shares_character(x: (&str, &str), y: (&str, &str)) -> bool {
    x.0 == y.0 || x.0 == y.1 || x.1 == y.0 || x.1 == y.1
}

/// Chooses a pair. A round uses each ordered pair of `roster` once, without mirrors. Over the
/// candidates, in order: pairs whose unordered cross did not appear in the last 4 fights and, of
/// those, pairs that share no character with the previous fight. A preference that would leave no
/// candidate is ignored. `recent` has the most recent fight last. `None` with fewer than 2
/// characters.
pub fn pick_matchup<'a>(
    roster: &'a [String],
    used: &[(String, String)],
    recent: &[(String, String)],
    roll: u32,
) -> Option<Matchup<'a>> {
    let all: Vec<(&'a str, &'a str)> = roster
        .iter()
        .flat_map(|a| {
            roster
                .iter()
                .filter(move |b| *b != a)
                .map(move |b| (a.as_str(), b.as_str()))
        })
        .collect();
    let unused: Vec<(&'a str, &'a str)> = all
        .iter()
        .copied()
        .filter(|pair| !used.iter().any(|u| (u.0.as_str(), u.1.as_str()) == *pair))
        .collect();
    let new_round = unused.is_empty();
    let candidates = if new_round { all } else { unused };

    let fresh: Vec<(&'a str, &'a str)> = candidates
        .iter()
        .copied()
        .filter(|pair| {
            !recent
                .iter()
                .any(|r| same_cross(*pair, (r.0.as_str(), r.1.as_str())))
        })
        .collect();
    let candidates = if fresh.is_empty() { candidates } else { fresh };

    let candidates = match recent.last() {
        Some(last) => {
            let apart: Vec<(&'a str, &'a str)> = candidates
                .iter()
                .copied()
                .filter(|pair| !shares_character(*pair, (last.0.as_str(), last.1.as_str())))
                .collect();
            if apart.is_empty() { candidates } else { apart }
        }
        None => candidates,
    };

    let index = usize::try_from(roll).ok()? % candidates.len().max(1);
    let (a, b) = *candidates.get(index)?;
    Some(Matchup { a, b, new_round })
}

/// `SHA-256` of the sorted slugs, each followed by a newline, which no slug contains. If it changes,
/// the next fight opens a new round (RF20).
pub fn roster_sha256(sorted_slugs: &[String]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    for slug in sorted_slugs {
        hasher.update(slug.as_bytes());
        hasher.update(b"\n");
    }
    hasher.finalize().into()
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use proptest::prelude::*;

    use super::*;

    fn roster(n: usize) -> Vec<String> {
        (0..n).map(|i| format!("c{i}")).collect()
    }

    #[derive(Default)]
    struct Season {
        used: Vec<(String, String)>,
        recent: Vec<(String, String)>,
    }

    impl Season {
        /// The step of the runner, with what `store::matchup_context` would read.
        fn pick(&mut self, roster: &[String], roll: u32) -> (String, String) {
            let pick = pick_matchup(roster, &self.used, &self.recent, roll).unwrap();
            if pick.new_round {
                self.used.clear();
            }
            (pick.a.to_owned(), pick.b.to_owned())
        }

        fn fought(&mut self, pair: (String, String)) {
            self.used.push(pair.clone());
            self.recent.push(pair);
            if self.recent.len() > 4 {
                self.recent.remove(0);
            }
        }
    }

    fn cross(pair: &(String, String)) -> (String, String) {
        if pair.0 < pair.1 {
            pair.clone()
        } else {
            (pair.1.clone(), pair.0.clone())
        }
    }

    proptest! {
        /// In 1200 fights that were not cancelled, each ordered pair exactly 100 times, with any
        /// rolls and with cancelled fights in between, which neither use their pair nor count as
        /// recent.
        #[test]
        fn every_ordered_pair_comes_exactly_the_same_times(
            rolls in prop::collection::vec(any::<u32>(), 1200),
            cancels in prop::collection::vec(any::<bool>(), 1200),
        ) {
            let roster = roster(4);
            let mut season = Season::default();
            let mut count: BTreeMap<(String, String), u32> = BTreeMap::new();
            for (roll, cancel) in rolls.iter().zip(&cancels) {
                if *cancel {
                    // Picked, announced and cancelled: it leaves no trace, except that a new round
                    // it opened is opened again by the next one.
                    let mut ghost = Season {
                        used: season.used.clone(),
                        recent: season.recent.clone(),
                    };
                    let _ = ghost.pick(&roster, roll.wrapping_mul(31));
                }
                let pair = season.pick(&roster, *roll);
                *count.entry(pair.clone()).or_default() += 1;
                season.fought(pair);
            }
            prop_assert_eq!(count.len(), 12);
            for (pair, times) in count {
                prop_assert_eq!(times, 100, "{:?}", pair);
            }
        }

        /// No mirrors, with any roster size and any rolls.
        #[test]
        fn no_character_fights_itself(
            size in 2_usize..9,
            rolls in prop::collection::vec(any::<u32>(), 200),
        ) {
            let roster = roster(size);
            let mut season = Season::default();
            for roll in rolls {
                let pair = season.pick(&roster, roll);
                prop_assert_ne!(&pair.0, &pair.1);
                prop_assert!(roster.contains(&pair.0));
                prop_assert!(roster.contains(&pair.1));
                season.fought(pair);
            }
        }
    }

    /// With 4 characters and any roll, no cross repeats at less than 5 fights: a walk of every
    /// reachable state, not a sample (RF20).
    #[test]
    fn with_four_characters_a_cross_never_repeats_within_five_fights() {
        let roster = roster(4);
        let all: Vec<(String, String)> = roster
            .iter()
            .flat_map(|a| {
                roster
                    .iter()
                    .filter(move |b| *b != a)
                    .map(move |b| (a.clone(), b.clone()))
            })
            .collect();
        type State = (BTreeSet<usize>, Vec<usize>);
        let mut seen: BTreeSet<State> = BTreeSet::new();
        let mut stack: Vec<State> = vec![(BTreeSet::new(), Vec::new())];
        let index = |pair: (&str, &str)| {
            all.iter()
                .position(|p| (p.0.as_str(), p.1.as_str()) == pair)
                .unwrap()
        };
        let mut shared = 0_u32;
        let mut edges = 0_u32;
        while let Some(state) = stack.pop() {
            if !seen.insert(state.clone()) {
                continue;
            }
            let (used, recent) = state;
            let used_pairs: Vec<(String, String)> = used.iter().map(|i| all[*i].clone()).collect();
            let recent_pairs: Vec<(String, String)> =
                recent.iter().map(|i| all[*i].clone()).collect();
            // Every distinct choice: `roll` only selects among what is left, and no more than 12.
            let mut choices = BTreeSet::new();
            for roll in 0..12 {
                let pick = pick_matchup(&roster, &used_pairs, &recent_pairs, roll).unwrap();
                choices.insert((index((pick.a, pick.b)), pick.new_round));
            }
            for (choice, new_round) in choices {
                let pair = &all[choice];
                for earlier in &recent_pairs {
                    assert_ne!(
                        cross(earlier),
                        cross(pair),
                        "{recent_pairs:?} then {pair:?}"
                    );
                }
                edges += 1;
                if let Some(last) = recent_pairs.last()
                    && shares_character(
                        (pair.0.as_str(), pair.1.as_str()),
                        (last.0.as_str(), last.1.as_str()),
                    )
                {
                    shared += 1;
                }
                let mut next_used = if new_round {
                    BTreeSet::new()
                } else {
                    used.clone()
                };
                next_used.insert(choice);
                let mut next_recent = recent.clone();
                next_recent.push(choice);
                if next_recent.len() > 4 {
                    next_recent.remove(0);
                }
                stack.push((next_used, next_recent));
            }
        }
        assert!(seen.len() > 1000, "{}", seen.len());
        // About half of the fights share a character with the previous one (RF20).
        assert!(
            f64::from(shared) / f64::from(edges) > 0.4,
            "{shared}/{edges}"
        );
        assert!(
            f64::from(shared) / f64::from(edges) < 0.6,
            "{shared}/{edges}"
        );
    }

    #[test]
    fn fewer_than_two_characters_have_no_pair() {
        assert_eq!(pick_matchup(&roster(0), &[], &[], 0), None);
        assert_eq!(pick_matchup(&roster(1), &[], &[], 0), None);
    }

    #[test]
    fn the_preferences_are_inside_the_candidates() {
        // Only one pair left in the round, and it is recent: it still comes, in this round.
        let roster = roster(2);
        let used = vec![("c0".to_owned(), "c1".to_owned())];
        let recent = used.clone();
        let pick = pick_matchup(&roster, &used, &recent, 7).unwrap();
        assert_eq!((pick.a, pick.b, pick.new_round), ("c1", "c0", false));
    }

    #[test]
    fn the_roster_hash_depends_on_the_slugs() {
        let one = roster_sha256(&roster(4));
        assert_eq!(one, roster_sha256(&roster(4)));
        assert_ne!(one, roster_sha256(&roster(5)));
        // The separator: these two would be equal by plain concatenation.
        assert_ne!(
            roster_sha256(&["ab".to_owned(), "c".to_owned()]),
            roster_sha256(&["a".to_owned(), "bc".to_owned()])
        );
    }
}
