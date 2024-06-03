//! Serde utilities
use serde_yml::{Mapping, Value};
use thiserror::Error;

/// An error merging YAML values.
#[derive(Debug, Error)]
pub enum Error {
    /// The types of the values do not match.
    #[error("Types do not match {0} != {1}")]
    InvalidTypes(String, String),
}

/// Merge two YAML values
///
/// # Errors
///
/// Will return an error if the types of the values do not match.
pub fn merge_yaml(a: Value, b: Value) -> Result<Value, Error> {
    #[allow(clippy::match_same_arms)]
    match (a, b) {
        (Value::Mapping(mut a), Value::Mapping(b)) => {
            let mut r = Mapping::new();
            for (k, vb) in b {
                let va = a.remove(k.clone()).unwrap_or(Value::Null);
                if !vb.is_null() {
                    r.insert(k.clone(), merge_yaml(va, vb)?);
                }
            }
            for (k, va) in a {
                if !va.is_null() {
                    r.insert(k, va);
                }
            }

            Ok(Value::Mapping(r))
        }
        (Value::Number(_), b @ Value::Number(_)) => Ok(b),
        (Value::String(_), b @ Value::String(_)) => Ok(b),
        (Value::Sequence(_), b @ Value::Sequence(_)) => Ok(b),
        (Value::Bool(_), b @ Value::Bool(_)) => Ok(b),
        (Value::Null, b) => Ok(b),
        (_, b @ Value::Null) => Ok(b),
        (a, b) => Err(Error::InvalidTypes(
            serde_yml::to_string(&a).unwrap_or_default(),
            serde_yml::to_string(&b).unwrap_or_default(),
        )),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use serde_yml::Value;

    #[test]
    fn test_merge_yaml_simple() {
        let a = r"
a: 1
c: 3
d: 5
";

        let b = r"
a: 2
b: 3
d: null
";

        let c = r"
a: 2
b: 3
c: 3
";

        let a = serde_yml::from_str::<Value>(a).unwrap();
        let b = serde_yml::from_str::<Value>(b).unwrap();
        let c = serde_yml::from_str::<Value>(c).unwrap();
        let r = merge_yaml(a, b).unwrap();
        assert_eq!(r, c);
    }

    #[test]
    fn test_merge_yaml_nested() {
        let a = r"
first:
    a: 1
    c: 3
third:
    a: 1
    c: 3
";

        let b = r"
second:
    a: 2
    b: 3
third:
    a: 2
    b: 3
";

        let c = r"
first:
    a: 1
    c: 3
second:
    a: 2
    b: 3
third:
    a: 2
    b: 3
    c: 3
";

        let a = serde_yml::from_str::<Value>(a).unwrap();
        let b = serde_yml::from_str::<Value>(b).unwrap();
        let c = serde_yml::from_str::<Value>(c).unwrap();
        let r = merge_yaml(a, b).unwrap();
        assert_eq!(r, c);
    }
}
