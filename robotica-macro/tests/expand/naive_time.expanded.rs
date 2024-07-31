use robotica_macro::naive_time_constant;
pub fn main() {
    let x = match chrono::NaiveTime::from_num_seconds_from_midnight_opt(3723u32, 0) {
        Some(time) => time,
        None => {
            ::core::panicking::panic_fmt(format_args!("Invalid time"));
        }
    };
    let x = (/*ERROR*/);
    let x = (/*ERROR*/);
    let x = (/*ERROR*/);
    let x = (/*ERROR*/);
    let x = (/*ERROR*/);
    let x = (/*ERROR*/);
    let days = (/*ERROR*/);
    let hours = (/*ERROR*/);
    let minutes = (/*ERROR*/);
    let neg_days = (/*ERROR*/);
    let neg_hours = (/*ERROR*/);
    let neg_minutes = (/*ERROR*/);
    let half_day = (/*ERROR*/);
}
