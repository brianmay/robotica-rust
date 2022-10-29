//! Functions for dealing with mqtt topics

/// Does this topic match the given pattern?
///
/// The pattern can contain wildcards:
/// - `+` matches a single level
/// - `#` matches zero or more levels
///
/// # Examples
///
/// ```
/// use robotica_backend::services::mqtt::topics::topic_matches;
///
/// assert!(topic_matches("foo/bar", "foo/bar"));
/// assert!(topic_matches("foo", "foo/+"));
/// assert!(topic_matches("foo/", "foo/+"));
/// assert!(topic_matches("foo/bar", "foo/+"));
/// assert!(topic_matches("foo", "foo/#"));
/// assert!(topic_matches("foo/", "foo/#"));
/// assert!(topic_matches("foo/bar", "foo/#"));
/// assert!(topic_matches("foo/bar/baz", "foo/#"));
/// assert!(topic_matches("foo/or/bar", "foo/+/bar"));
/// assert!(!topic_matches("foo/bar", "foo"));
/// assert!(!topic_matches("foo/bar", "foo/bar/baz"));
/// assert!(!topic_matches("foo/bar", "foo/baz"));
/// assert!(!topic_matches("foo/bar", "foo/baz/#"));
/// assert!(!topic_matches("foo/bar", "foo/+/bar"));
/// assert!(!topic_matches("foo/or", "foo/+/bar"));
/// assert!(!topic_matches("foo/or/else", "foo/+/bar"));
/// ```
#[must_use]
pub fn topic_matches(topic: &str, pattern: &str) -> bool {
    let mut topic_parts = topic.split('/');
    let mut pattern_parts = pattern.split('/');
    loop {
        match (topic_parts.next(), pattern_parts.next()) {
            (Some(topic_part), Some(pattern_part)) => {
                if pattern_part == "#" {
                    return true;
                } else if pattern_part != "+" && pattern_part != topic_part {
                    return false;
                }
            }
            (Some(_), None) => return false,
            (None, Some(pattern_part)) => {
                if pattern_part == "#" {
                    return true;
                } else if pattern_part != "+" {
                    return false;
                }
            }
            (None, None) => return true,
        }
    }
}

/// Does this topic match any of the given patterns?
/// # Examples
///
/// ```
/// use robotica_backend::services::mqtt::topics::topic_matches_any;
///
/// let patterns = vec!["foo/bar", "foo/baz"];
/// let patterns = patterns.iter().map(|s| s.to_string());
/// assert!(topic_matches_any("foo/bar", patterns.clone()));
/// assert!(topic_matches_any("foo/baz", patterns.clone()));
/// assert!(!topic_matches_any("foo", patterns.clone()));
/// assert!(!topic_matches_any("foo/bar/baz", patterns));
/// ```
#[must_use]
pub fn topic_matches_any(topic: &str, patterns: impl IntoIterator<Item = String>) -> bool {
    patterns
        .into_iter()
        .any(|pattern| topic_matches(topic, &pattern))
}
