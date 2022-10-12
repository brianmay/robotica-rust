use std::{
    collections::{HashMap, HashSet},
    fmt::Display,
};

use field_ref::{FieldRef, GetField};
use thiserror::Error;

#[derive(Debug, Eq, PartialEq)]
pub enum Reference<T> {
    String(FieldRef<T, String>),
    Int(FieldRef<T, i32>),
}

// Note built in Clone method requires T: Clone, which is not required by this implementation.
#[allow(clippy::clone_on_copy)]
impl<T> Clone for Reference<T> {
    fn clone(&self) -> Self {
        match self {
            Self::String(arg0) => Self::String(arg0.clone()),
            Self::Int(arg0) => Self::Int(arg0.clone()),
        }
    }
}

#[derive(Debug)]
pub enum Value {
    String(String),
    Int(i32),
}

#[derive(Debug, Eq, PartialEq)]
pub enum Type {
    String,
    Int,
}

impl Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::String => write!(f, "String"),
            Self::Int => write!(f, "Int"),
        }
    }
}

#[derive(Error, Debug)]
pub enum TypeError {
    #[error("Types {0} and {1} are not compatible")]
    TypeMismatch(Type, Type),
}

impl From<i32> for Value {
    fn from(i: i32) -> Self {
        Value::Int(i)
    }
}

impl From<String> for Value {
    fn from(s: String) -> Self {
        Value::String(s)
    }
}

pub struct Fields<T> {
    pub any: HashMap<String, Reference<T>>,
    pub sets: HashMap<String, FieldRef<T, HashSet<String>>>,
}

impl<T> Default for Fields<T> {
    fn default() -> Self {
        Self {
            any: HashMap::new(),
            sets: HashMap::new(),
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum Expr<T> {
    Number(i32),
    String(String),
    Variable(Reference<T>),
    Op(Box<Expr<T>>, Opcode, Box<Expr<T>>),
}

impl<T: Clone> Clone for Expr<T> {
    fn clone(&self) -> Self {
        match self {
            Self::Number(arg0) => Self::Number(*arg0),
            Self::String(arg0) => Self::String(arg0.clone()),
            Self::Variable(arg0) => Self::Variable(arg0.clone()),
            Self::Op(arg0, arg1, arg2) => Self::Op(arg0.clone(), *arg1, arg2.clone()),
        }
    }
}

impl<T> Expr<T> {
    fn eval(&self, context: &T) -> Value {
        match self {
            Expr::Number(n) => (*n).into(),
            Expr::String(s) => s.clone().into(),
            Expr::Variable(v) => match v {
                Reference::String(r) => context.get_field(*r).clone().into(),
                Reference::Int(r) => (*context.get_field(*r)).into(),
            },
            Expr::Op(l, op, r) => op.eval(&l.eval(context), &r.eval(context)),
        }
    }

    pub fn type_of(&self) -> Result<Type, TypeError> {
        match self {
            Expr::Number(_) => Ok(Type::Int),
            Expr::String(_) => Ok(Type::String),
            Expr::Variable(v) => match v {
                Reference::String(_) => Ok(Type::String),
                Reference::Int(_) => Ok(Type::Int),
            },
            Expr::Op(l, _, r) => {
                let l_type = l.type_of()?;
                let r_type = r.type_of()?;
                if l_type == r_type {
                    Ok(l_type)
                } else {
                    Err(TypeError::TypeMismatch(l_type, r_type))
                }
            }
        }
    }
}

#[derive(Debug, Eq, PartialEq, Copy, Clone)]
pub enum Opcode {
    Mul,
    Div,
    Add,
    Sub,
    Remainder,
}

impl Opcode {
    fn eval(self, l: &Value, r: &Value) -> Value {
        let l = match l {
            Value::Int(i) => i,
            Value::String(_) => panic!("Invalid left operand"),
        };

        let r = match r {
            Value::Int(i) => i,
            Value::String(_) => panic!("Invalid right operand"),
        };

        let result = match self {
            Opcode::Mul => l * r,
            Opcode::Div => l / r,
            Opcode::Add => l + r,
            Opcode::Sub => l - r,
            Opcode::Remainder => l % r,
        };

        result.into()
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum Condition<T> {
    Boolean(Box<Boolean<T>>),
    Op(Box<Expr<T>>, ConditionOpcode, Box<Expr<T>>),
    In(String, field_ref::FieldRef<T, HashSet<String>>),
    NotIn(String, field_ref::FieldRef<T, HashSet<String>>),
}

impl<T: Clone> Clone for Condition<T> {
    fn clone(&self) -> Self {
        match self {
            Self::Boolean(b) => Self::Boolean(b.clone()),
            Self::Op(l, op, r) => Self::Op(l.clone(), *op, r.clone()),
            Self::In(s, r) => Self::In(s.clone(), *r),
            Self::NotIn(s, r) => Self::NotIn(s.clone(), *r),
        }
    }
}

impl<T> Condition<T> {
    fn eval(&self, context: &T) -> bool {
        match self {
            Condition::Boolean(b) => b.eval(context),
            Condition::Op(l, op, r) => {
                let l = l.eval(context);
                let r = r.eval(context);
                match (l, r) {
                    (Value::Int(l), Value::Int(r)) => op.eval_int(l, r),
                    (Value::String(l), Value::String(r)) => op.eval_string(&l, &r),
                    _ => panic!("Invalid operands"),
                }
            }
            Condition::In(l, r) => context.get_field(*r).contains(l),
            Condition::NotIn(l, r) => !context.get_field(*r).contains(l),
        }
    }
}

#[derive(Debug, Eq, PartialEq, Copy, Clone)]
pub enum ConditionOpcode {
    Eq,
    NotEq,
    Lt,
    Lte,
    Gt,
    Gte,
}

impl ConditionOpcode {
    const fn eval_int(self, l: i32, r: i32) -> bool {
        match self {
            ConditionOpcode::Eq => l == r,
            ConditionOpcode::NotEq => l != r,
            ConditionOpcode::Lt => l < r,
            ConditionOpcode::Lte => l <= r,
            ConditionOpcode::Gt => l > r,
            ConditionOpcode::Gte => l >= r,
        }
    }

    fn eval_string(self, l: &str, r: &str) -> bool {
        match self {
            ConditionOpcode::Eq => l == r,
            ConditionOpcode::NotEq => l != r,
            ConditionOpcode::Lt => l < r,
            ConditionOpcode::Lte => l <= r,
            ConditionOpcode::Gt => l > r,
            ConditionOpcode::Gte => l >= r,
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum Boolean<T> {
    Not(Condition<T>),
    And(Box<Boolean<T>>, Condition<T>),
    Or(Box<Boolean<T>>, Condition<T>),
    Cond(Condition<T>),
}

impl<T: Clone> Clone for Boolean<T> {
    fn clone(&self) -> Self {
        match self {
            Boolean::Not(c) => Boolean::Not(c.clone()),
            Boolean::And(l, r) => Boolean::And(l.clone(), r.clone()),
            Boolean::Or(l, r) => Boolean::Or(l.clone(), r.clone()),
            Boolean::Cond(c) => Boolean::Cond(c.clone()),
        }
    }
}

impl<T> Boolean<T> {
    pub fn eval(&self, context: &T) -> bool {
        match self {
            Boolean::Not(cond) => !cond.eval(context),
            Boolean::And(left, right) => left.eval(context) && right.eval(context),
            Boolean::Or(left, right) => left.eval(context) || right.eval(context),
            Boolean::Cond(cond) => cond.eval(context),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use field_ref::field_ref_of;

    use super::*;

    #[derive(Default)]
    struct Context {
        food: i32,
        dog: HashSet<String>,
    }

    #[test]
    fn test_eval_single() {
        let context = Context::default();

        let result = Boolean::Cond(Condition::Op(
            Box::new(Expr::Number(10)),
            ConditionOpcode::Eq,
            Box::new(Expr::Number(10)),
        ))
        .eval(&context);

        assert!(result);
    }

    #[test]
    fn test_eval_not() {
        let context = Context::default();

        let result = Boolean::Not(Condition::Op(
            Box::new(Expr::Number(10)),
            ConditionOpcode::Eq,
            Box::new(Expr::Number(10)),
        ))
        .eval(&context);

        assert!(!result);
    }

    #[test]
    fn test_eval_in() {
        let mut context = Context::default();
        context.dog.insert("food".to_string());

        let result = Boolean::Cond(Condition::In(
            "food".to_string(),
            field_ref_of!(Context => dog),
        ))
        .eval(&context);

        assert!(result);
    }

    #[test]
    fn test_eval_not_in() {
        let mut context = Context::default();
        context.dog.insert("food".to_string());
        // context
        //     .set_values
        //     .insert("dog".to_string(), HashSet::from(["food".to_string()]));

        let result = Boolean::Cond(Condition::NotIn(
            "food".to_string(),
            field_ref_of!(Context => dog),
        ))
        .eval(&context);

        assert!(!result);
    }

    #[test]
    fn test_eval_and() {
        let context = Context::default();

        let result = Boolean::And(
            Box::new(Boolean::Cond(Condition::Op(
                Box::new(Expr::Number(10)),
                ConditionOpcode::Eq,
                Box::new(Expr::Number(10)),
            ))),
            Condition::Op(
                Box::new(Expr::Number(10)),
                ConditionOpcode::Eq,
                Box::new(Expr::Number(11)),
            ),
        )
        .eval(&context);

        assert!(!result);
    }

    #[test]
    fn test_eval_or_and() {
        let context = Context::default();

        let result = Boolean::And(
            Box::new(Boolean::Or(
                Box::new(Boolean::Cond(Condition::Op(
                    Box::new(Expr::Number(9)),
                    ConditionOpcode::Eq,
                    Box::new(Expr::Number(9)),
                ))),
                Condition::Op(
                    Box::new(Expr::Number(10)),
                    ConditionOpcode::Eq,
                    Box::new(Expr::Number(11)),
                ),
            )),
            Condition::Op(
                Box::new(Expr::Number(11)),
                ConditionOpcode::Eq,
                Box::new(Expr::Number(12)),
            ),
        )
        .eval(&context);

        assert!(!result);
    }

    #[test]
    fn test_eval_or_and_override() {
        let context = Context::default();

        let result = Boolean::Or(
            Box::new(Boolean::Cond(Condition::Op(
                Box::new(Expr::Number(9)),
                ConditionOpcode::Eq,
                Box::new(Expr::Number(9)),
            ))),
            Condition::Boolean(Box::new(Boolean::And(
                Box::new(Boolean::Cond(Condition::Op(
                    Box::new(Expr::Number(10)),
                    ConditionOpcode::Eq,
                    Box::new(Expr::Number(11)),
                ))),
                Condition::Op(
                    Box::new(Expr::Number(11)),
                    ConditionOpcode::Eq,
                    Box::new(Expr::Number(12)),
                ),
            ))),
        )
        .eval(&context);

        assert!(result);
    }

    #[test]
    fn test_eval_expression() {
        let context = Context {
            food: 22,
            ..Context::default()
        };

        let result = Boolean::Cond(Condition::Op(
            Box::new(Expr::Op(
                Box::new(Expr::Op(
                    Box::new(Expr::Number(2)),
                    Opcode::Mul,
                    Box::new(Expr::Variable(Reference::Int(
                        field_ref_of!(Context => food),
                    ))),
                )),
                Opcode::Add,
                Box::new(Expr::Number(1)),
            )),
            ConditionOpcode::Eq,
            Box::new(Expr::Number(45)),
        ))
        .eval(&context);

        assert!(result);
    }
}

#[derive(Error, Debug)]
pub enum ConditionsError {
    #[error("field {0} not found")]
    FieldNotFound(String),

    #[error("type error: {0}")]
    TypeError(#[from] TypeError),
}
