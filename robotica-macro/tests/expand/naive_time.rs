use robotica_macro::naive_time_constant;

pub fn main() {
    let x = naive_time_constant!(01:02:03);
    let x = naive_time_constant!(-01:02:03);
    let x = naive_time_constant!(01:-01:03);
    let x = naive_time_constant!(01:60:03);
    let x = naive_time_constant!(01:02:-01);
    let x = naive_time_constant!(01:02:60);
    let x = naive_time_constant!(1.5:02:60);
    let days = naive_time_constant!(2 days);
    let hours = naive_time_constant!(3 hours);
    let minutes = naive_time_constant!(4 minutes);
    let neg_days = naive_time_constant!(-2 days);
    let neg_hours = naive_time_constant!(-3 hours);
    let neg_minutes = naive_time_constant!(-4 minutes);
    let half_day = naive_time_constant!(2.5 days);
}
