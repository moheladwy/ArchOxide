use super::*;

#[test]
fn lists_unique_primary_names_case_insensitively_and_preserves_spelling() {
    let listing = "Noto Sans 日本語\nJetBrainsMono Nerd Font Mono\nCafé Sans\n\n\
                   café sans\njetbrainsmono nerd font mono\n Alpha, Mono \nZeta\n";
    assert_eq!(
        parse_families(listing),
        [
            "Alpha, Mono",
            "Café Sans",
            "JetBrainsMono Nerd Font Mono",
            "Noto Sans 日本語",
            "Zeta",
        ]
    );
}

#[test]
fn skips_names_that_cannot_be_written_as_single_line_font_overrides() {
    for listing in ["", "   \n\n", "Bad\tFont\nInvalid\u{2028}Family\nBad\0Name"] {
        assert!(parse_families(listing).is_empty());
    }
}
