//! Abstract syntax tree for conditions.

use std::{collections::HashSet, fmt::Display};

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use tap::Pipe;
use thiserror::Error;

use super::BooleanParser;

/// Quotes a string.
#[must_use]
pub fn quote(string: &str) -> String {
    let string = str::replace(string, "'", "\\'");
    format!("'{string}'")
}

/// Unquotes a string.
#[must_use]
pub fn unquote(string: &str) -> String {
    if (string.starts_with('\'') && string.ends_with('\''))
        || (string.starts_with('\"') && string.ends_with('\"'))
    {
        let string = string[1..string.len() - 1].to_string();
        str::replace(&string, "\\", "")
    } else {
        string.to_string()
    }
}

/// Error type for eval AST.
#[derive(Error, Debug)]
#[cfg_attr(test, derive(PartialEq))]
pub enum Error {
    /// Invalid field name.
    #[error("Invalid field name: {0}")]
    InvalidFieldName(String),

    /// Incompatible field type.
    #[error("Incompatible field type: {0} and {1}")]
    IncompatibleFieldTypes(Scalar, Scalar),

    /// Cannot test floats for equality.
    #[error("Cannot test floats for equality: {0} and {1}")]
    CannotTestFloatsForEquality(f32, f32),
}

/// Trait for getting values from a context.
pub trait GetValues: Sized {
    /// Returns the fields that can be queried.
    fn get_fields() -> Fields<Self>;

    /// Returns the value of a scalar field.
    fn get_scalar(&self, field: &Reference<Self>) -> Option<Scalar>;

    /// Returns the hash set of a field.
    fn get_hash_set(&self, field: &FieldRef<Self, HashSet<String>>) -> Option<&HashSet<String>>;
}

enum SerializeValue {
    String(String),
    Tree(Vec<SerializeValue>),
}

enum BracketableString {
    Bracketed(String),
    Unbracketed(String),
}

impl BracketableString {
    fn into_string(self) -> String {
        match self {
            Self::Unbracketed(s) => s,
            Self::Bracketed(s) => format!("({s})"),
        }
    }

    fn into_raw_string(self) -> String {
        match self {
            Self::Unbracketed(s) | Self::Bracketed(s) => s,
        }
    }
}

impl SerializeValue {
    fn new_string(s: impl Into<String>) -> Self {
        Self::String(s.into())
    }

    fn serialize_to_bracketable(&self) -> BracketableString {
        match self {
            Self::String(s) => BracketableString::Unbracketed(s.clone()),
            Self::Tree(v) => match v.as_slice() {
                [SerializeValue::String(s)] => BracketableString::Unbracketed(s.clone()),
                _ => v
                    .iter()
                    .map(SerializeValue::serialize_to_bracketable)
                    .map(BracketableString::into_string)
                    .collect::<Vec<String>>()
                    .join(" ")
                    .pipe(BracketableString::Bracketed),
            },
        }
    }
}

/// Reference to a field.
#[derive(Debug)]
pub struct FieldRef<T, F> {
    name: String,
    _phantom1: std::marker::PhantomData<T>,
    _phantom2: std::marker::PhantomData<F>,
}

impl<T, F> PartialEq for FieldRef<T, F> {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

impl Eq for FieldRef<String, Scalar> {}

impl<T, F> FieldRef<T, F> {
    /// Returns the name of the field.
    #[must_use]
    pub fn get_name(&self) -> &str {
        &self.name
    }
}

#[allow(clippy::implicit_hasher)]
impl<T: GetValues> FieldRef<T, HashSet<String>> {
    fn get_hash_set<'a>(&self, context: &'a T) -> Result<&'a HashSet<String>, Error> {
        context
            .get_hash_set(self)
            .ok_or_else(|| Error::InvalidFieldName(self.name.clone()))
    }
}

impl<T, F> Clone for FieldRef<T, F> {
    fn clone(&self) -> Self {
        Self {
            name: self.name.clone(),
            _phantom1: std::marker::PhantomData,
            _phantom2: std::marker::PhantomData,
        }
    }
}

impl<T, F> FieldRef<T, F> {
    /// Creates a new field reference.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            _phantom1: std::marker::PhantomData,
            _phantom2: std::marker::PhantomData,
        }
    }

    fn to_tree(&self) -> SerializeValue {
        SerializeValue::new_string(&self.name)
    }
}

impl<'de, T, F> Deserialize<'de> for FieldRef<T, F> {
    fn deserialize<D>(deserializer: D) -> Result<FieldRef<T, F>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s: String = Deserialize::deserialize(deserializer)?;
        Ok(FieldRef::new(s))
    }
}

/// Reference to a string or int field.
pub type Reference<T> = FieldRef<T, Scalar>;

impl<T> Reference<T> {
    /// Create a new reference to a field.
    pub fn new_scalar(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            _phantom1: std::marker::PhantomData,
            _phantom2: std::marker::PhantomData,
        }
    }
}

impl<T: GetValues> Reference<T> {
    fn get_scalar(&self, context: &T) -> Result<Scalar, Error> {
        context
            .get_scalar(self)
            .ok_or_else(|| Error::InvalidFieldName(self.get_name().to_string()))
    }
}

/// Value type, represents a String or an Integer.
#[derive(Clone, Debug)]
#[cfg_attr(test, derive(PartialEq))]
pub enum Scalar {
    /// String value.
    String(String),

    /// Integer value.
    Integer(i32),

    /// Float value.
    Float(f32),
}

impl Display for Scalar {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::String(s) => write!(f, "{s}"),
            Self::Integer(i) => write!(f, "{i}"),
            Self::Float(fl) => write!(f, "{fl}"),
        }
    }
}

impl Scalar {
    /// Creates a new value from a string.
    pub fn new_string(s: impl Into<String>) -> Self {
        Self::String(s.into())
    }

    /// Creates a new value from an integer.
    #[must_use]
    pub const fn new_int(i: i32) -> Self {
        Self::Integer(i)
    }

    /// Creates a new value from a float.
    #[must_use]
    pub const fn new_float(f: f32) -> Self {
        Self::Float(f)
    }
}

impl From<i32> for Scalar {
    fn from(i: i32) -> Self {
        Scalar::Integer(i)
    }
}

impl From<String> for Scalar {
    fn from(s: String) -> Self {
        Scalar::String(s)
    }
}

/// Fields that can be queried.
pub struct Fields<T> {
    /// Scalar fields.
    pub scalars: HashSet<String>,

    /// Hash set fields.
    pub hash_sets: HashSet<String>,
    _phantom: std::marker::PhantomData<T>,
}

impl<T> Fields<T> {
    /// Creates a new fields object.
    #[must_use]
    pub const fn new(scalars: HashSet<String>, hash_sets: HashSet<String>) -> Self {
        Self {
            scalars,
            hash_sets,
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<T> Default for Fields<T> {
    fn default() -> Self {
        Self {
            scalars: HashSet::new(),
            hash_sets: HashSet::new(),
            _phantom: std::marker::PhantomData,
        }
    }
}

/// AST for boolean expressions.
#[derive(Debug, PartialEq)]
pub enum Expr<T> {
    /// Integer value.
    Integer(i32),

    /// Float value.
    Float(f32),

    /// String value.
    String(String),

    /// Variable reference.
    Variable(Reference<T>),

    /// Binary operation.
    Op(Box<Expr<T>>, Opcode, Box<Expr<T>>),
}

impl<T> Expr<T> {
    fn to_tree(&self) -> SerializeValue {
        match self {
            Expr::Integer(n) => SerializeValue::new_string(n.to_string()),
            Expr::Float(f) => SerializeValue::new_string(format!("{f:.01}")),
            Expr::String(s) => SerializeValue::new_string(quote(s)),
            Expr::Variable(v) => v.to_tree(),
            Expr::Op(l, op, r) => {
                let v = vec![
                    l.to_tree(),
                    SerializeValue::new_string(op.to_string()),
                    r.to_tree(),
                ];
                SerializeValue::Tree(v)
            }
        }
    }
}
impl<T: Clone> Clone for Expr<T> {
    fn clone(&self) -> Self {
        match self {
            Self::Integer(arg0) => Self::Integer(*arg0),
            Self::Float(arg0) => Self::Float(*arg0),
            Self::String(arg0) => Self::String(arg0.clone()),
            Self::Variable(arg0) => Self::Variable(arg0.clone()),
            Self::Op(arg0, arg1, arg2) => Self::Op(arg0.clone(), *arg1, arg2.clone()),
        }
    }
}

impl<T: GetValues> Expr<T> {
    fn eval(&self, context: &T) -> Result<Scalar, Error> {
        match self {
            Expr::Integer(n) => (*n).pipe(Scalar::Integer).pipe(Ok),
            Expr::Float(f) => (*f).pipe(Scalar::Float).pipe(Ok),
            Expr::String(s) => s.clone().pipe(Scalar::String).pipe(Ok),
            Expr::Variable(v) => v.get_scalar(context),
            Expr::Op(l, op, r) => op.eval(&l.eval(context)?, &r.eval(context)?),
        }
    }
}

/// Integer operation.
#[derive(Debug, Eq, PartialEq, Copy, Clone)]
pub enum Opcode {
    /// Multiplication.
    Mul,

    /// Division.
    Div,

    /// Addition.
    Add,

    /// Subtraction.
    Sub,

    /// Remainder.
    Remainder,
}

impl Opcode {
    fn eval(self, l: &Scalar, r: &Scalar) -> Result<Scalar, Error> {
        match (l, r) {
            (Scalar::Integer(l), Scalar::Integer(r)) => self.eval_int(*l, *r),
            (Scalar::Float(l), Scalar::Float(r)) => self.eval_float(*l, *r),
            (l, r) => Err(Error::IncompatibleFieldTypes(l.clone(), r.clone())),
        }
    }

    fn eval_int(self, l: i32, r: i32) -> Result<Scalar, Error> {
        let result = match self {
            Opcode::Mul => l * r,
            Opcode::Div => l / r,
            Opcode::Add => l + r,
            Opcode::Sub => l - r,
            Opcode::Remainder => l % r,
        };

        result.pipe(Scalar::new_int).pipe(Ok)
    }

    fn eval_float(self, l: f32, r: f32) -> Result<Scalar, Error> {
        let result = match self {
            Opcode::Mul => l * r,
            Opcode::Div => l / r,
            Opcode::Add => l + r,
            Opcode::Sub => l - r,
            Opcode::Remainder => l % r,
        };

        result.pipe(Scalar::new_float).pipe(Ok)
    }

    const fn to_string(self) -> &'static str {
        match self {
            Opcode::Mul => "*",
            Opcode::Div => "/",
            Opcode::Add => "+",
            Opcode::Sub => "-",
            Opcode::Remainder => "%",
        }
    }
}

/// AST for conditions.
#[derive(Debug, PartialEq)]
pub enum Condition<T> {
    /// Boolean value.
    Boolean(Box<Boolean<T>>),

    /// Comparison operation.
    Op(Box<Expr<T>>, ConditionOpcode, Box<Expr<T>>),

    /// In operation.
    In(String, FieldRef<T, HashSet<String>>),

    /// Not in operation.
    NotIn(String, FieldRef<T, HashSet<String>>),
}

impl<T> Condition<T> {
    fn to_tree(&self) -> SerializeValue {
        match self {
            Condition::Boolean(b) => b.to_tree(),
            Condition::Op(l, op, r) => vec![
                l.to_tree(),
                SerializeValue::new_string(op.to_string()),
                r.to_tree(),
            ]
            .pipe(SerializeValue::Tree),
            Condition::In(l, r) => vec![
                SerializeValue::new_string(quote(l)),
                SerializeValue::new_string("in"),
                r.to_tree(),
            ]
            .pipe(SerializeValue::Tree),
            Condition::NotIn(l, r) => vec![
                SerializeValue::new_string(quote(l)),
                SerializeValue::new_string("not in"),
                r.to_tree(),
            ]
            .pipe(SerializeValue::Tree),
        }
    }
}
impl<T: Clone> Clone for Condition<T> {
    fn clone(&self) -> Self {
        match self {
            Self::Boolean(b) => Self::Boolean(b.clone()),
            Self::Op(l, op, r) => Self::Op(l.clone(), *op, r.clone()),
            Self::In(s, r) => Self::In(s.clone(), r.clone()),
            Self::NotIn(s, r) => Self::NotIn(s.clone(), r.clone()),
        }
    }
}

impl<T: GetValues> Condition<T> {
    fn eval(&self, context: &T) -> Result<bool, Error> {
        match self {
            Condition::Boolean(b) => b.eval(context),
            Condition::Op(l, op, r) => {
                let l = l.eval(context)?;
                let r = r.eval(context)?;
                match (l, r) {
                    (Scalar::Integer(l), Scalar::Integer(r)) => op.eval_int(l, r).pipe(Ok),
                    (Scalar::Float(l), Scalar::Float(r)) => op.eval_float(l, r),
                    (Scalar::String(l), Scalar::String(r)) => op.eval_string(&l, &r).pipe(Ok),
                    (l, r) => Err(Error::IncompatibleFieldTypes(l, r)),
                }
            }
            Condition::In(l, r) => r.get_hash_set(context).map(|m| m.contains(l)),
            Condition::NotIn(l, r) => r.get_hash_set(context).map(|m| !m.contains(l)),
        }
    }
}

/// Condition opcode.
#[derive(Debug, Eq, PartialEq, Copy, Clone)]
pub enum ConditionOpcode {
    /// Equal.
    Eq,

    /// Not equal.
    NotEq,

    /// Less than.
    Lt,

    /// Less than or equal.
    Lte,

    /// Greater than.
    Gt,

    /// Greater than or equal.
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

    fn eval_float(self, l: f32, r: f32) -> Result<bool, Error> {
        match self {
            ConditionOpcode::NotEq | ConditionOpcode::Eq => {
                Err(Error::CannotTestFloatsForEquality(l, r))
            }
            ConditionOpcode::Lt => Ok(l < r),
            ConditionOpcode::Lte => Ok(l <= r),
            ConditionOpcode::Gt => Ok(l > r),
            ConditionOpcode::Gte => Ok(l >= r),
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

    const fn to_string(self) -> &'static str {
        match self {
            ConditionOpcode::Eq => "==",
            ConditionOpcode::NotEq => "!=",
            ConditionOpcode::Lt => "<",
            ConditionOpcode::Lte => "<=",
            ConditionOpcode::Gt => ">",
            ConditionOpcode::Gte => ">=",
        }
    }
}

/// AST for boolean expressions.
#[derive(Debug, PartialEq)]
pub enum Boolean<T> {
    /// Negation.
    Not(Condition<T>),

    /// Logical AND.
    And(Box<Boolean<T>>, Condition<T>),

    /// Logical OR.
    Or(Box<Boolean<T>>, Condition<T>),

    /// Condition.
    Condition(Condition<T>),
}

impl<T: Clone> Clone for Boolean<T> {
    fn clone(&self) -> Self {
        match self {
            Boolean::Not(c) => Boolean::Not(c.clone()),
            Boolean::And(l, r) => Boolean::And(l.clone(), r.clone()),
            Boolean::Or(l, r) => Boolean::Or(l.clone(), r.clone()),
            Boolean::Condition(c) => Boolean::Condition(c.clone()),
        }
    }
}

impl<T> Boolean<T> {
    fn to_tree(&self) -> SerializeValue {
        match self {
            Boolean::Not(cond) => {
                vec![SerializeValue::new_string("not"), cond.to_tree()].pipe(SerializeValue::Tree)
            }
            Boolean::And(left, right) | Boolean::Or(left, right) => vec![
                left.to_tree(),
                SerializeValue::new_string(match self {
                    Boolean::And(_, _) => "and",
                    Boolean::Or(_, _) => "or",
                    _ => unreachable!(),
                }),
                right.to_tree(),
            ]
            .pipe(SerializeValue::Tree),
            Boolean::Condition(cond) => cond.to_tree(),
        }
    }

    /// Serializes the expression to a string.
    #[must_use]
    pub fn serialize_to_string(&self) -> String {
        let tree = self.to_tree();
        tree.serialize_to_bracketable().into_raw_string()
    }
}

impl<T: GetValues> Boolean<T> {
    /// Evaluates the boolean expression.
    ///
    /// # Errors
    ///
    /// Returns an error if the expression cannot be evaluated.
    pub fn eval(&self, context: &T) -> Result<bool, Error> {
        match self {
            Boolean::Not(cond) => cond.eval(context).map(|b| !b),
            Boolean::And(left, right) => Ok(left.eval(context)? && right.eval(context)?),
            Boolean::Or(left, right) => Ok(left.eval(context)? || right.eval(context)?),
            Boolean::Condition(cond) => cond.eval(context),
        }
    }
}

impl<'de, T: GetValues> Deserialize<'de> for Boolean<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let fields = T::get_fields();
        let s: String = Deserialize::deserialize(deserializer)?;
        let expr = BooleanParser::new()
            .parse(&fields, &s)
            .map_err(|e| serde::de::Error::custom(format!("Error parsing condition: {e}")))?;
        Ok(expr)
    }
}

impl<T> Serialize for Boolean<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let string = self.serialize_to_string();
        serializer.serialize_str(&string)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use std::collections::HashSet;

    use super::*;

    #[derive(Default)]
    struct Context {
        food: i32,
        dog: HashSet<String>,
    }

    impl GetValues for Context {
        fn get_fields() -> Fields<Self> {
            Fields::new(
                HashSet::from(["food".to_string()]),
                HashSet::from(["dog".to_string()]),
            )
        }

        fn get_scalar(&self, field: &Reference<Self>) -> Option<Scalar> {
            match field.get_name() {
                "food" => Some(Scalar::Integer(self.food)),
                _ => None,
            }
        }

        fn get_hash_set(
            &self,
            field: &FieldRef<Self, HashSet<String>>,
        ) -> Option<&HashSet<String>> {
            match field.name.as_str() {
                "dog" => Some(&self.dog),
                _ => None,
            }
        }
    }

    #[test]
    fn test_eval_single() {
        let context = Context::default();

        let result = Boolean::Condition(Condition::Op(
            Box::new(Expr::Integer(10)),
            ConditionOpcode::Eq,
            Box::new(Expr::Integer(10)),
        ))
        .eval(&context)
        .unwrap();

        assert!(result);
    }

    #[test]
    fn test_eval_not() {
        let context = Context::default();

        let result = Boolean::Not(Condition::Op(
            Box::new(Expr::Integer(10)),
            ConditionOpcode::Eq,
            Box::new(Expr::Integer(10)),
        ))
        .eval(&context)
        .unwrap();

        assert!(!result);
    }

    #[test]
    fn test_eval_in() {
        let mut context = Context::default();
        context.dog.insert("food".to_string());

        let result = Boolean::Condition(Condition::In("food".to_string(), FieldRef::new("dog")))
            .eval(&context)
            .unwrap();

        assert!(result);
    }

    #[test]
    fn test_eval_not_in() {
        let mut context = Context::default();
        context.dog.insert("food".to_string());

        let result = Boolean::Condition(Condition::NotIn("food".to_string(), FieldRef::new("dog")))
            .eval(&context)
            .unwrap();

        assert!(!result);
    }

    #[test]
    fn test_eval_and() {
        let context = Context::default();

        let result = Boolean::And(
            Box::new(Boolean::Condition(Condition::Op(
                Box::new(Expr::Integer(10)),
                ConditionOpcode::Eq,
                Box::new(Expr::Integer(10)),
            ))),
            Condition::Op(
                Box::new(Expr::Integer(10)),
                ConditionOpcode::Eq,
                Box::new(Expr::Integer(11)),
            ),
        )
        .eval(&context)
        .unwrap();

        assert!(!result);
    }

    #[test]
    fn test_eval_or_and() {
        let context = Context::default();

        let result = Boolean::And(
            Box::new(Boolean::Or(
                Box::new(Boolean::Condition(Condition::Op(
                    Box::new(Expr::Integer(9)),
                    ConditionOpcode::Eq,
                    Box::new(Expr::Integer(9)),
                ))),
                Condition::Op(
                    Box::new(Expr::Integer(10)),
                    ConditionOpcode::Eq,
                    Box::new(Expr::Integer(11)),
                ),
            )),
            Condition::Op(
                Box::new(Expr::Integer(11)),
                ConditionOpcode::Eq,
                Box::new(Expr::Integer(12)),
            ),
        )
        .eval(&context)
        .unwrap();

        assert!(!result);
    }

    #[test]
    fn test_eval_or_and_override() {
        let context = Context::default();

        let result = Boolean::Or(
            Box::new(Boolean::Condition(Condition::Op(
                Box::new(Expr::Integer(9)),
                ConditionOpcode::Eq,
                Box::new(Expr::Integer(9)),
            ))),
            Condition::Boolean(Box::new(Boolean::And(
                Box::new(Boolean::Condition(Condition::Op(
                    Box::new(Expr::Integer(10)),
                    ConditionOpcode::Eq,
                    Box::new(Expr::Integer(11)),
                ))),
                Condition::Op(
                    Box::new(Expr::Integer(11)),
                    ConditionOpcode::Eq,
                    Box::new(Expr::Integer(12)),
                ),
            ))),
        )
        .eval(&context)
        .unwrap();

        assert!(result);
    }

    #[test]
    fn test_eval_expression() {
        let context = Context {
            food: 22,
            ..Context::default()
        };

        let result = Boolean::Condition(Condition::Op(
            Box::new(Expr::Op(
                Box::new(Expr::Op(
                    Box::new(Expr::Integer(2)),
                    Opcode::Mul,
                    Box::new(Expr::Variable(Reference::new_scalar("food"))),
                )),
                Opcode::Add,
                Box::new(Expr::Integer(1)),
            )),
            ConditionOpcode::Eq,
            Box::new(Expr::Integer(45)),
        ))
        .eval(&context)
        .unwrap();

        assert!(result);
    }
}

/// Error type for parsing conditions.
#[derive(Error, Debug)]
#[cfg_attr(test, derive(Eq, PartialEq))]
pub enum ConditionError {
    /// Field not found.
    #[error("field {0} not found")]
    FieldNotFound(String),
}

#[cfg(test)]
mod test {
    #[test]
    fn test_quote() {
        let quoted = super::quote("hello's world");
        assert_eq!(quoted, "'hello\\'s world'");
    }

    #[test]
    fn test_unquote() {
        let unquoted = super::unquote("'hello\\'s world'");
        assert_eq!(unquoted, "hello's world");
    }
}
