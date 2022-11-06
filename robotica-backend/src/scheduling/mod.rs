//! Schedule events to happen at a specific time.

lalrpop_mod!(
    #[allow(clippy::needless_lifetimes)]
    #[allow(clippy::missing_const_for_fn)]
    #[allow(clippy::redundant_pub_crate)]
    #[allow(clippy::redundant_closure_for_method_calls)]
    #[allow(clippy::needless_pass_by_value)]
    #[allow(clippy::just_underscores_and_digits)]
    #[allow(clippy::too_many_lines)]
    #[allow(clippy::cast_sign_loss)]
    #[allow(clippy::no_effect_underscore_binding)]
    #[allow(clippy::unnested_or_patterns)]
    #[allow(clippy::option_if_let_else)]
    conditions,
    "/scheduling/conditions.rs"
); // synthesized by LALRPOP

mod ast;
pub mod classifier;
pub mod executor;
pub mod scheduler;
pub mod sequencer;

#[cfg(test)]
mod test;
