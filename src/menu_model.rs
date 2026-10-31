//! The tray menu's item model, pure so its identity rules can be tested.
//!
//! The menu is owner-drawn: Windows asks us to measure and draw each item and hands back one
//! number per item (its "item data"). That number must identify *that item*. An earlier bug
//! patched it in by command id, but the title and every separator share id 0, so they all
//! resolved to the title. The rule now is positional: item data is the item's index in the
//! pre-order [`flatten`] of the tree, assigned when the item is created.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ItemKind {
    Title,
    Action,
    Checked(bool),
    Submenu,
    Separator,
}

#[derive(Clone, Debug)]
pub struct Item {
    pub id: u32,
    pub label: String,
    /// Optional second line that says what the item does.
    pub hint: String,
    pub kind: ItemKind,
    pub enabled: bool,
    pub children: Vec<Item>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Drawn {
    pub label: String,
    pub hint: String,
    pub kind: ItemKind,
    pub enabled: bool,
}

/// What the owner-draw callbacks look up, in the order items are created: an item first, then
/// its children.
pub fn flatten(items: &[Item]) -> Vec<Drawn> {
    let mut out = Vec::new();
    push(items, &mut out);
    out
}

fn push(items: &[Item], out: &mut Vec<Drawn>) {
    for item in items {
        out.push(Drawn {
            label: item.label.clone(),
            hint: item.hint.clone(),
            kind: item.kind,
            enabled: item.enabled,
        });
        push(&item.children, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: u32, label: &str, kind: ItemKind, children: Vec<Item>) -> Item {
        Item {
            id,
            label: label.into(),
            hint: String::new(),
            kind,
            enabled: true,
            children,
        }
    }

    fn sample() -> Vec<Item> {
        vec![
            item(0, "AllBlack11 3.5 Preview", ItemKind::Title, vec![]),
            item(0, "", ItemKind::Separator, vec![]),
            item(1, "Enabled", ItemKind::Checked(true), vec![]),
            item(
                0,
                "Presets",
                ItemKind::Submenu,
                vec![
                    item(20, "Pure OLED", ItemKind::Action, vec![]),
                    item(21, "Classic Mica", ItemKind::Action, vec![]),
                ],
            ),
            item(0, "", ItemKind::Separator, vec![]),
            item(0, "", ItemKind::Separator, vec![]),
            item(6, "Exit", ItemKind::Action, vec![]),
        ]
    }

    #[test]
    fn every_item_gets_its_own_entry_in_creation_order() {
        let drawn = flatten(&sample());
        assert_eq!(drawn.len(), 9);
        let kinds: Vec<ItemKind> = drawn.iter().map(|d| d.kind).collect();
        assert_eq!(
            kinds,
            vec![
                ItemKind::Title,
                ItemKind::Separator,
                ItemKind::Checked(true),
                ItemKind::Submenu,
                ItemKind::Action,
                ItemKind::Action,
                ItemKind::Separator,
                ItemKind::Separator,
                ItemKind::Action,
            ]
        );
    }

    #[test]
    fn separators_and_submenus_never_resolve_to_the_title() {
        // The regression: items sharing command id 0 must still have distinct identities.
        let drawn = flatten(&sample());
        let title = &drawn[0];
        for (index, entry) in drawn.iter().enumerate().skip(1) {
            assert_ne!(entry, title, "item {index} resolved to the title");
        }
        assert_eq!(drawn[3].label, "Presets");
        assert_eq!(drawn[1].label, "");
        assert_eq!(drawn[4].label, "Pure OLED");
    }

    #[test]
    fn index_assignment_by_a_running_counter_matches_flatten() {
        // Mirrors how the platform builds the menu: one counter, item before its children.
        fn assign(items: &[Item], next: &mut usize, out: &mut Vec<(usize, String)>) {
            for item in items {
                out.push((*next, item.label.clone()));
                *next += 1;
                assign(&item.children, next, out);
            }
        }
        let items = sample();
        let mut assigned = Vec::new();
        assign(&items, &mut 0, &mut assigned);
        let drawn = flatten(&items);
        for (index, label) in assigned {
            assert_eq!(drawn[index].label, label);
        }
    }
}
