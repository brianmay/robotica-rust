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

pub const fn twins() -> &'static str {
    "twins"
}
