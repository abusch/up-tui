//! A terminal backend wrapper that works around a ratatui drawing bug with emoji.

use std::io::Stdout;

use ratatui::Terminal;
use ratatui::backend::{Backend, ClearType, CrosstermBackend, WindowSize};
use ratatui::buffer::{Cell, CellWidth};
use ratatui::layout::{Position, Size};

/// The terminal the app draws on.
pub type AppTerminal = Terminal<WideCharBackend<CrosstermBackend<Stdout>>>;

/// Set up the terminal like `ratatui::init()` (raw mode, alternate screen, and a panic
/// hook that restores the terminal), with the backend wrapped in `WideCharBackend`.
/// Restore it with `ratatui::restore()`.
pub fn init() -> std::io::Result<AppTerminal> {
    // `ratatui::init()` does the setup; only its terminal's backend is replaced.
    drop(ratatui::init());
    let backend = WideCharBackend(CrosstermBackend::new(std::io::stdout()));
    Terminal::new(backend)
}

/// Wraps a backend to drop updates to cells covered by a wide character drawn just
/// before them.
///
/// When ratatui draws an emoji presentation sequence (one containing U+FE0F, e.g.
/// "🛋️") over cells that weren't blank, its diff also emits the cell under the
/// emoji's second column, to clear it on terminals that draw such emoji one column
/// wide. The crossterm backend writes that cell right after the emoji without moving
/// the cursor, since it's the next column. But terminals that draw the emoji two
/// columns wide have already moved the cursor past that column, so the blank, and every
/// cell written after it without a cursor move, lands one column too far right.
///
/// The emoji already covers that cell, so the update isn't needed; dropping it makes
/// the backend move the cursor to the next cell it draws.
///
/// This is ratatui issue #2651 (<https://github.com/ratatui/ratatui/issues/2651>),
/// fixed by PR #2721 but not released as of ratatui-crossterm 0.1.2. Remove this
/// wrapper (and use `ratatui::init()` and `DefaultTerminal` again) once a
/// ratatui-crossterm release includes that fix.
pub struct WideCharBackend<B>(pub B);

/// Drop cells that are covered by the wide character drawn just before them.
fn skip_covered_cells<'a>(
    content: impl Iterator<Item = (u16, u16, &'a Cell)>,
) -> impl Iterator<Item = (u16, u16, &'a Cell)> {
    // The row and the end column (exclusive) covered by the last wide character drawn.
    let mut covered: Option<(u16, u16)> = None;
    content.filter(move |&(x, y, cell)| {
        if covered.is_some_and(|(row, end)| y == row && x < end) {
            return false;
        }
        let width = cell.cell_width();
        covered = (width > 1).then(|| (y, x.saturating_add(width)));
        true
    })
}

impl<B: Backend> Backend for WideCharBackend<B> {
    type Error = B::Error;

    fn draw<'a, I>(&mut self, content: I) -> Result<(), Self::Error>
    where
        I: Iterator<Item = (u16, u16, &'a Cell)>,
    {
        self.0.draw(skip_covered_cells(content))
    }

    fn append_lines(&mut self, n: u16) -> Result<(), Self::Error> {
        self.0.append_lines(n)
    }

    fn hide_cursor(&mut self) -> Result<(), Self::Error> {
        self.0.hide_cursor()
    }

    fn show_cursor(&mut self) -> Result<(), Self::Error> {
        self.0.show_cursor()
    }

    fn get_cursor_position(&mut self) -> Result<Position, Self::Error> {
        self.0.get_cursor_position()
    }

    fn set_cursor_position<P: Into<Position>>(&mut self, position: P) -> Result<(), Self::Error> {
        self.0.set_cursor_position(position)
    }

    fn clear(&mut self) -> Result<(), Self::Error> {
        self.0.clear()
    }

    fn clear_region(&mut self, clear_type: ClearType) -> Result<(), Self::Error> {
        self.0.clear_region(clear_type)
    }

    fn size(&self) -> Result<Size, Self::Error> {
        self.0.size()
    }

    fn window_size(&mut self) -> Result<WindowSize, Self::Error> {
        self.0.window_size()
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        self.0.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;
    use ratatui::style::Style;

    /// A writer whose output can be read after the backend has taken it.
    #[derive(Clone, Default)]
    struct SharedOutput(std::rc::Rc<std::cell::RefCell<Vec<u8>>>);

    impl std::io::Write for SharedOutput {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.borrow_mut().write(buf)
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    /// The cells ratatui's diff emits for drawing `next` over `prev`, as (x, symbol).
    fn diff(prev: &str, next: &str, filter: bool) -> Vec<(u16, String)> {
        let area = Rect::new(0, 0, 10, 1);
        let (mut before, mut after) = (Buffer::empty(area), Buffer::empty(area));
        before.set_string(0, 0, prev, Style::default());
        after.set_string(0, 0, next, Style::default());
        let cells = before.diff(&after);
        let cells: Vec<_> = if filter {
            skip_covered_cells(cells.into_iter()).collect()
        } else {
            cells
        };
        cells
            .into_iter()
            .map(|(x, _, cell)| (x, cell.symbol().to_string()))
            .collect()
    }

    #[test]
    fn emoji_over_text_does_not_rewrite_the_covered_cell() {
        // ratatui emits the blank under the emoji's second column (x = 1) straight
        // after the emoji: the cause of the bug being worked around.
        let emoji = || (0, "🛋\u{FE0F}".to_string());
        assert_eq!(
            diff("abcd", "🛋\u{FE0F} L", false),
            [emoji(), (1, " ".into()), (2, " ".into()), (3, "L".into())]
        );
        // Without it, the backend moves the cursor to x = 2 before drawing the space.
        assert_eq!(
            diff("abcd", "🛋\u{FE0F} L", true),
            [emoji(), (2, " ".into()), (3, "L".into())]
        );
    }

    #[test]
    fn cursor_is_moved_after_the_emoji() {
        let area = Rect::new(0, 0, 10, 1);
        let (mut before, mut after) = (Buffer::empty(area), Buffer::empty(area));
        before.set_string(0, 0, "abcd", Style::default());
        after.set_string(0, 0, "🛋\u{FE0F} L", Style::default());
        let output = SharedOutput::default();
        let mut backend = WideCharBackend(CrosstermBackend::new(output.clone()));
        backend.draw(before.diff(&after).into_iter()).unwrap();
        let output = String::from_utf8(output.0.borrow().clone()).unwrap();
        // The emoji is followed by a cursor move to row 1, column 3 (1-based), not by
        // the space written in place.
        assert!(
            output.contains("🛋\u{FE0F}\x1b[1;3H "),
            "unexpected output: {output:?}"
        );
    }

    #[test]
    fn other_cells_are_kept() {
        assert_eq!(
            diff("abcdef", "xy寿zw", true),
            [
                (0, "x".into()),
                (1, "y".into()),
                (2, "寿".into()),
                (4, "z".into()),
                (5, "w".into())
            ]
        );
    }
}
