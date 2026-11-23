//! Keeping secondary text readable. Themes copy their editor palettes faithfully, so
//! their dim text (an editor's comment colour) is often meant to be barely visible on
//! the base background, and on our lighter backgrounds (bars, panels, selections) it
//! can all but disappear (Nord's `TEXT_DIM` on its selection has a contrast of 1.2).

use opaline::{OpalineColor, Theme, names::tokens};
use ratatui::style::Color;

/// The contrast that secondary text must have with its background: WCAG's minimum for
/// large text. Dim text is meant to recede, and 4.5 (for body text) would make it as
/// bright as normal text in many themes.
const MIN_CONTRAST: f64 = 3.0;

/// The colour for text in `token` (e.g. `TEXT_DIM`) drawn on `bg` (a background
/// token): the token's colour if it is readable there, or else blended toward
/// `TEXT_PRIMARY` just enough to be, so it stays as faint as the theme intends where
/// it can. If no blend is readable, the one with the most contrast.
pub fn readable(theme: &Theme, token: &str, bg: &str) -> Color {
    const STEPS: u8 = 20;
    let bg = theme.color(bg);
    let from = theme.color(token);
    let to = theme.color(tokens::TEXT_PRIMARY);
    let blends = (0..=STEPS).map(|step| from.lerp(to, f32::from(step) / f32::from(STEPS)));
    let contrast = |color: &OpalineColor| contrast_ratio(*color, bg);
    blends
        .clone()
        .find(|color| contrast(color) >= MIN_CONTRAST)
        .or_else(|| blends.max_by(|a, b| contrast(a).total_cmp(&contrast(b))))
        .unwrap_or(from)
        .into()
}

/// The WCAG contrast ratio between two colours, from 1 (the same) to 21.
fn contrast_ratio(a: OpalineColor, b: OpalineColor) -> f64 {
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

/// The WCAG relative luminance of a colour.
fn luminance(color: OpalineColor) -> f64 {
    let channel = |value: u8| {
        let value = f64::from(value) / 255.0;
        if value <= 0.039_28 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(color.r) + 0.7152 * channel(color.g) + 0.0722 * channel(color.b)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rgb(color: Color) -> OpalineColor {
        match color {
            Color::Rgb(r, g, b) => OpalineColor::new(r, g, b),
            other => panic!("not an RGB colour: {other:?}"),
        }
    }

    #[test]
    fn contrast_ratio_matches_wcag() {
        let black = OpalineColor::new(0, 0, 0);
        let white = OpalineColor::new(255, 255, 255);
        assert!((contrast_ratio(black, white) - 21.0).abs() < 0.01);
        assert!((contrast_ratio(white, white) - 1.0).abs() < 0.01);
    }

    #[test]
    fn dim_text_is_readable_on_every_background_in_every_builtin_theme() {
        let backgrounds = [
            tokens::BG_BASE,
            tokens::BG_PANEL,
            tokens::BG_HIGHLIGHT,
            tokens::BG_ELEVATED,
            tokens::BG_SELECTION,
        ];
        for info in opaline::list_available_themes() {
            let theme = opaline::load_by_name(&info.name).unwrap();
            let primary = theme.color(tokens::TEXT_PRIMARY);
            for bg in backgrounds {
                for token in [tokens::TEXT_DIM, tokens::TEXT_MUTED] {
                    // As readable as the theme allows: at least the minimum, or as
                    // much as the better of the token and normal text.
                    let best = contrast_ratio(primary, theme.color(bg))
                        .max(contrast_ratio(theme.color(token), theme.color(bg)))
                        .min(MIN_CONTRAST);
                    let color = rgb(readable(&theme, token, bg));
                    assert!(
                        contrast_ratio(color, theme.color(bg)) >= best - 0.01,
                        "{}: {token} on {bg}",
                        info.name
                    );
                }
            }
        }
    }

    #[test]
    fn readable_text_is_left_alone() {
        for info in opaline::list_available_themes() {
            let theme = opaline::load_by_name(&info.name).unwrap();
            let dim = theme.color(tokens::TEXT_DIM);
            if contrast_ratio(dim, theme.color(tokens::BG_BASE)) >= MIN_CONTRAST {
                assert_eq!(
                    readable(&theme, tokens::TEXT_DIM, tokens::BG_BASE),
                    dim.into()
                );
            }
        }
    }

    #[test]
    fn nord_dim_text_on_the_selection_is_brightened() {
        let theme = opaline::load_by_name("nord").unwrap();
        let dim = theme.color(tokens::TEXT_DIM);
        let selection = theme.color(tokens::BG_SELECTION);
        assert!(contrast_ratio(dim, selection) < 1.5);
        let color = rgb(readable(&theme, tokens::TEXT_DIM, tokens::BG_SELECTION));
        let ratio = contrast_ratio(color, selection);
        // Just enough: not as bright as normal text.
        assert!(
            (MIN_CONTRAST..MIN_CONTRAST + 0.5).contains(&ratio),
            "{ratio}"
        );
    }
}
