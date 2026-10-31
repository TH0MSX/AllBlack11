//! Version text shown to people. "3.0.0-preview.1" reads as "3.0 Preview".

pub fn display_version(raw: &str) -> String {
    match raw.split_once("-preview") {
        Some((base, _)) => format!("{} Preview", base.strip_suffix(".0").unwrap_or(base)),
        None => raw.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::display_version;

    #[test]
    fn previews_keep_major_and_minor() {
        assert_eq!(display_version("3.0.0-preview.1"), "3.0 Preview");
        assert_eq!(display_version("3.1.0-preview.2"), "3.1 Preview");
        assert_eq!(display_version("3.1.4-preview.1"), "3.1.4 Preview");
    }

    #[test]
    fn releases_are_shown_as_is() {
        assert_eq!(display_version("3.0.0"), "3.0.0");
    }
}
