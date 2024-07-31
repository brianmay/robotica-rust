use robotica_macro::time_delta_constant;

pub fn main() {
    let x = time_delta_constant!(01:02:03);
    let x = time_delta_constant!(-01:02:03);
    let x = time_delta_constant!(01:-01:03);
    let x = time_delta_constant!(01:60:03);
    let x = time_delta_constant!(01:02:-01);
    let x = time_delta_constant!(01:02:60);
    let x = time_delta_constant!(1.5:02:60);
    let days = time_delta_constant!(2 days);
    let hours = time_delta_constant!(3 hours);
    let minutes = time_delta_constant!(4 minutes);
    let neg_days = time_delta_constant!(-2 days);
    let neg_hours = time_delta_constant!(-3 hours);
    let neg_minutes = time_delta_constant!(-4 minutes);
    let half_day = time_delta_constant!(2.5 days);
}
