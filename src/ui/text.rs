//! Laying out text, shared by the views: truncating and aligning it, the selection
//! marker, and section headings.

use ratatui::text::{Line, Span};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// Drawn before the selected row of a list (in the accent colour). Other rows are
/// indented by the same width.
pub const SELECTION_MARKER: &str = "▎ ";
/// Columns taken by `SELECTION_MARKER`.
pub const SELECTION_MARKER_WIDTH: usize = 2;

/// A section heading's text: the title in capitals, filled to `width` columns with a
/// dotted rule, e.g. "BALANCE ┄┄┄┄".
pub fn heading(title: &str, width: usize) -> String {
    let title = format!("{} ", title.to_uppercase());
    format!("{title:┄<width$}")
}

/// `left` with `right` right-aligned on the same line of `width` columns, or `right`
/// on a line of its own (left-aligned) if they don't both fit with a space between.
pub fn right_aligned(
    left: Vec<Span<'static>>,
    right: Vec<Span<'static>>,
    width: usize,
) -> Vec<Line<'static>> {
    match one_line(left, right, width) {
        Ok(line) => vec![line],
        Err((left, right)) => vec![Line::from(left), Line::from(right)],
    }
}

/// `left` with `right` right-aligned on the same line of `width` columns, or `left`
/// alone if they don't both fit with a space between: for a right part that can be
/// left out.
pub fn right_aligned_if_room(
    left: Vec<Span<'static>>,
    right: Vec<Span<'static>>,
    width: usize,
) -> Line<'static> {
    one_line(left, right, width).unwrap_or_else(|(left, _)| Line::from(left))
}

/// The parts of a line that don't fit on it: `(left, right)`.
type Unfitted = (Vec<Span<'static>>, Vec<Span<'static>>);

/// `left` with `right` right-aligned on a line of `width` columns, or both back if they
/// don't fit with a space between.
fn one_line(
    mut left: Vec<Span<'static>>,
    right: Vec<Span<'static>>,
    width: usize,
) -> Result<Line<'static>, Unfitted> {
    let span_width = |spans: &[Span]| -> usize { spans.iter().map(|s| s.content.width()).sum() };
    let (left_width, right_width) = (span_width(&left), span_width(&right));
    if left_width + 1 + right_width > width {
        return Err((left, right));
    }
    left.push(Span::raw(" ".repeat(width - left_width - right_width)));
    left.extend(right);
    Ok(Line::from(left))
}

/// Truncate `s` to at most `max_width` terminal columns, appending `…` if anything was cut.
pub fn truncate_to_width(s: &str, max_width: usize) -> String {
    if s.width() <= max_width {
        return s.to_string();
    }
    if max_width == 0 {
        return String::new();
    }
    let budget = max_width - 1; // reserve one column for the ellipsis
    let mut out = String::new();
    let mut width = 0;
    // Measure whole graphemes, as ratatui does: an emoji followed by a variation
    // selector is wider than its characters' widths added up, and mustn't be split.
    for grapheme in s.graphemes(true) {
        let w = grapheme.width();
        if width + w > budget {
            break;
        }
        out.push_str(grapheme);
        width += w;
    }
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headings_fill_the_width() {
        assert_eq!(heading("When", 10), "WHEN ┄┄┄┄┄");
        assert_eq!(heading("Details", 4), "DETAILS ");
    }

    #[test]
    fn without_room_the_right_part_wraps_or_is_left_out() {
        let spans = |s: &'static str| vec![Span::raw(s)];
        let text = |line: &Line| -> String { line.spans.iter().map(|s| &*s.content).collect() };
        let lines = |lines: Vec<Line>| -> Vec<String> { lines.iter().map(text).collect() };
        assert_eq!(
            lines(right_aligned(spans("ab"), spans("cd"), 7)),
            ["ab   cd"]
        );
        assert_eq!(lines(right_aligned(spans("ab"), spans("cd"), 5)), ["ab cd"]);
        assert_eq!(
            lines(right_aligned(spans("ab"), spans("cd"), 4)),
            ["ab", "cd"]
        );
        let if_room = |width| text(&right_aligned_if_room(spans("ab"), spans("cd"), width));
        assert_eq!(if_room(5), "ab cd");
        assert_eq!(if_room(4), "ab");
    }

    #[test]
    fn short_strings_are_unchanged() {
        assert_eq!(truncate_to_width("Café", 10), "Café");
        assert_eq!(truncate_to_width("Café", 4), "Café");
    }

    #[test]
    fn truncation_fits_width_exactly() {
        let s = truncate_to_width("Woolworths Metro", 8);
        assert_eq!(s, "Woolwor…");
        assert_eq!(s.width(), 8);
    }

    #[test]
    fn multibyte_and_wide_characters_are_measured_in_columns() {
        assert_eq!(truncate_to_width("Café Allongé", 6), "Café …");
        // Each CJK character is 2 columns wide; a split one is dropped, not cut in half.
        let s = truncate_to_width("寿司寿司", 6);
        assert_eq!(s, "寿司…");
        assert!(s.width() <= 6);
    }

    #[test]
    fn emoji_with_a_selector_are_measured_whole() {
        // "🏖\u{FE0F}" is 2 columns, though its characters are 1 and 0 columns wide.
        let s = truncate_to_width("🏖\u{FE0F}🏖\u{FE0F} Beach", 4);
        assert_eq!(s, "🏖\u{FE0F}…");
        assert_eq!(s.width(), 3);
        assert_eq!(truncate_to_width("a🏖\u{FE0F}bc", 3), "a…");
    }

    #[test]
    fn zero_width_yields_empty() {
        assert_eq!(truncate_to_width("abc", 0), "");
    }
}
