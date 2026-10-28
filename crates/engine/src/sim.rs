//! Turn-based simulation with simulated time in integer ms (003 RF7–RF9, "Modelo de
//! simulación"). Every roll goes through [`Rolls`], in the order of RF7: initiative, then per
//! action choice, defense, variance, critical and jitter (steps 1–5) and, after an action
//! without KO nor knockdown, the breather roll and its duration (steps 6–7, 006 RF2–RF3).
use std::num::NonZeroU32;

use protocol::{AbilityKind, EndReason, FightEvent, FightLog, FighterInfo, MomentKind, Side};

use crate::rng::{Rolls, bound};
use crate::ruleset::{EVENTS_PER_ACTION_MAX, EVENTS_TAIL_MAX, Ruleset};
use crate::spec::{CharacterSpec, MoveKind, STRIKE_ID, SpecError};

const PERMILLE: NonZeroU32 = bound(1000);
const COIN: NonZeroU32 = bound(2);

// --- Formulas (one per row of the ruleset table) -------------------------------------------

/// `hp * 1000 / max_hp`.
pub(crate) fn relative(hp: u16, max_hp: u16) -> u32 {
    u32::from(hp) * 1000 / u32::from(max_hp.max(1))
}

/// `hp_base + hp_per_point * hp`, if it fits in `u16` (the bounds test proves it does).
pub(crate) fn max_hp(r: &Ruleset, hp_stat: u8) -> Option<u16> {
    u16::try_from(r.hp_base() + r.hp_per_point() * u32::from(hp_stat)).ok()
}

pub(crate) fn interval(r: &Ruleset, speed: u8) -> u32 {
    r.interval_base_ms()
        .saturating_sub(r.interval_per_speed_ms() * u32::from(speed))
}

pub(crate) fn first_turn(r: &Ruleset, speed: u8) -> u32 {
    r.intro_ms() + interval(r, speed) / 2
}

pub(crate) const fn windup(r: &Ruleset, kind: AbilityKind) -> u32 {
    match kind {
        AbilityKind::Strike => r.windup_strike_ms(),
        AbilityKind::Heavy => r.windup_heavy_ms(),
        AbilityKind::Special => r.windup_special_ms(),
    }
}

pub(crate) fn dodge_permille(r: &Ruleset, speed_def: u8) -> u32 {
    r.dodge_base() + r.dodge_per_speed() * u32::from(speed_def)
}

pub(crate) fn block_permille(r: &Ruleset, defense_def: u8, caution_def: u8) -> u32 {
    r.block_base() + u32::from(defense_def) / r.block_defense_div() + u32::from(caution_def)
}

pub(crate) fn is_desperate(r: &Ruleset, hp: u16, max_hp: u16) -> bool {
    relative(hp, max_hp) <= r.desperation_permille()
}

pub(crate) fn crit_permille(r: &Ruleset, special_atk: u8, desperate: bool) -> u32 {
    let bonus = if desperate {
        r.desperation_crit_bonus()
    } else {
        0
    };
    r.crit_base() + r.crit_per_special() * u32::from(special_atk) + bonus
}

/// `power * (offset + off) / (offset + def)`.
pub(crate) fn base_damage(r: &Ruleset, power: u32, off: u8, def: u8) -> u32 {
    power * (r.damage_offset() + u32::from(off)) / (r.damage_offset() + u32::from(def))
}

/// Damage of a hit and the same damage without the critical multiplier (for
/// `decisive_crit`): variance, desperation, critical, weakness, then at least 1.
pub(crate) fn hit_damage(
    r: &Ruleset,
    base: u32,
    variance: u32,
    desperate: bool,
    crit: bool,
    weak: bool,
) -> (u32, u32) {
    let mut d = base * (r.variance_min() + variance) / 100;
    if desperate {
        d = d * r.desperation_dmg_num() / r.desperation_dmg_den();
    }
    let mut without_crit = d;
    if crit {
        d = d * r.crit_num() / r.crit_den();
    }
    if weak {
        d = d * r.weak_num() / r.weak_den();
        without_crit = without_crit * r.weak_num() / r.weak_den();
    }
    (d.max(1), without_crit.max(1))
}

/// Damage of a block: variance, weakness, chip. No critical nor desperation; may be 0.
pub(crate) fn block_damage(r: &Ruleset, base: u32, variance: u32, weak: bool) -> u32 {
    let mut d = base * (r.variance_min() + variance) / 100;
    if weak {
        d = d * r.weak_num() / r.weak_den();
    }
    d * r.block_chip_num() / r.block_chip_den()
}

/// Damage applied: `min(d, hp)`. Exact: if `d` does not fit in `u16` it is above `hp`.
pub(crate) fn applied(d: u32, hp: u16) -> u16 {
    u16::try_from(d).map_or(hp, |d| d.min(hp))
}

/// Result by time (RF9): higher relative hp wins by decision; equal is a draw.
pub(crate) fn decide(rel_a: u32, rel_b: u32) -> (Option<Side>, EndReason) {
    match rel_a.cmp(&rel_b) {
        std::cmp::Ordering::Greater => (Some(Side::A), EndReason::Decision),
        std::cmp::Ordering::Less => (Some(Side::B), EndReason::Decision),
        std::cmp::Ordering::Equal => (None, EndReason::Draw),
    }
}

/// Who acts (RF8): lower `next_at`; on a tie, the one that did not act last; if nobody has
/// acted, the one that won the initiative.
pub(crate) fn next_actor(a_next: u32, b_next: u32, last: Option<Side>, initiative: Side) -> Side {
    match a_next.cmp(&b_next) {
        std::cmp::Ordering::Less => Side::A,
        std::cmp::Ordering::Greater => Side::B,
        std::cmp::Ordering::Equal => last.map_or(initiative, Side::opponent),
    }
}

/// `comeback`: a side that was at some point `comeback_deficit_permille` or more behind is
/// now strictly ahead (evaluated with the marks from before this resolution).
pub(crate) const fn is_comeback(
    was_behind: bool,
    sent: bool,
    rel_self: u32,
    rel_other: u32,
) -> bool {
    was_behind && !sent && rel_self > rel_other
}

/// The mark "was `comeback_deficit_permille` or more behind".
pub(crate) fn is_behind(r: &Ruleset, rel_self: u32, rel_other: u32) -> bool {
    rel_other >= rel_self + r.comeback_deficit_permille()
}

/// Moments of a KO (the attacker wins): `decisive_crit` if the critical made the difference,
/// `clutch_ko` if the winner was at `clutch_permille` or less.
pub(crate) fn ko_moments(
    r: &Ruleset,
    crit: bool,
    without_crit: u32,
    hp_before: u16,
    rel_winner: u32,
) -> (bool, bool) {
    (
        crit && without_crit < u32::from(hp_before),
        rel_winner <= r.clutch_permille(),
    )
}

const fn ability_kind(kind: MoveKind) -> AbilityKind {
    match kind {
        MoveKind::Heavy => AbilityKind::Heavy,
        MoveKind::Special => AbilityKind::Special,
    }
}

// --- State ----------------------------------------------------------------------------------

struct Fighter<'a> {
    spec: &'a CharacterSpec,
    max_hp: u16,
    hp: u16,
    next_at: u32,
    /// Own actions left before each ability is available again (0 = available).
    cooldowns: Vec<u8>,
    low_hp_sent: bool,
    was_behind: bool,
    comeback_sent: bool,
}

impl<'a> Fighter<'a> {
    fn new(spec: &'a CharacterSpec, r: &Ruleset) -> Result<Self, SpecError> {
        let max_hp = max_hp(r, spec.stats.hp).ok_or(SpecError::StatOutOfRange)?;
        Ok(Self {
            spec,
            max_hp,
            hp: max_hp,
            next_at: first_turn(r, spec.stats.speed),
            cooldowns: vec![0; spec.abilities.len()],
            low_hp_sent: false,
            was_behind: false,
            comeback_sent: false,
        })
    }

    fn relative(&self) -> u32 {
        relative(self.hp, self.max_hp)
    }

    /// Weighted choice (RF7 step 1): strike, then the available abilities in file order.
    /// `None` is the strike.
    fn choose<R: Rolls>(&self, rolls: &mut R, r: &Ruleset, desperate: bool) -> Option<usize> {
        let p = self.spec.personality;
        let strike = r.w_strike_base() + u32::from(p.aggression);
        let mut options = vec![(None, strike)];
        for (i, (ability, cooldown)) in self.spec.abilities.iter().zip(&self.cooldowns).enumerate()
        {
            if *cooldown > 0 {
                continue;
            }
            let weight = match ability.kind {
                MoveKind::Heavy => r.w_heavy_base() + u32::from(p.aggression),
                MoveKind::Special => {
                    let w = r.w_special_base() + r.w_special_per_show() * u32::from(p.showmanship);
                    if desperate {
                        w * r.w_special_desperation_mul()
                    } else {
                        w
                    }
                }
            };
            options.push((Some(i), weight));
        }
        let total = options
            .iter()
            .fold(0_u32, |sum, (_, weight)| sum.saturating_add(*weight));
        let pick = rolls.roll(bound(total));
        let mut cumulative = 0_u32;
        for (choice, weight) in options {
            cumulative = cumulative.saturating_add(weight);
            if pick < cumulative {
                return choice;
            }
        }
        None
    }

    /// After choosing: the used ability waits `cooldown` own actions, the others count down.
    fn tick_cooldowns(&mut self, used: Option<usize>) {
        for (i, (counter, ability)) in self
            .cooldowns
            .iter_mut()
            .zip(&self.spec.abilities)
            .enumerate()
        {
            if used == Some(i) {
                *counter = ability.cooldown;
            } else {
                *counter = counter.saturating_sub(1);
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Defense {
    Dodge,
    Block,
    Hit,
}

struct Fight<'a> {
    r: &'a Ruleset,
    a: Fighter<'a>,
    b: Fighter<'a>,
    events: Vec<FightEvent>,
    last: Option<Side>,
    /// Resolutions without KO nor knockdown since the start, the last breather or the last
    /// knockdown (006 RF2).
    since_breather: u32,
}

/// Simulates a fight between two **already validated** characters.
pub(crate) fn run<R: Rolls>(
    rolls: &mut R,
    a: &CharacterSpec,
    b: &CharacterSpec,
    r: &Ruleset,
) -> Result<FightLog, SpecError> {
    let initiative = if rolls.roll(COIN) == 0 {
        Side::A
    } else {
        Side::B
    };
    let mut fight = Fight {
        r,
        a: Fighter::new(a, r)?,
        b: Fighter::new(b, r)?,
        events: vec![FightEvent::Intro { t: 0 }],
        last: None,
        since_breather: 0,
    };
    let reserve = (EVENTS_PER_ACTION_MAX + EVENTS_TAIL_MAX) as usize;
    loop {
        let side = next_actor(fight.a.next_at, fight.b.next_at, fight.last, initiative);
        let at = match side {
            Side::A => fight.a.next_at,
            Side::B => fight.b.next_at,
        };
        if at.saturating_add(r.windup_special_ms()) > r.time_limit_ms() {
            fight.time_up();
            break;
        }
        if fight.events.len() + reserve > usize::from(r.max_events()) {
            return Err(SpecError::EventLimit);
        }
        if fight.action(rolls, side) {
            break;
        }
    }
    Ok(FightLog {
        ruleset_version: r.version(),
        a: FighterInfo {
            slug: a.slug.clone(),
            max_hp: fight.a.max_hp,
        },
        b: FighterInfo {
            slug: b.slug.clone(),
            max_hp: fight.b.max_hp,
        },
        events: fight.events,
    })
}

impl Fight<'_> {
    fn time_up(&mut self) {
        let (winner, reason) = decide(self.a.relative(), self.b.relative());
        let t = self.r.time_limit_ms();
        self.events.push(FightEvent::TimeUp { t });
        self.events.push(FightEvent::End {
            t: t + self.r.end_ms(),
            winner,
            reason,
        });
    }

    /// One atomic action of `side`, from `attack` to its consequences. Returns `true` on KO.
    fn action<R: Rolls>(&mut self, rolls: &mut R, side: Side) -> bool {
        let r = self.r;
        let target = side.opponent();
        let (atk, def) = match side {
            Side::A => (&mut self.a, &mut self.b),
            Side::B => (&mut self.b, &mut self.a),
        };
        let events = &mut self.events;

        // 1. Choice.
        let t = atk.next_at;
        let desperate = is_desperate(r, atk.hp, atk.max_hp);
        let choice = atk.choose(rolls, r, desperate);
        atk.tick_cooldowns(choice);
        let ability = choice.and_then(|i| atk.spec.abilities.get(i));
        let (id, kind, power, element) = match ability {
            None => (STRIKE_ID, AbilityKind::Strike, r.strike_power(), None),
            Some(ab) => (
                ab.id.as_str(),
                ability_kind(ab.kind),
                u32::from(ab.power),
                ab.element,
            ),
        };
        events.push(FightEvent::Attack {
            t,
            attacker: side,
            ability: id.to_owned(),
            kind,
        });
        let t_res = t + windup(r, kind);

        // 2. Defense.
        let roll = rolls.roll(PERMILLE);
        let dodge = dodge_permille(r, def.spec.stats.speed);
        let block = block_permille(r, def.spec.stats.defense, def.spec.personality.caution);
        let defense = if roll < dodge {
            Defense::Dodge
        } else if roll < dodge + block {
            Defense::Block
        } else {
            Defense::Hit
        };

        // 3–4. Variance and critical.
        let weak = element.is_some_and(|e| def.spec.weaknesses.contains(&e));
        let (off, dfn) = if kind == AbilityKind::Special {
            (atk.spec.stats.special, def.spec.stats.special)
        } else {
            (atk.spec.stats.attack, def.spec.stats.defense)
        };
        let base = base_damage(r, power, off, dfn);
        let hp_before = def.hp;
        let mut crit = false;
        let mut without_crit = 0;
        let damage = match defense {
            Defense::Dodge => 0,
            Defense::Block => {
                let variance = rolls.roll(bound(r.variance_span()));
                applied(block_damage(r, base, variance, weak), hp_before)
            }
            Defense::Hit => {
                let variance = rolls.roll(bound(r.variance_span()));
                crit = rolls.roll(PERMILLE) < crit_permille(r, atk.spec.stats.special, desperate);
                let (d, nc) = hit_damage(r, base, variance, desperate, crit, weak);
                without_crit = nc;
                applied(d, hp_before)
            }
        };

        // 5. Jitter of the attacker's next turn (always consumed, decision 9 of plan-a).
        let jitter = rolls.roll(bound(r.jitter_ms().saturating_add(1)));

        def.hp = hp_before - damage;
        let ability = id.to_owned();
        events.push(match defense {
            Defense::Dodge => FightEvent::Dodge {
                t: t_res,
                attacker: side,
                ability,
            },
            Defense::Block => FightEvent::Block {
                t: t_res,
                attacker: side,
                ability,
                damage,
                hp: def.hp,
            },
            Defense::Hit => FightEvent::Hit {
                t: t_res,
                attacker: side,
                ability,
                damage,
                crit,
                weak,
                hp: def.hp,
            },
        });

        // Timing.
        atk.next_at = t_res + interval(r, atk.spec.stats.speed) + jitter;
        def.next_at = def.next_at.max(t_res + r.recovery_ms());
        let knockdown = defense == Defense::Hit
            && crit
            && def.hp > 0
            && u32::from(damage) * 1000 >= r.knockdown_min_permille() * u32::from(def.max_hp);
        if knockdown {
            let duration = u32::from(r.knockdown_ms());
            events.push(FightEvent::Knockdown {
                t: t_res,
                side: target,
                duration_ms: r.knockdown_ms(),
            });
            def.next_at = def.next_at.max(t_res + duration + r.recovery_ms());
            atk.next_at = atk.next_at.max(t_res + duration);
        }

        // Low hp of the target.
        if def.hp > 0 && !def.low_hp_sent && def.relative() <= r.low_hp_permille() {
            def.low_hp_sent = true;
            events.push(FightEvent::LowHp {
                t: t_res,
                side: target,
                hp: def.hp,
            });
        }

        // Comeback, with the marks from before this resolution; then the marks.
        let (rel_atk, rel_def) = (atk.relative(), def.relative());
        for (fighter, who, rel_self, rel_other) in [
            (&mut *atk, side, rel_atk, rel_def),
            (&mut *def, target, rel_def, rel_atk),
        ] {
            if is_comeback(
                fighter.was_behind,
                fighter.comeback_sent,
                rel_self,
                rel_other,
            ) {
                fighter.comeback_sent = true;
                events.push(FightEvent::Moment {
                    t: t_res,
                    kind: MomentKind::Comeback,
                    side: who,
                });
            }
        }
        atk.was_behind |= is_behind(r, rel_atk, rel_def);
        def.was_behind |= is_behind(r, rel_def, rel_atk);

        // KO.
        if def.hp == 0 {
            let (decisive, clutch) = ko_moments(r, crit, without_crit, hp_before, rel_atk);
            if decisive {
                events.push(FightEvent::Moment {
                    t: t_res,
                    kind: MomentKind::DecisiveCrit,
                    side,
                });
            }
            if clutch {
                events.push(FightEvent::Moment {
                    t: t_res,
                    kind: MomentKind::ClutchKo,
                    side,
                });
            }
            events.push(FightEvent::Ko {
                t: t_res,
                side: target,
            });
            events.push(FightEvent::End {
                t: t_res + r.end_ms(),
                winner: Some(side),
                reason: EndReason::Ko,
            });
            return true;
        }
        self.last = Some(side);
        self.breather(rolls, knockdown);
        false
    }

    /// The breather after an action without KO (006 RF2, steps 6–7 of RF7): a knockdown resets
    /// the counter without rolling; otherwise, once the counter reaches
    /// `breather_after_resolutions`, a roll may move both clocks by the same duration, which
    /// keeps who acts next.
    fn breather<R: Rolls>(&mut self, rolls: &mut R, knockdown: bool) {
        let r = self.r;
        if knockdown {
            self.since_breather = 0;
            return;
        }
        self.since_breather = self.since_breather.saturating_add(1);
        if self.since_breather < r.breather_after_resolutions() {
            return;
        }
        // 6. Does a breather start?
        if rolls.roll(PERMILLE) >= r.breather_chance_permille() {
            return;
        }
        // 7. Its duration.
        let span = rolls.roll(bound(r.breather_span_ms().saturating_add(1)));
        let d = r.breather_min_ms().saturating_add(span);
        self.a.next_at = self.a.next_at.saturating_add(d);
        self.b.next_at = self.b.next_at.saturating_add(d);
        self.since_breather = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::Roller;
    use crate::spec::{Ability, Element, Personality, Stats};
    use crate::{Seed, simulate};

    fn r() -> Ruleset {
        Ruleset::v1()
    }

    fn fighter(
        slug: &str,
        stats: [u8; 5],
        personality: [u8; 3],
        abilities: Vec<Ability>,
    ) -> CharacterSpec {
        let [hp, attack, defense, speed, special] = stats;
        let [aggression, caution, showmanship] = personality;
        CharacterSpec {
            slug: slug.to_owned(),
            stats: Stats {
                hp,
                attack,
                defense,
                speed,
                special,
            },
            personality: Personality {
                aggression,
                caution,
                showmanship,
            },
            abilities,
            weaknesses: vec![],
        }
    }

    fn ability(id: &str, kind: MoveKind, power: u8, cooldown: u8) -> Ability {
        Ability {
            id: id.to_owned(),
            kind,
            power,
            cooldown,
            element: None,
        }
    }

    fn plain(slug: &str, stats: [u8; 5]) -> CharacterSpec {
        fighter(
            slug,
            stats,
            [50, 50, 50],
            vec![ability("big_one", MoveKind::Heavy, 40, 2)],
        )
    }

    /// Defense outcome the script forces: dodge = roll 0, hit = roll 999, block = the given
    /// roll (the test computes one inside the block band of the defender).
    #[derive(Clone, Copy)]
    enum Def {
        Dodge,
        Block(u32),
        Hit,
    }

    /// One action as the script sees it.
    #[derive(Clone, Copy)]
    struct Plan {
        /// Pick the last available option instead of the first (the strike).
        last_option: bool,
        defense: Def,
        variance: u32,
        crit: bool,
        /// Answer to step 6, if the engine asks for it: 0 (breather) or 999 (none).
        breather: bool,
        /// Answer to step 7 (`roll(801)`).
        breather_ms: u32,
    }

    const DODGE: Plan = Plan {
        last_option: false,
        defense: Def::Dodge,
        variance: 0,
        crit: false,
        breather: false,
        breather_ms: 0,
    };

    const HIT: Plan = Plan {
        defense: Def::Hit,
        ..DODGE
    };

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Step {
        Initiative,
        Choice,
        Defense,
        Variance,
        Crit,
        Jitter,
        /// After the jitter: step 6 (bound 1000) or the choice of the next action.
        AfterJitter,
        BreatherSpan,
    }

    /// A roller that follows the RF7 order as a state machine, checks every bound it is asked
    /// for, and answers from a per-action plan. After the jitter, the bound tells step 6
    /// (`roll(1000)`) from the next choice, whose weight totals never reach 1000 in these tests.
    struct Scripted<F> {
        step: Step,
        action: usize,
        plan: Plan,
        initiative: u32,
        strategy: F,
        /// Actions (0-based) after which step 6 was asked for.
        breather_asks: Vec<usize>,
    }

    impl<F: FnMut(usize) -> Plan> Scripted<F> {
        fn new(initiative: u32, strategy: F) -> Self {
            Self {
                step: Step::Initiative,
                action: 0,
                plan: DODGE,
                initiative,
                strategy,
                breather_asks: vec![],
            }
        }

        fn choice(&mut self, n: u32) -> (u32, Step) {
            assert_ne!(n, 1000, "a weight total that looks like step 6");
            self.plan = (self.strategy)(self.action);
            self.action += 1;
            (if self.plan.last_option { n - 1 } else { 0 }, Step::Defense)
        }
    }

    impl<F: FnMut(usize) -> Plan> Rolls for Scripted<F> {
        fn roll(&mut self, n: NonZeroU32) -> u32 {
            let n = n.get();
            let (value, next) = match self.step {
                Step::Initiative => {
                    assert_eq!(n, 2);
                    (self.initiative, Step::Choice)
                }
                Step::Choice => self.choice(n),
                Step::Defense => {
                    assert_eq!(n, 1000);
                    match self.plan.defense {
                        Def::Dodge => (0, Step::Jitter),
                        Def::Block(roll) => (roll, Step::Variance),
                        Def::Hit => (999, Step::Variance),
                    }
                }
                Step::Variance => {
                    assert_eq!(n, 31);
                    let next = match self.plan.defense {
                        Def::Hit => Step::Crit,
                        Def::Dodge | Def::Block(_) => Step::Jitter,
                    };
                    (self.plan.variance, next)
                }
                Step::Crit => {
                    assert_eq!(n, 1000);
                    (if self.plan.crit { 0 } else { 999 }, Step::Jitter)
                }
                Step::Jitter => {
                    assert_eq!(n, 201);
                    (0, Step::AfterJitter)
                }
                Step::AfterJitter if n == 1000 => {
                    self.breather_asks.push(self.action - 1);
                    if self.plan.breather {
                        (0, Step::BreatherSpan)
                    } else {
                        (999, Step::Choice)
                    }
                }
                Step::AfterJitter => self.choice(n),
                Step::BreatherSpan => {
                    assert_eq!(n, 801);
                    (self.plan.breather_ms, Step::Choice)
                }
            };
            self.step = next;
            value
        }
    }

    fn run_scripted(
        a: &CharacterSpec,
        b: &CharacterSpec,
        initiative: u32,
        strategy: impl FnMut(usize) -> Plan,
    ) -> FightLog {
        run_asked(a, b, initiative, strategy).0
    }

    /// The log and the actions after which step 6 was asked for.
    fn run_asked(
        a: &CharacterSpec,
        b: &CharacterSpec,
        initiative: u32,
        strategy: impl FnMut(usize) -> Plan,
    ) -> (FightLog, Vec<usize>) {
        a.validate(&r()).unwrap();
        b.validate(&r()).unwrap();
        let mut rolls = Scripted::new(initiative, strategy);
        let log = run(&mut rolls, a, b, &r()).unwrap();
        (log, rolls.breather_asks)
    }

    fn resolution_times(log: &FightLog) -> Vec<u32> {
        log.events
            .iter()
            .filter(|e| {
                matches!(
                    e,
                    FightEvent::Hit { .. } | FightEvent::Block { .. } | FightEvent::Dodge { .. }
                )
            })
            .map(FightEvent::t)
            .collect()
    }

    fn attacks(log: &FightLog) -> Vec<(u32, Side, String)> {
        log.events
            .iter()
            .filter_map(|e| match e {
                FightEvent::Attack {
                    t,
                    attacker,
                    ability,
                    ..
                } => Some((*t, *attacker, ability.clone())),
                _ => None,
            })
            .collect()
    }

    fn resolution_time(log: &FightLog, nth: usize) -> u32 {
        log.events
            .iter()
            .filter(|e| {
                matches!(
                    e,
                    FightEvent::Hit { .. } | FightEvent::Block { .. } | FightEvent::Dodge { .. }
                )
            })
            .nth(nth)
            .map(FightEvent::t)
            .unwrap()
    }

    // --- Formulas -------------------------------------------------------------------------

    #[test]
    fn formulas_of_the_table() {
        let r = r();
        assert_eq!(max_hp(&r, 1), Some(504));
        assert_eq!(max_hp(&r, 100), Some(900));
        assert_eq!(interval(&r, 100), 800);
        assert_eq!(interval(&r, 1), 1790);
        assert_eq!(first_turn(&r, 100), 3400);
        assert_eq!(first_turn(&r, 85), 3475);
        assert_eq!(windup(&r, AbilityKind::Strike), 400);
        assert_eq!(windup(&r, AbilityKind::Heavy), 700);
        assert_eq!(windup(&r, AbilityKind::Special), 1100);
        assert_eq!(dodge_permille(&r, 85), 210);
        assert_eq!(block_permille(&r, 45, 20), 60 + 22 + 20);
        assert_eq!(crit_permille(&r, 55, false), 95);
        assert_eq!(crit_permille(&r, 55, true), 245);
        // Strike 20 * (100 + 60) / (100 + 45) = 22; special 70 * 155 / 160 = 67.
        assert_eq!(base_damage(&r, 20, 60, 45), 22);
        assert_eq!(base_damage(&r, 70, 55, 60), 67);
        assert!(is_desperate(&r, 225, 900));
        assert!(!is_desperate(&r, 226, 900));
        assert_eq!(relative(0, 504), 0);
        assert_eq!(relative(504, 504), 1000);
    }

    #[test]
    fn hit_damage_order_is_variance_desperation_crit_weakness() {
        let r = r();
        // 100 * 85 / 100 = 85.
        assert_eq!(hit_damage(&r, 100, 0, false, false, false), (85, 85));
        // 100 * 115 / 100 = 115.
        assert_eq!(hit_damage(&r, 100, 30, false, false, false), (115, 115));
        // 85 * 5/4 = 106; crit 106 * 3/2 = 159; weak 159 * 5/4 = 198, without crit 106 * 5/4 = 132.
        assert_eq!(hit_damage(&r, 100, 0, true, true, true), (198, 132));
        // Crit without desperation: 85 * 3/2 = 127.
        assert_eq!(hit_damage(&r, 100, 0, false, true, false), (127, 85));
        // Never below 1.
        assert_eq!(hit_damage(&r, 0, 0, false, false, false), (1, 1));
    }

    #[test]
    fn block_damage_is_variance_weakness_chip() {
        let r = r();
        // 100 * 85 / 100 = 85, * 3/10 = 25.
        assert_eq!(block_damage(&r, 100, 0, false), 25);
        // 85 * 5/4 = 106, * 3/10 = 31.
        assert_eq!(block_damage(&r, 100, 0, true), 31);
        // May be 0.
        assert_eq!(block_damage(&r, 2, 0, false), 0);
    }

    #[test]
    fn applied_damage_is_clipped_to_the_hp() {
        assert_eq!(applied(30, 100), 30);
        assert_eq!(applied(130, 100), 100);
        assert_eq!(applied(u32::MAX, 7), 7);
    }

    #[test]
    fn decision_and_draw() {
        assert_eq!(decide(600, 500), (Some(Side::A), EndReason::Decision));
        assert_eq!(decide(499, 500), (Some(Side::B), EndReason::Decision));
        assert_eq!(decide(500, 500), (None, EndReason::Draw));
        // Same permille with a different max_hp: 252/504 and 450/900 are both 500.
        assert_eq!(
            decide(relative(252, 504), relative(450, 900)),
            (None, EndReason::Draw)
        );
    }

    #[test]
    fn turn_order() {
        assert_eq!(next_actor(10, 20, None, Side::B), Side::A);
        assert_eq!(next_actor(30, 20, Some(Side::B), Side::A), Side::B);
        assert_eq!(next_actor(20, 20, None, Side::B), Side::B);
        assert_eq!(next_actor(20, 20, None, Side::A), Side::A);
        assert_eq!(next_actor(20, 20, Some(Side::A), Side::A), Side::B);
        assert_eq!(next_actor(20, 20, Some(Side::B), Side::A), Side::A);
    }

    #[test]
    fn moments_in_constructed_scenarios() {
        let r = r();
        // Comeback: needs the previous mark and strictly ahead, once.
        assert!(is_comeback(true, false, 501, 500));
        assert!(!is_comeback(true, false, 500, 500));
        assert!(!is_comeback(false, false, 900, 100));
        assert!(!is_comeback(true, true, 900, 100));
        assert!(is_behind(&r, 500, 750));
        assert!(!is_behind(&r, 500, 749));
        // Decisive crit: the crit made the KO; the damage without it would not have.
        assert_eq!(ko_moments(&r, true, 40, 50, 900), (true, false));
        assert_eq!(ko_moments(&r, true, 50, 50, 900), (false, false));
        assert_eq!(ko_moments(&r, false, 40, 40, 900), (false, false));
        // Clutch KO: the winner at 60 ‰ or less.
        assert_eq!(ko_moments(&r, false, 10, 10, 60), (false, true));
        assert_eq!(ko_moments(&r, false, 10, 10, 61), (false, false));
    }

    // --- Scripted fights ------------------------------------------------------------------

    #[test]
    fn rolls_follow_the_rf7_order_in_a_real_fight() {
        struct Recorder<R> {
            inner: R,
            asked: Vec<u32>,
            given: Vec<u32>,
        }
        impl<R: Rolls> Rolls for Recorder<R> {
            fn roll(&mut self, n: NonZeroU32) -> u32 {
                let value = self.inner.roll(n);
                self.asked.push(n.get());
                self.given.push(value);
                value
            }
        }
        /// One action as the log shows it: its rolls of steps 1–5, knockdown and KO.
        struct Seen {
            rolls: &'static [u32],
            knockdown: bool,
            ko: bool,
        }
        let r = r();
        let a = plain("one-a", [60, 60, 60, 60, 60]);
        let b = plain("one-b", [70, 70, 40, 60, 60]);
        let mut breathers = 0;
        for byte in 0..8 {
            let mut rec = Recorder {
                inner: Roller::from_seed(&Seed::from_bytes([byte; 32])),
                asked: vec![],
                given: vec![],
            };
            let log = run(&mut rec, &a, &b, &r).unwrap();
            let mut actions: Vec<Seen> = vec![];
            for event in &log.events {
                // 0 stands for the weight total of the choice, which depends on the cooldowns.
                let rolls: &'static [u32] = match event {
                    FightEvent::Dodge { .. } => &[0, 1000, 201],
                    FightEvent::Block { .. } => &[0, 1000, 31, 201],
                    FightEvent::Hit { .. } => &[0, 1000, 31, 1000, 201],
                    FightEvent::Knockdown { .. } => {
                        if let Some(seen) = actions.last_mut() {
                            seen.knockdown = true;
                        }
                        continue;
                    }
                    FightEvent::Ko { .. } => {
                        if let Some(seen) = actions.last_mut() {
                            seen.ko = true;
                        }
                        continue;
                    }
                    _ => continue,
                };
                actions.push(Seen {
                    rolls,
                    knockdown: false,
                    ko: false,
                });
            }
            // Steps 6–7 with the counter of RF2 replicated.
            let mut expected = vec![2];
            let mut since = 0;
            for seen in &actions {
                expected.extend(seen.rolls);
                if seen.ko {
                    continue;
                }
                if seen.knockdown {
                    since = 0;
                    continue;
                }
                since += 1;
                if since >= r.breather_after_resolutions() {
                    let at = expected.len();
                    expected.push(1000);
                    let given = rec.given.get(at).copied().unwrap();
                    if given < r.breather_chance_permille() {
                        expected.push(r.breather_span_ms() + 1);
                        since = 0;
                        breathers += 1;
                    }
                }
            }
            assert_eq!(rec.asked.len(), expected.len());
            for (asked, want) in rec.asked.iter().zip(&expected) {
                if *want == 0 {
                    assert!(*asked >= 100, "weight total {asked}");
                } else {
                    assert_eq!(asked, want);
                }
            }
        }
        assert!(breathers > 0, "no breather in 8 fights");
    }

    #[test]
    fn breather_moves_both_clocks_and_keeps_the_order() {
        let a = plain("one-a", [60, 60, 60, 60, 60]);
        let b = plain("one-b", [60, 60, 60, 50, 60]);
        let r = r();
        let nth = usize::try_from(r.breather_after_resolutions()).unwrap() - 1;
        let (calm, asks) = run_asked(&a, &b, 0, |_| DODGE);
        // Nothing is rolled before the 9th resolution; then after every action.
        assert_eq!(asks.first(), Some(&nth));
        assert_eq!(asks.get(1), Some(&(nth + 1)));
        // A roll >= chance moves nothing.
        assert!(
            resolution_times(&calm)
                .windows(2)
                .all(|w| matches!(w, [x, y] if y - x < r.breather_gap_min()))
        );
        for span in [0, r.breather_span_ms()] {
            let (paused, asks) = run_asked(&a, &b, 0, |i| Plan {
                breather: i == nth,
                breather_ms: span,
                ..DODGE
            });
            let d = r.breather_min_ms() + span;
            // The counter starts again after the breather.
            assert_eq!(asks.get(1), Some(&(2 * nth + 1)));
            let (t_res, next) = (
                resolution_time(&paused, nth),
                resolution_time(&paused, nth + 1),
            );
            assert!(next >= t_res + r.breather_gap_min(), "{t_res} -> {next}");
            assert!(next - t_res >= d, "{t_res} -> {next}");
            // Same attackers, every later attack exactly `d` later.
            let (before, after) = (attacks(&calm), attacks(&paused));
            for (i, (x, y)) in before.iter().zip(&after).enumerate() {
                assert_eq!(x.1, y.1, "attacker {i}");
                let shift = if i > nth { d } else { 0 };
                assert_eq!(y.0, x.0 + shift, "attack {i}");
            }
        }
    }

    #[test]
    fn knockdown_and_ko_do_not_roll_a_breather() {
        let r = r();
        let nth = usize::try_from(r.breather_after_resolutions()).unwrap() - 1;
        // A knockdown at action 4 resets the counter: the first ask is 9 actions later.
        let a = plain("puncher", [50, 100, 50, 50, 50]);
        let b = plain("glass", [1, 50, 1, 50, 50]);
        let crit = Plan {
            variance: 30,
            crit: true,
            ..HIT
        };
        let (log, asks) = run_asked(&a, &b, 0, |i| if i == 4 { crit } else { DODGE });
        assert!(
            log.events
                .iter()
                .any(|e| matches!(e, FightEvent::Knockdown { .. }))
        );
        assert_eq!(asks.first(), Some(&(4 + nth + 1)));
        // Plain hits until the KO: every action from the 9th is asked, except the KO.
        let (log, asks) = run_asked(&a, &b, 0, |_| HIT);
        assert_eq!(log.outcome().map(|o| o.1), Some(EndReason::Ko));
        let actions = attacks(&log).len();
        assert!(actions > nth + 1);
        assert_eq!(asks, (nth..actions - 1).collect::<Vec<_>>());
    }

    #[test]
    fn recovery_after_hit_block_and_dodge() {
        // A (speed 100) acts first at 3400 and resolves at 3800; B (speed 1) would act at
        // 3895, but recovery pushes it to 3800 + 300 = 4100.
        let a = plain("fast", [50, 50, 50, 100, 50]);
        let b = plain("slow", [50, 50, 50, 1, 50]);
        let block = Def::Block(dodge_permille(&r(), 1));
        for (first, resolution) in [
            (HIT, "hit"),
            (
                Plan {
                    defense: block,
                    ..DODGE
                },
                "block",
            ),
            (DODGE, "dodge"),
        ] {
            let log = run_scripted(&a, &b, 0, |i| if i == 0 { first } else { DODGE });
            let kind = log.events.get(2).map(|e| {
                serde_json::to_value(e)
                    .unwrap()
                    .get("type")
                    .unwrap()
                    .as_str()
                    .unwrap()
                    .to_owned()
            });
            assert_eq!(kind.as_deref(), Some(resolution));
            let attacks = attacks(&log);
            assert_eq!(attacks.first(), Some(&(3400, Side::A, "strike".to_owned())));
            assert_eq!(attacks.get(1), Some(&(4100, Side::B, "strike".to_owned())));
        }
    }

    #[test]
    fn knockdown_extends_both_clocks() {
        // A: attack 100, special 50 (crit 90 ‰); B: defense 1, hp 1 (max 504). Strike
        // 20 * 200 / 101 = 39, variance 30 → 44, crit → 66 ≥ 10 % of 504.
        let a = plain("puncher", [50, 100, 50, 50, 50]);
        let b = plain("glass", [1, 50, 1, 50, 50]);
        let crit = Plan {
            variance: 30,
            crit: true,
            ..HIT
        };
        let log = run_scripted(&a, &b, 0, |i| if i == 0 { crit } else { DODGE });
        let t_res = resolution_time(&log, 0);
        assert!(log.events.contains(&FightEvent::Hit {
            t: t_res,
            attacker: Side::A,
            ability: "strike".to_owned(),
            damage: 66,
            crit: true,
            weak: false,
            hp: 438,
        }));
        assert!(log.events.contains(&FightEvent::Knockdown {
            t: t_res,
            side: Side::B,
            duration_ms: 1500,
        }));
        // A (interval 1300) waits for the knockdown: t_res + 1500. B waits t_res + 1800, and
        // then A's strike (resolved at t_res + 1900) pushes it to t_res + 2200 by recovery.
        let attacks = attacks(&log);
        assert_eq!(
            attacks.get(1).map(|a| (a.0, a.1)),
            Some((t_res + 1500, Side::A))
        );
        assert_eq!(
            attacks.get(2).map(|a| (a.0, a.1)),
            Some((t_res + 2200, Side::B))
        );
        // No knockdown without a crit.
        let log = run_scripted(&a, &b, 0, |i| {
            if i == 0 {
                Plan {
                    crit: false,
                    ..crit
                }
            } else {
                DODGE
            }
        });
        assert!(
            !log.events
                .iter()
                .any(|e| matches!(e, FightEvent::Knockdown { .. }))
        );
    }

    #[test]
    fn time_limit_stops_new_attacks_and_ends_in_a_draw() {
        let a = plain("one-a", [60, 60, 60, 60, 60]);
        let b = plain("one-b", [60, 60, 60, 60, 60]);
        let log = run_scripted(&a, &b, 1, |_| DODGE);
        let r = r();
        let last = attacks(&log).last().map(|a| a.0).unwrap();
        assert!(last + r.windup_special_ms() <= r.time_limit_ms());
        // The next turn of either fighter would cross the limit: interval 1200 > 1100 left.
        assert!(last + interval(&r, 60) + r.windup_special_ms() > r.time_limit_ms());
        let tail: Vec<&FightEvent> = log.events.iter().rev().take(2).collect();
        assert_eq!(
            tail,
            [
                &FightEvent::End {
                    t: 90_000,
                    winner: None,
                    reason: EndReason::Draw
                },
                &FightEvent::TimeUp { t: 86_000 },
            ]
        );
        // B won the initiative (both start at 3600).
        assert_eq!(attacks(&log).first().map(|a| a.1), Some(Side::B));
    }

    #[test]
    fn cooldown_blocks_the_next_own_actions() {
        let mut a = plain("cooler", [60, 60, 60, 60, 60]);
        a.abilities = vec![ability("slam", MoveKind::Heavy, 40, 2)];
        let b = plain("other", [60, 60, 60, 60, 60]);
        // Always pick the last available option: slam when available, else strike.
        let log = run_scripted(&a, &b, 0, |_| Plan {
            last_option: true,
            ..DODGE
        });
        let used: Vec<String> = attacks(&log)
            .into_iter()
            .filter(|x| x.1 == Side::A)
            .map(|x| x.2)
            .take(7)
            .collect();
        assert_eq!(
            used,
            [
                "slam", "strike", "strike", "slam", "strike", "strike", "slam"
            ]
        );
        // Cooldown 0: available every action.
        a.abilities = vec![ability("jab", MoveKind::Heavy, 20, 0)];
        let log = run_scripted(&a, &b, 0, |_| Plan {
            last_option: true,
            ..DODGE
        });
        assert!(
            attacks(&log)
                .iter()
                .filter(|x| x.1 == Side::A)
                .all(|x| x.2 == "jab")
        );
    }

    #[test]
    fn ko_ends_the_fight_for_the_attacker() {
        // Everybody hits with a crit and max variance until someone falls.
        let a = plain("puncher", [50, 100, 50, 50, 50]);
        let b = plain("glass", [1, 50, 1, 50, 50]);
        let all_in = Plan {
            variance: 30,
            crit: true,
            ..HIT
        };
        let log = run_scripted(&a, &b, 0, |_| all_in);
        let (winner, reason) = log.outcome().unwrap();
        assert_eq!(reason, EndReason::Ko);
        let ko = log
            .events
            .iter()
            .find_map(|e| match e {
                FightEvent::Ko { t, side } => Some((*t, *side)),
                _ => None,
            })
            .unwrap();
        assert_eq!(winner, Some(ko.1.opponent()));
        assert_eq!(
            log.events.last().map(FightEvent::t),
            Some(ko.0 + r().end_ms())
        );
    }

    #[test]
    fn weakness_multiplies_and_is_reported() {
        let mut a = plain("burner", [50, 60, 50, 50, 90]);
        a.abilities = vec![Ability {
            element: Some(Element::Fire),
            ..ability("flame", MoveKind::Special, 100, 2)
        }];
        let mut b = plain("leaf", [50, 50, 50, 50, 50]);
        b.weaknesses = vec![Element::Fire];
        let hit = Plan {
            last_option: true,
            ..HIT
        };
        let log = run_scripted(&a, &b, 0, |i| if i == 0 { hit } else { DODGE });
        // Special: 100 * 190 / 150 = 126, * 85/100 = 107, weak * 5/4 = 133.
        let first_hit = log
            .events
            .iter()
            .find(|e| matches!(e, FightEvent::Hit { .. }));
        assert!(matches!(
            first_hit,
            Some(FightEvent::Hit {
                damage: 133,
                weak: true,
                crit: false,
                ..
            })
        ));
    }

    #[test]
    fn event_limit_is_a_guard_not_a_truncation() {
        let a = plain("one-a", [60, 60, 60, 60, 60]);
        let b = plain("one-b", [60, 60, 60, 60, 60]);
        let seed = Seed::from_bytes([3; 32]);
        let log = simulate(&seed, &a, &b, &r()).unwrap();
        // A badly calibrated ruleset: the engine returns the error instead of truncating.
        let tight = r().with_max_events(20);
        assert_eq!(simulate(&seed, &a, &b, &tight), Err(SpecError::EventLimit));
        // With exactly the room the fight needs plus the reserve, it fits.
        let reserve = EVENTS_PER_ACTION_MAX + EVENTS_TAIL_MAX;
        let exact = u16::try_from(log.events.len() + reserve as usize).unwrap();
        assert_eq!(
            simulate(&seed, &a, &b, &r().with_max_events(exact)),
            Ok(log)
        );
    }
}
