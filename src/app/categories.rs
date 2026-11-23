//! The transaction categories, as fetched once on startup.

use std::collections::HashMap;

use opaline::names::tokens;

use crate::client::models::Category;

/// The colour token for each of Up's root categories; the categories in it have the
/// same colour. The status tokens are used because they're clearly different hues
/// (green, yellow/orange, red and blue/cyan) in every builtin theme.
const ROOT_COLORS: [(&str, &str); 4] = [
    ("good-life", tokens::WARNING),
    ("home", tokens::INFO),
    ("personal", tokens::SUCCESS),
    ("transport", tokens::ERROR),
];

/// The categories by id.
#[derive(Debug, Default)]
pub struct Categories {
    by_id: HashMap<String, Category>,
}

impl Categories {
    pub fn new(categories: impl IntoIterator<Item = Category>) -> Self {
        Categories {
            by_id: categories.into_iter().map(|c| (c.id.clone(), c)).collect(),
        }
    }

    /// The category's display name, or its id if it isn't known (e.g. the categories
    /// haven't loaded yet).
    pub fn name<'a>(&'a self, id: &'a str) -> &'a str {
        self.by_id.get(id).map_or(id, |c| c.name.as_str())
    }

    /// The root category that `id` is in (`id` itself if it's a root category, or if
    /// it isn't known).
    pub fn root<'a>(&'a self, id: &'a str) -> &'a str {
        let mut id = id;
        // Bounded, in case the parents ever form a cycle.
        for _ in 0..=self.by_id.len() {
            match self.by_id.get(id).and_then(|c| c.parent.as_deref()) {
                Some(parent) => id = parent,
                None => break,
            }
        }
        id
    }

    /// The ids of Up's root categories that have their own colour, in a fixed order.
    pub fn coloured_roots() -> impl Iterator<Item = &'static str> {
        ROOT_COLORS.iter().map(|&(id, _)| id)
    }

    /// The colour token for the root category that `id` is in, or `None` if it isn't
    /// one of Up's root categories (or isn't known yet).
    pub fn color_token(&self, id: &str) -> Option<&'static str> {
        let root = self.root(id);
        ROOT_COLORS
            .iter()
            .find(|&&(r, _)| r == root)
            .map(|&(_, token)| token)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A category for tests.
    pub(crate) fn named(id: &str, name: &str, parent: Option<&str>) -> Category {
        Category {
            id: id.to_string(),
            name: name.to_string(),
            parent: parent.map(str::to_string),
        }
    }

    fn category(id: &str, parent: Option<&str>) -> Category {
        named(id, &id.to_uppercase(), parent)
    }

    fn categories() -> Categories {
        Categories::new([
            category("transport", None),
            category("fuel", Some("transport")),
            category("good-life", None),
            category("restaurants", Some("good-life")),
            category("home", None),
            category("personal", None),
            category("new-root", None),
        ])
    }

    #[test]
    fn names_fall_back_to_the_id() {
        let categories = categories();
        assert_eq!(categories.name("fuel"), "FUEL");
        assert_eq!(categories.name("unknown"), "unknown");
    }

    #[test]
    fn children_are_in_their_parents_root() {
        let categories = categories();
        assert_eq!(categories.root("fuel"), "transport");
        assert_eq!(categories.root("transport"), "transport");
        assert_eq!(categories.root("unknown"), "unknown");
    }

    #[test]
    fn each_root_has_its_own_colour_and_children_share_it() {
        let categories = categories();
        let color = |id| categories.color_token(id);
        assert_eq!(color("good-life"), Some(tokens::WARNING));
        assert_eq!(color("restaurants"), Some(tokens::WARNING));
        assert_eq!(color("home"), Some(tokens::INFO));
        assert_eq!(color("personal"), Some(tokens::SUCCESS));
        assert_eq!(color("transport"), Some(tokens::ERROR));
        assert_eq!(color("fuel"), Some(tokens::ERROR));
        assert_eq!(color("new-root"), None);
        assert_eq!(color("unknown"), None);
    }

    #[test]
    fn root_colours_are_distinct_in_every_builtin_theme() {
        for info in opaline::list_available_themes() {
            let theme = opaline::load_by_name(&info.name).unwrap();
            let colors: Vec<_> = ROOT_COLORS.iter().map(|&(_, t)| theme.color(t)).collect();
            for (i, a) in colors.iter().enumerate() {
                for b in &colors[i + 1..] {
                    assert_ne!(
                        a, b,
                        "{}: two root categories have the same colour",
                        info.name
                    );
                }
            }
        }
    }

    #[test]
    fn a_cycle_does_not_hang() {
        let categories = Categories::new([category("a", Some("b")), category("b", Some("a"))]);
        categories.root("a");
        assert_eq!(categories.color_token("a"), None);
    }
}
