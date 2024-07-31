use robotica_macro::duration_constant;

pub fn main() {
    let x = duration_constant!(01:02:03);
    let x = duration_constant!(-01:02:03);
    let x = duration_constant!(01:-01:03);
    let x = duration_constant!(01:60:03);
    let x = duration_constant!(01:02:-01);
    let x = duration_constant!(01:02:60);
    let x = duration_constant!(1.5:02:60);
    let days = duration_constant!(2 days);
    let hours = duration_constant!(3 hours);
    let minutes = duration_constant!(4 minutes);
    let neg_days = duration_constant!(-2 days);
    let neg_hours = duration_constant!(-3 hours);
    let neg_minutes = duration_constant!(-4 minutes);
    let half_day = duration_constant!(2.5 days);
}
