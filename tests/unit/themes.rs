use crate::config::{Appearance, Config};

use super::*;

#[test]
fn config_preserves_existing_and_dwm_theme_keys() {
    let keys = [
        "dark",
        "light",
        "terminal",
        "nord",
        "dracula",
        "gruvbox",
        "catppuccin",
        "tokyonight",
        "onedark",
        "solarized",
        "rosepine",
        "everforest",
        "monochrome",
        "catppuccin-latte",
        "gruvbox-light",
        "solarized-light",
        "rosepine-dawn",
        "tokyonight-day",
    ];
    assert_eq!(Theme::ALL.len(), keys.len());
    for (&theme, key) in Theme::ALL.iter().zip(keys) {
        let config = Config {
            appearance: Appearance {
                theme,
                ..Appearance::default()
            },
            ..Config::default()
        };
        let file = toml::to_string(&config).unwrap();
        assert!(file.contains(&format!("theme = \"{key}\"")), "{file}");
        assert_eq!(toml::from_str::<Config>(&file).unwrap(), config);
    }
}

#[test]
fn rgb_themes_keep_text_readable_in_normal_and_selected_rows() {
    for &theme in Theme::ALL.iter().filter(|&&theme| theme != Theme::Terminal) {
        let (fg, bg, muted, accent, selection) = theme.colors();
        assert_ne!(bg, selection, "{} selection", theme.label());
        for (text, background) in [(fg, bg), (muted, bg), (accent, bg), (accent, selection)] {
            let text = luminance(text);
            let background = luminance(background);
            let ratio = (text.max(background) + 0.05) / (text.min(background) + 0.05);
            assert!(
                ratio >= 4.5,
                "{} contrast ratio is {:.2}",
                theme.label(),
                ratio
            );
        }
    }
}

fn luminance(color: Color) -> f64 {
    let Color::Rgb(red, green, blue) = color else {
        panic!("Expected an RGB theme color, got {color}");
    };
    let [red, green, blue] = [red, green, blue].map(|channel| {
        let channel = f64::from(channel) / 255.0;
        if channel <= 0.04045 {
            channel / 12.92
        } else {
            ((channel + 0.055) / 1.055).powf(2.4)
        }
    });
    0.2126 * red + 0.7152 * green + 0.0722 * blue
}
