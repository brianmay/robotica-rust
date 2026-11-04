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
/// use robotica_common::mqtt::topics::topic_matches;
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
/// use robotica_common::mqtt::topics::topic_matches_any;
///
/// let patterns = vec!["foo/bar", "foo/baz"];
/// assert!(topic_matches_any("foo/bar", &patterns));
/// assert!(topic_matches_any("foo/baz", &patterns));
/// assert!(!topic_matches_any("foo", &patterns));
/// assert!(!topic_matches_any("foo/bar/baz", &patterns));
/// ```
#[must_use]
pub fn topic_matches_any(topic: &str, patterns: &[&str]) -> bool {
    patterns.iter().any(|pattern| topic_matches(topic, pattern))
}

/// A specification of an MQTT topic: either an exact topic or a wildcard pattern.
///
/// Exact topics match only the identical topic string. Patterns may contain
/// the MQTT wildcards `+` (single level) and `#` (zero or more levels) and
/// match via [`topic_matches`].
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum TopicSpec {
    /// An exact topic name; only this topic is matched.
    Exact(String),

    /// A wildcard pattern containing `+` or `#`.
    Pattern(String),
}

impl TopicSpec {
    /// Does the given topic match this specification?
    ///
    /// # Examples
    ///
    /// ```
    /// use robotica_common::mqtt::topics::TopicSpec;
    ///
    /// let exact = TopicSpec::Exact("stat/foo/POWER".to_string());
    /// assert!(exact.matches("stat/foo/POWER"));
    /// assert!(!exact.matches("stat/foo/POWER2"));
    ///
    /// let pattern = TopicSpec::Pattern("robotica/state/+/occupancy".to_string());
    /// assert!(pattern.matches("robotica/state/lounge/occupancy"));
    /// assert!(!pattern.matches("robotica/state/lounge/presence"));
    /// ```
    #[must_use]
    pub fn matches(&self, topic: &str) -> bool {
        match self {
            TopicSpec::Exact(spec) => spec == topic,
            TopicSpec::Pattern(pattern) => topic_matches(topic, pattern),
        }
    }
}

/// The set of MQTT topics that a view or controller subscribes to and sends on.
///
/// Topic derivation is the single source of truth for topic permissions: the
/// backend derives the per-user allow-list from the views a user can access,
/// so this type is the enforcement primitive for MQTT access checks.
#[derive(Debug, Default, Clone, Eq, PartialEq)]
pub struct Topics {
    /// The topics subscribed to.
    pub subscribe: Vec<TopicSpec>,

    /// The topics messages are sent to.
    pub send: Vec<TopicSpec>,
}

impl Topics {
    /// Add an exact topic to the subscribe list, if not already present.
    pub fn subscribe_exact(&mut self, topic: String) {
        Self::add(&mut self.subscribe, TopicSpec::Exact(topic));
    }

    /// Add a wildcard pattern to the subscribe list, if not already present.
    pub fn subscribe_pattern(&mut self, pattern: String) {
        Self::add(&mut self.subscribe, TopicSpec::Pattern(pattern));
    }

    /// Add an exact topic to the send list, if not already present.
    pub fn send_exact(&mut self, topic: String) {
        Self::add(&mut self.send, TopicSpec::Exact(topic));
    }

    /// Add a wildcard pattern to the send list, if not already present.
    pub fn send_pattern(&mut self, pattern: String) {
        Self::add(&mut self.send, TopicSpec::Pattern(pattern));
    }

    /// Merge another set of topics into this one, skipping duplicates.
    pub fn merge(&mut self, other: &Topics) {
        for spec in &other.subscribe {
            Self::add(&mut self.subscribe, spec.clone());
        }
        for spec in &other.send {
            Self::add(&mut self.send, spec.clone());
        }
    }

    /// Is the given topic allowed to be subscribed to?
    ///
    /// # Examples
    ///
    /// ```
    /// use robotica_common::mqtt::topics::Topics;
    ///
    /// let mut topics = Topics::default();
    /// topics.subscribe_exact("stat/foo/POWER".to_string());
    /// topics.subscribe_pattern("robotica/state/+/+/occupancy".to_string());
    ///
    /// assert!(topics.is_subscribe_allowed("stat/foo/POWER"));
    /// assert!(topics.is_subscribe_allowed("robotica/state/lounge/default/occupancy"));
    /// assert!(!topics.is_subscribe_allowed("stat/bar/POWER"));
    /// assert!(!topics.is_send_allowed("stat/foo/POWER"));
    /// ```
    #[must_use]
    pub fn is_subscribe_allowed(&self, topic: &str) -> bool {
        self.subscribe.iter().any(|spec| spec.matches(topic))
    }

    /// Is the given topic allowed to be sent to?
    #[must_use]
    pub fn is_send_allowed(&self, topic: &str) -> bool {
        self.send.iter().any(|spec| spec.matches(topic))
    }

    /// Add a spec to a list, skipping duplicates.
    fn add(list: &mut Vec<TopicSpec>, spec: TopicSpec) {
        if !list.contains(&spec) {
            list.push(spec);
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn test_topics_merge_deduplicates() {
        let mut a = Topics::default();
        a.subscribe_exact("stat/foo/POWER".to_string());
        a.send_exact("cmnd/foo/POWER".to_string());

        let mut b = Topics::default();
        b.subscribe_exact("stat/foo/POWER".to_string());
        b.subscribe_exact("tele/foo/LWT".to_string());

        a.merge(&b);
        a.merge(&b);

        assert_eq!(a.subscribe.len(), 2);
        assert_eq!(a.send.len(), 1);
    }

    #[test]
    fn test_pattern_and_exact_matching() {
        let mut topics = Topics::default();
        topics.subscribe_exact("robotica/state/brian/locations".to_string());
        topics.subscribe_pattern("robotica/state/+/+/occupancy".to_string());

        assert!(topics.is_subscribe_allowed("robotica/state/brian/locations"));
        assert!(!topics.is_subscribe_allowed("robotica/state/ellie/locations"));
        assert!(topics.is_subscribe_allowed("robotica/state/lounge/default/occupancy"));
        assert!(!topics.is_subscribe_allowed("robotica/state/brian/presence"));
    }
}
