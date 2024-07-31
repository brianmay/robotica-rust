use robotica_macro::duration_constant;

#[test]
pub fn test_duration_constant_hms() {
    let result = duration_constant!(1:2:3);
    let expected = std::time::Duration::from_secs(3723u64);
    assert_eq!(result, expected);
}

#[test]
pub fn test_duration_constant_days() {
    let result = duration_constant!(2 days);
    let expected = std::time::Duration::from_secs(2 * 24 * 3600);
    assert_eq!(result, expected);
}

#[test]
pub fn test_duration_constant_hours() {
    let result = duration_constant!(2 hours);
    let expected = std::time::Duration::from_secs(2 * 3600);
    assert_eq!(result, expected);
}

#[test]
pub fn test_duration_constant_minutes() {
    let result = duration_constant!(12 minutes);
    let expected = std::time::Duration::from_secs(12 * 60);
    assert_eq!(result, expected);
}
