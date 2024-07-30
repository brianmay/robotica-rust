//! This module contains the parser for the conditions language.

lalrpop_mod!(
    #[allow(missing_docs)]
    #[allow(clippy::all, clippy::pedantic, clippy::nursery)]
    #[allow(clippy::unwrap_used)]
    conditions,
    "/conditions/conditions.rs"
); // synthesized by LALRPOP

pub mod ast;
pub use conditions::BooleanExprParser;

#[cfg(test)]
mod test;
