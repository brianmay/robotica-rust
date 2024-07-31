use robotica_macro::time_delta_constant;

#[test]
pub fn test_time_delta_constant_hms() {
    let result = time_delta_constant!(1:2:3);
    let expected = chrono::TimeDelta::seconds(3723i64);
    assert_eq!(result, expected);
}

#[test]
pub fn test_time_delta_constant_days() {
    let result = time_delta_constant!(2 days);
    let expected = chrono::TimeDelta::seconds(2 * 24 * 3600);
    assert_eq!(result, expected);
}

#[test]
pub fn test_time_delta_constant_hours() {
    let result = time_delta_constant!(2 hours);
    let expected = chrono::TimeDelta::seconds(2 * 3600);
    assert_eq!(result, expected);
}

#[test]
pub fn test_time_delta_constant_minutes() {
    let result = time_delta_constant!(12 minutes);
    let expected = chrono::TimeDelta::seconds(12 * 60);
    assert_eq!(result, expected);
}

#[test]
pub fn test_time_delta_neg_constant_hms() {
    let result = time_delta_constant!(-1:2:3);
    let expected = chrono::TimeDelta::seconds(-3723i64);
    assert_eq!(result, expected);
}

#[test]
pub fn test_time_delta_neg_constant_days() {
    let result = time_delta_constant!(-2 days);
    let expected = chrono::TimeDelta::seconds(-2 * 24 * 3600);
    assert_eq!(result, expected);
}

#[test]
pub fn test_time_delta_neg_constant_hours() {
    let result = time_delta_constant!(-2 hours);
    let expected = chrono::TimeDelta::seconds(-2 * 3600);
    assert_eq!(result, expected);
}

#[test]
pub fn test_time_delta_neg_constant_minutes() {
    let result = time_delta_constant!(-2 minutes);
    let expected = chrono::TimeDelta::seconds(-2 * 60);
    assert_eq!(result, expected);
}
