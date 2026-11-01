//! Accessible names for the text boxes in Settings (Q01).
//!
//! Settings paints its pages itself, but its text boxes are real Windows edit controls, and an edit
//! control has no label of its own: a screen reader reaching one would say only "edit". The host
//! gives each one a name and a short description through the Windows accessibility property
//! service (`IAccPropServices`, see `platform/accname.rs`). The words live here, in one table, so
//! they can be checked without Windows and stay in step with the visible text next to each box.

/// One text box in Settings.
pub struct EditLabel {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
}

/// In the order of `Edits` in `platform/settings.rs`.
pub const EDIT_LABELS: [EditLabel; 9] = [
    EditLabel {
        id: "filter",
        name: "Filter windows",
        description: "Type part of a program or window class name to narrow the list.",
    },
    EditLabel {
        id: "caption",
        name: "Title bar color",
        description: "A color such as #000000, or default.",
    },
    EditLabel {
        id: "border",
        name: "Border color",
        description: "A color such as #000000, or none, accent or default.",
    },
    EditLabel {
        id: "text",
        name: "Title text color",
        description: "A color such as #FFFFFF, or default.",
    },
    EditLabel {
        id: "tint",
        name: "Acrylic tint color",
        description: "A color such as #000000. Used by the tinted acrylic backdrop.",
    },
    EditLabel {
        id: "rule",
        name: "New rule or test",
        description: "A program name such as notepad.exe, optionally followed by class=Name. Press Add rule to create it or Test to try it without applying.",
    },
    EditLabel {
        id: "exclude",
        name: "App to leave alone",
        description: "A program name such as steam.exe. Press Add to exclude it.",
    },
    EditLabel {
        id: "look",
        name: "Look name",
        description: "A name for the look you want to save.",
    },
    EditLabel {
        id: "help",
        name: "Search help",
        description: "Type a word to find a help topic.",
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_edit_box_has_a_distinct_name_and_a_description() {
        for (index, label) in EDIT_LABELS.iter().enumerate() {
            assert!(
                !label.name.is_empty() && !label.description.is_empty(),
                "{}",
                label.id
            );
            for other in &EDIT_LABELS[index + 1..] {
                assert_ne!(label.id, other.id);
                assert_ne!(label.name, other.name, "two boxes would sound the same");
            }
        }
    }

    #[test]
    fn names_are_short_and_in_sentence_case_without_punctuation() {
        for label in &EDIT_LABELS {
            assert!(label.name.chars().count() <= 24, "{}", label.name);
            assert!(!label.name.ends_with(['.', ':', '…']), "{}", label.name);
            assert!(label.name.chars().next().is_some_and(char::is_uppercase));
            assert!(label.description.ends_with('.'), "{}", label.description);
            assert!(
                !label.name.contains("edit"),
                "the role is announced separately"
            );
        }
    }
}
