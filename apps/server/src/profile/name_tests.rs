// Tests of `profile::name` (T20 table, 008 RF-B7). Included from name.rs.

use proptest::prelude::*;

use super::*;

fn ok(input: &str) -> DisplayName {
    validate(input).unwrap_or_else(|e| panic!("{input:?}: {e:?}"))
}

#[test]
fn valid_names_are_normalized_and_trimmed() {
    assert_eq!(ok("TigreVeloz42").as_str(), "TigreVeloz42");
    assert_eq!(ok("  Ana  ").as_str(), "Ana");
    // NFC: a decomposed é becomes the composed one.
    assert_eq!(ok("Jose\u{301}").as_str(), "José");
    // 24 graphemes, with combining marks that are not extra graphemes.
    assert!(validate(&"e\u{301}".repeat(24)).is_ok());
}

#[test]
fn the_length_is_3_to_24_graphemes() {
    for input in ["", "ab", "  ab  ", "\u{301}\u{301}"] {
        assert_eq!(validate(input), Err(NameError::Invalid), "{input:?}");
    }
    assert!(validate("abc").is_ok());
    assert!(validate(&"a".repeat(24)).is_ok());
    assert_eq!(validate(&"a".repeat(25)), Err(NameError::Invalid));
    assert_eq!(validate(&"é".repeat(25)), Err(NameError::Invalid));
}

#[test]
fn control_bidi_and_zero_width_are_refused() {
    let mut forbidden: Vec<char> = vec!['\u{0}', '\u{7}', '\u{1f}', '\u{7f}', '\u{9f}', '\n', '\t'];
    forbidden.extend(['\u{061C}', '\u{200E}', '\u{200F}']);
    forbidden.extend('\u{202A}'..='\u{202E}');
    forbidden.extend('\u{2066}'..='\u{2069}');
    forbidden.extend(['\u{200B}', '\u{200C}', '\u{200D}', '\u{2060}', '\u{FEFF}']);
    for c in forbidden {
        let input = format!("Ana{c}Bel");
        assert_eq!(
            validate(&input),
            Err(NameError::Invalid),
            "U+{:04X}",
            u32::from(c)
        );
    }
}

/// security-reviewer on #54: every Default_Ignorable_Code_Point (an invisible name, or a name
/// that looks like another) and U+2800, the braille blank.
#[test]
fn invisible_characters_are_refused() {
    // The cases security confirmed slipped through.
    let confirmed = [
        '\u{2063}',
        '\u{3164}',
        '\u{FFA0}',
        '\u{115F}',
        '\u{034F}',
        '\u{00AD}',
        '\u{180E}',
        '\u{17B4}',
        '\u{FE0F}',
        '\u{E0061}',
        '\u{2800}',
    ];
    // Both ends of every range of the property.
    let table_ends = [
        '\u{00AD}',
        '\u{034F}',
        '\u{061C}',
        '\u{115F}',
        '\u{1160}',
        '\u{17B4}',
        '\u{17B5}',
        '\u{180B}',
        '\u{180F}',
        '\u{200B}',
        '\u{200F}',
        '\u{202A}',
        '\u{202E}',
        '\u{2060}',
        '\u{206F}',
        '\u{3164}',
        '\u{FE00}',
        '\u{FE0F}',
        '\u{FEFF}',
        '\u{FFA0}',
        '\u{FFF0}',
        '\u{FFF8}',
        '\u{1BCA0}',
        '\u{1BCA3}',
        '\u{1D173}',
        '\u{1D17A}',
        '\u{E0000}',
        '\u{E0FFF}',
    ];
    for c in confirmed.iter().chain(&table_ends) {
        assert!(is_invisible(*c), "U+{:04X}", u32::from(*c));
        for input in [
            format!("Ana{c}Bel"),
            format!("{c}{c}{c}"),
            format!("Ana{c}"),
        ] {
            assert_eq!(
                validate(&input),
                Err(NameError::Invalid),
                "U+{:04X}",
                u32::from(*c)
            );
        }
    }
    // Just outside the ranges: ordinary characters.
    for c in [
        '\u{00AE}', '\u{1161}', '\u{180A}', '\u{2070}', '\u{FE10}', '\u{FFF9}', '\u{2801}',
    ] {
        assert!(!is_invisible(c), "U+{:04X}", u32::from(c));
    }
    assert!(validate("Año⠁Ñu").is_ok());
}

#[test]
fn reserved_names_are_refused_by_key() {
    for input in [
        "admin",
        "Admin",
        "ADMIN",
        // Cyrillic а (U+0430) for the Latin a.
        "\u{430}dmin",
        "Moderador",
        "TorneoAbsurdo",
        // Fullwidth letters (NFKC).
        "\u{FF41}\u{FF44}\u{FF4D}\u{FF49}\u{FF4E}",
    ] {
        assert_eq!(validate(input), Err(NameError::Reserved), "{input:?}");
    }
    // A reserved word inside a longer name is fine: only the whole key is compared.
    assert!(validate("AdminDeLaCasa").is_ok());
}

/// game-designer on #54: the brand with a space, a dot, an underscore or a hyphen, the accented
/// forms that survive the skeleton, and the names of authorities a user could impersonate.
#[test]
fn reserved_names_ignore_separators_and_cover_authorities() {
    for input in [
        "Torneo Absurdo",
        "torneo.absurdo",
        "Torneo_Absurdo",
        "Torneo-Absurdo",
        "TORNEO  ABSURDO",
        "Torneo Absurdo Oficial",
        "Anónimo",
        "anónima",
        "Anonima",
        "Árbitro",
        "arbitro",
        "Referee",
        "Juez",
        "Judge",
        "Equipo",
        "Team",
        "Presentador",
        "Host",
        "Usuario Eliminado",
        "Deleted User",
        "deleted_user",
        "a.d.m.i.n",
        // security-reviewer on #54: any punctuation, not only four separators.
        "Torneo·Absurdo",
        "Torneo/Absurdo",
        "Torneo,Absurdo",
        "Torneo'Absurdo",
        "Torneo—Absurdo",
        "Torneo:Absurdo",
        "Torneo*Absurdo",
        "Torneo~Absurdo",
        "Torneo–Absurdo",
        "Torneo\u{3000}Absurdo",
        "¡Torneo Absurdo!",
        "Ano\u{301}nimo",
    ] {
        assert_eq!(validate(input), Err(NameError::Reserved), "{input:?}");
    }
    // The UNIQUE key keeps the separators: two people can still be "Ana Bel" and "AnaBel".
    assert_ne!(ok("Ana Bel").key(), ok("AnaBel").key());
}

#[test]
fn case_and_homoglyphs_give_the_same_key() {
    let latin = ok("Torneo_King");
    // Cyrillic о (U+043E) for the Latin o.
    assert_eq!(ok("T\u{43E}rneo_King").key(), latin.key());
    assert_eq!(ok("TORNEO_KING").key(), latin.key());
    assert_eq!(ok("torneo_king").key(), latin.key());
    assert_ne!(ok("Torneo_Queen").key(), latin.key());
    // The visible name keeps what the user wrote.
    assert_eq!(ok("TORNEO_KING").as_str(), "TORNEO_KING");
}

#[test]
fn the_key_reaches_a_fixed_point_on_a_known_case() {
    // One pass maps `%` to `º/₀`, which a second pass changes again.
    let key = key_of("% 0");
    assert_eq!(key_of(&key), key);
}

#[test]
fn debug_of_a_name_is_the_name_only() {
    // A display name is public data, not a secret, but it never reaches a log (RF-B17):
    // `DisplayName` has no `Display`.
    let name = ok("TigreVeloz42");
    assert!(format!("{name:?}").starts_with("DisplayName"));
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2048))]

    /// `display_name_key` is idempotent over its own output.
    #[test]
    fn the_key_is_idempotent(input in "\\PC{3,24}") {
        let once = key_of(&input);
        prop_assert_eq!(key_of(&once), once);
    }

    /// Every name that passes `validate` has 3 to 24 graphemes.
    #[test]
    fn valid_names_have_3_to_24_graphemes(input in "\\PC{0,30}") {
        if let Ok(name) = validate(&input) {
            let graphemes = unicode_segmentation::UnicodeSegmentation::graphemes(name.as_str(), true).count();
            prop_assert!((3..=24).contains(&graphemes), "{} graphemes", graphemes);
        }
    }
}
