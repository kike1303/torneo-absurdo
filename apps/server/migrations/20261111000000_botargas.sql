-- The player botarga (feature 021, phase 1; 016 phase 5a). Additive (ADR-0006 D10).
--
-- One botarga per user, created in the login transaction that creates the user. Nothing here walks
-- `users`: a user of before this migration (or one that an old binary creates during a deploy) has
-- no row until `POST /api/botarga` inserts it. As in the earlier migrations, every constraint that
-- can fail has a fixed name (ADR-0012 A9).
--
-- The CHECKs are the second barrier: the server validates a build against the catalog and
-- `engine::loadout::resolve` before it writes. The stats are the points the player spent, before the
-- accessories, so their range is the one of a `u8` and not the one of the ruleset; their sum stays
-- under the budget of the level, the same integer formula as `engine::loadout::budget` (a test
-- compares the two at every level). The ids are codes of `content/catalog`, never text for humans.
CREATE TABLE botargas (
    user_id uuid PRIMARY KEY REFERENCES users,
    level smallint NOT NULL DEFAULT 1 CONSTRAINT botargas_level_ck CHECK (level BETWEEN 1 AND 100),
    xp int NOT NULL DEFAULT 0 CONSTRAINT botargas_xp_ck CHECK (xp >= 0),
    hp smallint NOT NULL,
    attack smallint NOT NULL,
    defense smallint NOT NULL,
    speed smallint NOT NULL,
    special smallint NOT NULL,
    style_id text NOT NULL CONSTRAINT botargas_style_ck CHECK (char_length(style_id) BETWEEN 1 AND 64),
    -- In the order the player equipped them: the engine reads the abilities in order (016).
    abilities text[] NOT NULL,
    accessories text[] NOT NULL DEFAULT '{}',
    -- NULL: the variant of the tournament (016 "Animaciones cosméticas").
    cosmetic_idle text NULL,
    cosmetic_victory text NULL,
    body_type text NOT NULL DEFAULT 'normal',
    face_id text NOT NULL DEFAULT 'rookie',
    rating int NOT NULL DEFAULT 1000 CONSTRAINT botargas_rating_ck CHECK (rating BETWEEN 0 AND 4000),
    rated_fights int NOT NULL DEFAULT 0 CONSTRAINT botargas_rated_fights_ck CHECK (rated_fights >= 0),
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT botargas_stats_ck CHECK (
        hp BETWEEN 0 AND 255 AND attack BETWEEN 0 AND 255 AND defense BETWEEN 0 AND 255
        AND speed BETWEEN 0 AND 255 AND special BETWEEN 0 AND 255
        AND hp + attack + defense + speed + special <= 200 + (level - 1) * 100 / 99
    ),
    CONSTRAINT botargas_abilities_ck CHECK (
        cardinality(abilities) BETWEEN 1 AND 4 AND array_position(abilities, NULL) IS NULL
    ),
    CONSTRAINT botargas_accessories_ck CHECK (
        cardinality(accessories) <= 5 AND array_position(accessories, NULL) IS NULL
    ),
    CONSTRAINT botargas_cosmetics_ck CHECK (
        char_length(cosmetic_idle) BETWEEN 1 AND 64 AND char_length(cosmetic_victory) BETWEEN 1 AND 64
    ),
    CONSTRAINT botargas_look_ck CHECK (
        char_length(body_type) BETWEEN 1 AND 64 AND char_length(face_id) BETWEEN 1 AND 64
    )
);

-- What the player owns: a build only equips ids that have a row here. The base of the inventory of
-- TA-67. `source` says where it came from; this phase only writes `starter`.
CREATE TABLE botarga_unlocks (
    user_id uuid NOT NULL REFERENCES botargas,
    kind text NOT NULL CONSTRAINT botarga_unlocks_kind_ck
        CHECK (kind IN ('ability', 'accessory', 'cosmetic')),
    item_id text NOT NULL CONSTRAINT botarga_unlocks_item_ck
        CHECK (char_length(item_id) BETWEEN 1 AND 64),
    source text NOT NULL CONSTRAINT botarga_unlocks_source_ck
        CHECK (source IN ('starter', 'draw', 'level', 'torneo')),
    unlocked_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT botarga_unlocks_pk PRIMARY KEY (user_id, kind, item_id)
);
