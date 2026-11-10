//! The player botarga (021, phase 1): what a user reads and writes of their own fighter.
//! Contract: no human-readable text, only the ids of `content/catalog` and clip codes; the
//! botarga is named by the user's `display_name`, so nothing here carries a name or a user id.
use serde::{Deserialize, Serialize};

/// The stat points a player spent, before the accessories (016).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(deny_unknown_fields)]
pub struct BotargaStats {
    pub hp: u8,
    pub attack: u8,
    pub defense: u8,
    pub speed: u8,
    pub special: u8,
}

/// The idle clips a player can pick (016): the codes of the renderer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum IdleClip {
    #[serde(rename = "idle")]
    Idle,
    #[serde(rename = "idle_2")]
    Idle2,
    #[serde(rename = "idle_3")]
    Idle3,
    #[serde(rename = "idle_4")]
    Idle4,
    #[serde(rename = "idle_5")]
    Idle5,
}

impl IdleClip {
    pub const ALL: &'static [Self] = &[
        Self::Idle,
        Self::Idle2,
        Self::Idle3,
        Self::Idle4,
        Self::Idle5,
    ];

    /// The serialized code (the same string serde writes).
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Idle2 => "idle_2",
            Self::Idle3 => "idle_3",
            Self::Idle4 => "idle_4",
            Self::Idle5 => "idle_5",
        }
    }

    pub fn parse(code: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|clip| clip.as_str() == code)
    }
}

/// The victory clips a player can pick (016): the codes of the renderer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum VictoryClip {
    #[serde(rename = "win_1")]
    Win1,
    #[serde(rename = "win_2")]
    Win2,
    #[serde(rename = "win_3")]
    Win3,
}

impl VictoryClip {
    pub const ALL: &'static [Self] = &[Self::Win1, Self::Win2, Self::Win3];

    /// The serialized code (the same string serde writes).
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Win1 => "win_1",
            Self::Win2 => "win_2",
            Self::Win3 => "win_3",
        }
    }

    pub fn parse(code: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|clip| clip.as_str() == code)
    }
}

/// What a botarga picked to look like (016): its idle and its victory. `null`: the variant of
/// the tournament. Never part of a `CharacterSpec` nor of what a fight hashes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(deny_unknown_fields)]
pub struct FighterCosmetics {
    pub idle: Option<IdleClip>,
    pub victory: Option<VictoryClip>,
}

/// What a botarga has equipped, and the body of `PUT /api/botarga/build`: ids of the catalog the
/// user has unlocked. The server checks the sizes, the unlocks and the rules of the loadout.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(deny_unknown_fields)]
pub struct BotargaBuild {
    pub stats: BotargaStats,
    pub style_id: String,
    pub abilities: Vec<String>,
    pub accessories: Vec<String>,
    pub cosmetics: FighterCosmetics,
}

/// The attacks of today and what the next one pays (021 "Escalones del día").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct BotargaDaily {
    pub attacks: u16,
    pub factor_pct: u8,
}

/// Where the botarga stands: `budget_free` are the stat points of the level not spent yet, and
/// `xp_to_next` is 0 at the last level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct BotargaProgress {
    pub level: u8,
    pub xp: u32,
    pub xp_to_next: u32,
    pub budget_free: u16,
    pub slots_open: u8,
    pub daily: BotargaDaily,
}

/// What the user owns, by kind: ids of the catalog, sorted.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct BotargaUnlocks {
    pub abilities: Vec<String>,
    pub accessories: Vec<String>,
    pub cosmetics: Vec<String>,
}

/// `GET /api/botarga`, and the answer of `POST /api/botarga` and `PUT /api/botarga/build`: the
/// botarga of the session's user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct BotargaView {
    pub build: BotargaBuild,
    pub progress: BotargaProgress,
    pub unlocks: BotargaUnlocks,
    pub body_type: String,
    pub face_id: String,
    pub rating: u16,
    pub rated_fights: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build() -> BotargaBuild {
        BotargaBuild {
            stats: BotargaStats {
                hp: 40,
                attack: 40,
                defense: 40,
                speed: 40,
                special: 40,
            },
            style_id: "balanced".to_owned(),
            abilities: vec!["foam_headbutt".to_owned()],
            accessories: Vec::new(),
            cosmetics: FighterCosmetics {
                idle: Some(IdleClip::Idle2),
                victory: None,
            },
        }
    }

    const BUILD_JSON: &str = concat!(
        r#"{"stats":{"hp":40,"attack":40,"defense":40,"speed":40,"special":40},"#,
        r#""style_id":"balanced","abilities":["foam_headbutt"],"accessories":[],"#,
        r#""cosmetics":{"idle":"idle_2","victory":null}}"#,
    );

    #[test]
    fn a_view_has_the_exact_shape_and_no_user_id() {
        let view = BotargaView {
            build: build(),
            progress: BotargaProgress {
                level: 1,
                xp: 0,
                xp_to_next: 120,
                budget_free: 0,
                slots_open: 1,
                daily: BotargaDaily {
                    attacks: 0,
                    factor_pct: 100,
                },
            },
            unlocks: BotargaUnlocks {
                abilities: vec!["foam_headbutt".to_owned()],
                accessories: Vec::new(),
                cosmetics: Vec::new(),
            },
            body_type: "normal".to_owned(),
            face_id: "rookie".to_owned(),
            rating: 1000,
            rated_fights: 0,
        };
        let json = format!(
            concat!(
                r#"{{"build":{},"progress":{{"level":1,"xp":0,"xp_to_next":120,"budget_free":0,"#,
                r#""slots_open":1,"daily":{{"attacks":0,"factor_pct":100}}}},"#,
                r#""unlocks":{{"abilities":["foam_headbutt"],"accessories":[],"cosmetics":[]}},"#,
                r#""body_type":"normal","face_id":"rookie","rating":1000,"rated_fights":0}}"#,
            ),
            BUILD_JSON
        );
        assert_eq!(serde_json::to_string(&view).unwrap(), json);
        assert_eq!(serde_json::from_str::<BotargaView>(&json).unwrap(), view);
        assert!(!json.contains("user"));
    }

    #[test]
    fn a_build_refuses_what_it_does_not_know() {
        assert_eq!(
            serde_json::from_str::<BotargaBuild>(BUILD_JSON).unwrap(),
            build()
        );
        for bad in [
            BUILD_JSON.replace(r#""accessories":[],"#, r#""accessories":[],"level":100,"#),
            BUILD_JSON.replace(r#""hp":40,"#, r#""hp":40,"luck":1,"#),
            BUILD_JSON.replace(r#""hp":40,"#, r#""hp":256,"#),
            BUILD_JSON.replace(r#""hp":40,"#, r#""hp":-1,"#),
            BUILD_JSON.replace("idle_2", "idle_6"),
            BUILD_JSON.replace(r#""victory":null"#, r#""victory":"ko_1""#),
            BUILD_JSON.replace(r#""victory":null"#, r#""victory":null,"ko":"ko_1""#),
            BUILD_JSON.replace(r#""style_id":"balanced","#, ""),
        ] {
            assert!(serde_json::from_str::<BotargaBuild>(&bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn clip_codes_match_serde_and_parse_back() {
        for clip in IdleClip::ALL {
            assert_eq!(
                serde_json::to_value(clip).unwrap(),
                serde_json::Value::from(clip.as_str())
            );
            assert_eq!(IdleClip::parse(clip.as_str()), Some(*clip));
        }
        for clip in VictoryClip::ALL {
            assert_eq!(
                serde_json::to_value(clip).unwrap(),
                serde_json::Value::from(clip.as_str())
            );
            assert_eq!(VictoryClip::parse(clip.as_str()), Some(*clip));
        }
        assert_eq!(IdleClip::ALL.len(), 5);
        assert_eq!(VictoryClip::ALL.len(), 3);
        assert_eq!(IdleClip::parse("win_1"), None);
        assert_eq!(VictoryClip::parse("idle"), None);
    }
}
