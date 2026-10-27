//! Fight log produced by the engine (`crates/engine`) and consumed by the renderer and the
//! narrator (003 RF1–RF3).
//!
//! Contract: no human-readable text (`slug` and `ability` are codes), no 64-bit integers, no
//! maps and no floats, so the canonical JSON (`serde_json::to_vec`, fields in declaration
//! order) is byte-stable across platforms. The log never contains the seed.
use serde::{Deserialize, Serialize};

/// A side of the fight: `a` or `b`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum Side {
    A,
    B,
}

impl Side {
    /// The other side.
    #[must_use]
    pub const fn opponent(self) -> Self {
        match self {
            Self::A => Self::B,
            Self::B => Self::A,
        }
    }
}

/// Kind of an action: the basic `strike` or an ability of the character.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum AbilityKind {
    Strike,
    Heavy,
    Special,
}

/// Dramatic moments computed inside the fight (the narrator's "favourite losing" is not one:
/// it depends on the pools).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum MomentKind {
    Comeback,
    DecisiveCrit,
    ClutchKo,
}

/// How the fight ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum EndReason {
    Ko,
    Decision,
    Draw,
}

/// A fighter as the log presents it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct FighterInfo {
    /// Character code, e.g. `rata-electrica`.
    pub slug: String,
    pub max_hp: u16,
}

/// The whole development of a fight. Its duration is the `t` of the `end` event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct FightLog {
    pub ruleset_version: u16,
    pub a: FighterInfo,
    pub b: FighterInfo,
    pub events: Vec<FightEvent>,
}

impl FightLog {
    /// Winner and reason, read from the `end` event. `None` only if the log has no `end`
    /// (e.g. a deserialized one); a log produced by the engine always has it.
    #[must_use]
    pub fn outcome(&self) -> Option<(Option<Side>, EndReason)> {
        self.events.iter().rev().find_map(|event| match event {
            FightEvent::End { winner, reason, .. } => Some((*winner, *reason)),
            _ => None,
        })
    }
}

/// One event of the log.
///
/// `t` = ms since the start of the fight (playback offset). The target of an attack is always
/// the side opposite to `attacker`. `damage` is the damage applied (never more than the
/// target's previous hp); `hp` is the target's hp afterwards.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FightEvent {
    Intro {
        t: u32,
    },
    Attack {
        t: u32,
        attacker: Side,
        ability: String,
        kind: AbilityKind,
    },
    Hit {
        t: u32,
        attacker: Side,
        ability: String,
        damage: u16,
        crit: bool,
        weak: bool,
        hp: u16,
    },
    Block {
        t: u32,
        attacker: Side,
        ability: String,
        damage: u16,
        hp: u16,
    },
    Dodge {
        t: u32,
        attacker: Side,
        ability: String,
    },
    /// `side` = the one knocked down.
    Knockdown {
        t: u32,
        side: Side,
        duration_ms: u16,
    },
    LowHp {
        t: u32,
        side: Side,
        hp: u16,
    },
    Moment {
        t: u32,
        kind: MomentKind,
        side: Side,
    },
    /// `side` = the one knocked out.
    Ko {
        t: u32,
        side: Side,
    },
    TimeUp {
        t: u32,
    },
    End {
        t: u32,
        winner: Option<Side>,
        reason: EndReason,
    },
}

impl FightEvent {
    /// Time of the event in ms since the start of the fight.
    #[must_use]
    pub const fn t(&self) -> u32 {
        match self {
            Self::Intro { t }
            | Self::Attack { t, .. }
            | Self::Hit { t, .. }
            | Self::Block { t, .. }
            | Self::Dodge { t, .. }
            | Self::Knockdown { t, .. }
            | Self::LowHp { t, .. }
            | Self::Moment { t, .. }
            | Self::Ko { t, .. }
            | Self::TimeUp { t }
            | Self::End { t, .. } => *t,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(value: &str) -> String {
        value.to_owned()
    }

    fn assert_shape<T>(value: &T, json: &str)
    where
        T: Serialize + for<'de> Deserialize<'de> + PartialEq + std::fmt::Debug,
    {
        assert_eq!(serde_json::to_string(value).unwrap(), json);
        assert_eq!(&serde_json::from_str::<T>(json).unwrap(), value);
    }

    #[test]
    fn enums_have_the_exact_shape() {
        assert_shape(&Side::A, r#""a""#);
        assert_shape(&Side::B, r#""b""#);
        assert_shape(&AbilityKind::Strike, r#""strike""#);
        assert_shape(&AbilityKind::Heavy, r#""heavy""#);
        assert_shape(&AbilityKind::Special, r#""special""#);
        assert_shape(&MomentKind::Comeback, r#""comeback""#);
        assert_shape(&MomentKind::DecisiveCrit, r#""decisive_crit""#);
        assert_shape(&MomentKind::ClutchKo, r#""clutch_ko""#);
        assert_shape(&EndReason::Ko, r#""ko""#);
        assert_shape(&EndReason::Decision, r#""decision""#);
        assert_shape(&EndReason::Draw, r#""draw""#);
    }

    #[test]
    fn every_event_has_the_exact_shape() {
        let cases = [
            (FightEvent::Intro { t: 0 }, r#"{"type":"intro","t":0}"#),
            (
                FightEvent::Attack {
                    t: 3500,
                    attacker: Side::A,
                    ability: s("strike"),
                    kind: AbilityKind::Strike,
                },
                r#"{"type":"attack","t":3500,"attacker":"a","ability":"strike","kind":"strike"}"#,
            ),
            (
                FightEvent::Hit {
                    t: 3900,
                    attacker: Side::A,
                    ability: s("strike"),
                    damage: 22,
                    crit: false,
                    weak: true,
                    hp: 718,
                },
                r#"{"type":"hit","t":3900,"attacker":"a","ability":"strike","damage":22,"crit":false,"weak":true,"hp":718}"#,
            ),
            (
                FightEvent::Block {
                    t: 51900,
                    attacker: Side::B,
                    ability: s("rising_fist"),
                    damage: 14,
                    hp: 156,
                },
                r#"{"type":"block","t":51900,"attacker":"b","ability":"rising_fist","damage":14,"hp":156}"#,
            ),
            (
                FightEvent::Dodge {
                    t: 5300,
                    attacker: Side::B,
                    ability: s("spirit_wave"),
                },
                r#"{"type":"dodge","t":5300,"attacker":"b","ability":"spirit_wave"}"#,
            ),
            (
                FightEvent::Knockdown {
                    t: 6800,
                    side: Side::B,
                    duration_ms: 1500,
                },
                r#"{"type":"knockdown","t":6800,"side":"b","duration_ms":1500}"#,
            ),
            (
                FightEvent::LowHp {
                    t: 40200,
                    side: Side::A,
                    hp: 170,
                },
                r#"{"type":"low_hp","t":40200,"side":"a","hp":170}"#,
            ),
            (
                FightEvent::Moment {
                    t: 58300,
                    kind: MomentKind::Comeback,
                    side: Side::A,
                },
                r#"{"type":"moment","t":58300,"kind":"comeback","side":"a"}"#,
            ),
            (
                FightEvent::Ko {
                    t: 64200,
                    side: Side::B,
                },
                r#"{"type":"ko","t":64200,"side":"b"}"#,
            ),
            (
                FightEvent::TimeUp { t: 86000 },
                r#"{"type":"time_up","t":86000}"#,
            ),
            (
                FightEvent::End {
                    t: 68200,
                    winner: Some(Side::A),
                    reason: EndReason::Ko,
                },
                r#"{"type":"end","t":68200,"winner":"a","reason":"ko"}"#,
            ),
            (
                FightEvent::End {
                    t: 90000,
                    winner: None,
                    reason: EndReason::Draw,
                },
                r#"{"type":"end","t":90000,"winner":null,"reason":"draw"}"#,
            ),
        ];
        for (event, json) in &cases {
            assert_shape(event, json);
        }
    }

    fn sample_log(events: Vec<FightEvent>) -> FightLog {
        FightLog {
            ruleset_version: 1,
            a: FighterInfo {
                slug: s("rata-electrica"),
                max_hp: 720,
            },
            b: FighterInfo {
                slug: s("guerrero-erizado"),
                max_hp: 740,
            },
            events,
        }
    }

    #[test]
    fn fight_log_has_the_exact_shape() {
        let log = sample_log(vec![
            FightEvent::Intro { t: 0 },
            FightEvent::TimeUp { t: 86000 },
            FightEvent::End {
                t: 90000,
                winner: Some(Side::B),
                reason: EndReason::Decision,
            },
        ]);
        assert_shape(
            &log,
            concat!(
                r#"{"ruleset_version":1,"a":{"slug":"rata-electrica","max_hp":720},"#,
                r#""b":{"slug":"guerrero-erizado","max_hp":740},"events":["#,
                r#"{"type":"intro","t":0},{"type":"time_up","t":86000},"#,
                r#"{"type":"end","t":90000,"winner":"b","reason":"decision"}]}"#
            ),
        );
    }

    #[test]
    fn outcome_reads_the_end_event() {
        let log = sample_log(vec![
            FightEvent::Intro { t: 0 },
            FightEvent::End {
                t: 90000,
                winner: None,
                reason: EndReason::Draw,
            },
        ]);
        assert_eq!(log.outcome(), Some((None, EndReason::Draw)));
        assert_eq!(sample_log(vec![FightEvent::Intro { t: 0 }]).outcome(), None);
    }

    #[test]
    fn event_time_is_read_from_every_variant() {
        assert_eq!(FightEvent::TimeUp { t: 7 }.t(), 7);
        assert_eq!(
            FightEvent::Ko {
                t: 9,
                side: Side::A
            }
            .t(),
            9
        );
        assert_eq!(Side::A.opponent(), Side::B);
        assert_eq!(Side::B.opponent(), Side::A);
    }

    #[test]
    fn invalid_events_do_not_deserialize() {
        for json in [
            r#"{"type":"taunt","t":0}"#,
            r#"{"type":"intro"}"#,
            r#"{"t":0}"#,
            r#"{"type":"ko","t":1,"side":"c"}"#,
            r#"{"type":"hit","t":1,"attacker":"a","ability":"strike","damage":1,"crit":false,"hp":1}"#,
            r#"{"type":"end","t":1,"reason":"ko","winner":"x"}"#,
            r#"{"type":"intro","t":-1}"#,
            r#"{"type":"intro","t":1.5}"#,
        ] {
            assert!(
                serde_json::from_str::<FightEvent>(json).is_err(),
                "{json} should not deserialize"
            );
        }
        assert!(serde_json::from_str::<FighterInfo>(r#"{"slug":"x"}"#).is_err());
        assert!(serde_json::from_str::<FightLog>(r#"{"ruleset_version":1}"#).is_err());
    }
}
