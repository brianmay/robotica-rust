use std::{env, sync::Once};

static INIT: Once = Once::new();

pub fn setup() {
    INIT.call_once(|| {
        env_logger::init();
    });
    env::set_var("ROBOTICA_DEBUG", "false");
}
