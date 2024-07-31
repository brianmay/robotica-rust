use robotica_macro::naive_time_constant;

#[test]
pub fn test_duration_constant_hms() {
    let result = naive_time_constant!(01:02:03);
    let expected = chrono::NaiveTime::from_hms_opt(1, 2, 3).unwrap();
    assert_eq!(result, expected);
}
