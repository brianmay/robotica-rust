//! Scheduler events to happen at a specific time.

lalrpop_mod!(conditions, "/scheduling/conditions.rs"); // synthesized by LALRPOP

mod ast;
pub mod classifier;

#[cfg(test)]
mod test;
