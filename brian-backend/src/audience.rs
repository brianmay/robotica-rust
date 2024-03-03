//! Helpers for message audiences.

pub const fn everyone() -> &'static str {
    "everyone"
}

pub const fn brian(private: bool) -> &'static str {
    if private {
        "brian(private)"
    } else {
        "brian"
    }
}

#[allow(dead_code)]
pub const fn twins() -> &'static str {
    "twins"
}

pub const fn dining_room() -> &'static str {
    "dining"
}
