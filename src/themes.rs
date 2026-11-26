use ratatui::style::Color;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Theme {
    #[default]
    Dark,
    Light,
    Terminal,
    Nord,
    Dracula,
    #[serde(rename = "gruvbox")]
    GruvboxDark,
    #[serde(rename = "catppuccin")]
    CatppuccinMocha,
    #[serde(rename = "tokyonight")]
    TokyoNight,
    #[serde(rename = "onedark")]
    OneDark,
    #[serde(rename = "solarized")]
    SolarizedDark,
    #[serde(rename = "rosepine")]
    RosePine,
    Everforest,
    Monochrome,
    CatppuccinLatte,
    GruvboxLight,
    SolarizedLight,
    #[serde(rename = "rosepine-dawn")]
    RosePineDawn,
    #[serde(rename = "tokyonight-day")]
    TokyoNightDay,
}

impl Theme {
    pub const ALL: &'static [Self] = &[
        Self::Dark,
        Self::Light,
        Self::Terminal,
        Self::Nord,
        Self::Dracula,
        Self::GruvboxDark,
        Self::CatppuccinMocha,
        Self::TokyoNight,
        Self::OneDark,
        Self::SolarizedDark,
        Self::RosePine,
        Self::Everforest,
        Self::Monochrome,
        Self::CatppuccinLatte,
        Self::GruvboxLight,
        Self::SolarizedLight,
        Self::RosePineDawn,
        Self::TokyoNightDay,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Dark => "Dark",
            Self::Light => "Light",
            Self::Terminal => "Terminal",
            Self::Nord => "Nord",
            Self::Dracula => "Dracula",
            Self::GruvboxDark => "Gruvbox Dark",
            Self::CatppuccinMocha => "Catppuccin Mocha",
            Self::TokyoNight => "Tokyo Night",
            Self::OneDark => "One Dark",
            Self::SolarizedDark => "Solarized Dark",
            Self::RosePine => "Rosé Pine",
            Self::Everforest => "Everforest",
            Self::Monochrome => "Monochrome",
            Self::CatppuccinLatte => "Catppuccin Latte",
            Self::GruvboxLight => "Gruvbox Light",
            Self::SolarizedLight => "Solarized Light",
            Self::RosePineDawn => "Rosé Pine Dawn",
            Self::TokyoNightDay => "Tokyo Night Day",
        }
    }

    // Foreground, background, muted text, accent and selection background.
    pub fn colors(self) -> (Color, Color, Color, Color, Color) {
        match self {
            Self::Dark => palette(0xE6EDF3, 0x121820, 0x8E9FB0, 0x72DED5, 0x253B44),
            Self::Light => palette(0x192632, 0xF6F9FC, 0x506274, 0x00666F, 0xD2E9EC),
            Self::Terminal => (
                Color::Reset,
                Color::Reset,
                Color::Reset,
                Color::Cyan,
                Color::Reset,
            ),
            // DWM Titus palettes with contrast adjusted for secondary text and selection.
            Self::Nord => palette(0xD8DEE9, 0x2E3440, 0xA3ADC2, 0x88C0D0, 0x3B4252),
            Self::Dracula => palette(0xF8F8F2, 0x282A36, 0xA0A8C8, 0xD6ACFF, 0x44475A),
            Self::GruvboxDark => palette(0xEBDBB2, 0x282828, 0xA89984, 0xFABD2F, 0x504945),
            Self::CatppuccinMocha => palette(0xCDD6F4, 0x1E1E2E, 0xA6ADC8, 0xCBA6F7, 0x313244),
            Self::TokyoNight => palette(0xC0CAF5, 0x1A1B26, 0xA9B1D6, 0x7AA2F7, 0x24283B),
            Self::OneDark => palette(0xABB2BF, 0x282C34, 0x9AA4B5, 0x70BBFF, 0x3E4451),
            Self::SolarizedDark => palette(0x93A1A1, 0x002B36, 0x839496, 0x43AC9E, 0x073642),
            Self::RosePine => palette(0xE0DEF4, 0x191724, 0x908CAA, 0xC4A7E7, 0x26233A),
            Self::Everforest => palette(0xD3C6AA, 0x2D353B, 0xADA184, 0xA7C080, 0x3D4841),
            Self::Monochrome => palette(0xCCCCCC, 0x1A1A1A, 0xAAAAAA, 0xFFFFFF, 0x333333),
            Self::CatppuccinLatte => palette(0x4C4F69, 0xEFF1F5, 0x696C81, 0x203D8E, 0xBFC3D0),
            Self::GruvboxLight => palette(0x3C3836, 0xFBF1C7, 0x74675C, 0x573A0A, 0xBDAE93),
            Self::SolarizedLight => palette(0x49636D, 0xFDF6E3, 0x5D7178, 0x005A92, 0xE0DAC6),
            Self::RosePineDawn => palette(0x575279, 0xFAF4ED, 0x6E6986, 0x8A3C54, 0xDFDAD9),
            Self::TokyoNightDay => palette(0x2E3354, 0xE1E2E7, 0x4F5E95, 0x2E53AA, 0xD0D0D5),
        }
    }
}

fn palette(
    fg: u32,
    bg: u32,
    muted: u32,
    accent: u32,
    selection: u32,
) -> (Color, Color, Color, Color, Color) {
    let [fg, bg, muted, accent, selection] = [fg, bg, muted, accent, selection].map(|hex| {
        let [_, red, green, blue] = hex.to_be_bytes();
        Color::Rgb(red, green, blue)
    });
    (fg, bg, muted, accent, selection)
}

#[cfg(test)]
mod tests {
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
}
