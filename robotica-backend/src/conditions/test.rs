#![allow(clippy::unwrap_used)]

use std::collections::HashSet;

use crate::conditions::ast::FieldRef;

use super::ast::{
    Boolean, Condition, ConditionError, ConditionOpcode, Error, Expr, Fields, GetValues, Opcode,
    Reference, Scalar,
};
use super::conditions;

#[derive(Debug, PartialEq, Eq)]
struct Context {
    dog: HashSet<String>,
    text: String,
    cat: i32,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct JsonField {
    value: Boolean<Context>,
}

impl GetValues for Context {
    fn get_fields() -> Fields<Context> {
        let mut fields: Fields<Context> = Fields::default();
        fields.scalars.insert("cat".to_string());
        fields.scalars.insert("text".to_string());
        fields.hash_sets.insert("dog".to_string());
        fields
    }

    fn get_scalar(&self, field: &Reference<Self>) -> Option<super::ast::Scalar> {
        match field.get_name() {
            "cat" => Some(super::ast::Scalar::Integer(self.cat)),
            "text" => Some(super::ast::Scalar::String(self.text.clone())),
            _ => None,
        }
    }

    fn get_hash_set(
        &self,
        field: &super::ast::FieldRef<Self, HashSet<String>>,
    ) -> Option<&HashSet<String>> {
        match field.get_name() {
            "dog" => Some(&self.dog),
            _ => None,
        }
    }
}

#[rstest::rstest]
#[case(
    "10 == 10",
    Boolean::Condition(Condition::Op(
        Box::new(Expr::Integer(10)),
        ConditionOpcode::Eq,
        Box::new(Expr::Integer(10))
    ))
)]
#[case(
    "(10 == 10)",
    Boolean::Condition(Condition::Boolean(Box::new(Boolean::Condition(Condition::Op(
        Box::new(Expr::Integer(10)),
        ConditionOpcode::Eq,
        Box::new(Expr::Integer(10))
    )))))
)]
#[case(
    "10 == 'string'",
    Boolean::Condition(Condition::Op(
        Box::new(Expr::Integer(10)),
        ConditionOpcode::Eq,
        Box::new(Expr::String("string".to_string()))
    ))
)]
#[case(
    "10 != 10",
    Boolean::Condition(Condition::Op(
        Box::new(Expr::Integer(10)),
        ConditionOpcode::NotEq,
        Box::new(Expr::Integer(10))
    ))
)]
#[case(
    "10 < 10",
    Boolean::Condition(Condition::Op(
        Box::new(Expr::Integer(10)),
        ConditionOpcode::Lt,
        Box::new(Expr::Integer(10))
    ))
)]
#[case(
    "10.0 < 10.0",
    Boolean::Condition(Condition::Op(
        Box::new(Expr::Float(10.0)),
        ConditionOpcode::Lt,
        Box::new(Expr::Float(10.0))
    ))
)]
#[case(
    "10 <= 10",
    Boolean::Condition(Condition::Op(
        Box::new(Expr::Integer(10)),
        ConditionOpcode::Lte,
        Box::new(Expr::Integer(10))
    ))
)]
#[case(
    "10 > 10",
    Boolean::Condition(Condition::Op(
        Box::new(Expr::Integer(10)),
        ConditionOpcode::Gt,
        Box::new(Expr::Integer(10))
    ))
)]
#[case(
    "10 >= 10",
    Boolean::Condition(Condition::Op(
        Box::new(Expr::Integer(10)),
        ConditionOpcode::Gte,
        Box::new(Expr::Integer(10))
    ))
)]
#[case(
    "not 10 == 10",
    Boolean::Not(Condition::Op(
        Box::new(Expr::Integer(10)),
        ConditionOpcode::Eq,
        Box::new(Expr::Integer(10))
    ))
)]
#[case(
    "'food' in dog",
    Boolean::Condition(Condition::In(
        "food".to_string(),
        FieldRef::new("dog")
    ))
)]
#[case(
    "'food' not in dog",
    Boolean::Condition(Condition::NotIn(
        "food".to_string(),
        FieldRef::new("dog")
    ))
)]
#[case(
    "\"food\" in dog",
    Boolean::Condition(Condition::In(
        "food".to_string(),
        FieldRef::new("dog")
    ))
)]
#[case(
    "\"food\" not in dog",
    Boolean::Condition(Condition::NotIn(
        "food".to_string(),
        FieldRef::new("dog")
    ))
)]
#[case(
    "10 == 10 and 11 == 11",
    Boolean::And(
        Box::new(Boolean::Condition(Condition::Op(
            Box::new(Expr::Integer(10)),
            ConditionOpcode::Eq,
            Box::new(Expr::Integer(10))
        ))),
        Condition::Op(
            Box::new(Expr::Integer(11)),
            ConditionOpcode::Eq,
            Box::new(Expr::Integer(11))
        )
    )
)]
#[case(
    "10 == 10 or 11 == 11 and 12 == 12",
    Boolean::And(
        Box::new(Boolean::Or(
            Box::new(Boolean::Condition(Condition::Op(
                Box::new(Expr::Integer(10)),
                ConditionOpcode::Eq,
                Box::new(Expr::Integer(10))
            ))),
            Condition::Op(
                Box::new(Expr::Integer(11)),
                ConditionOpcode::Eq,
                Box::new(Expr::Integer(11))
            )
        )),
        Condition::Op(
            Box::new(Expr::Integer(12)),
            ConditionOpcode::Eq,
            Box::new(Expr::Integer(12))
        ),
    )
)]
#[case(
    "10 == 10 or (11 == 11 and 12 == 12)",
    Boolean::Or(
        Box::new(Boolean::Condition(Condition::Op(
            Box::new(Expr::Integer(10)),
            ConditionOpcode::Eq,
            Box::new(Expr::Integer(10))
        ))),
        Condition::Boolean(Box::new(Boolean::And(
            Box::new(Boolean::Condition(Condition::Op(
                Box::new(Expr::Integer(11)),
                ConditionOpcode::Eq,
                Box::new(Expr::Integer(11))
            ))),
            Condition::Op(
                Box::new(Expr::Integer(12)),
                ConditionOpcode::Eq,
                Box::new(Expr::Integer(12))
            )
        )))
    )
)]
#[case(
    "22 * cat + 66 == 9",
    Boolean::Condition(Condition::Op(
        Box::new(Expr::Op(
            Box::new(Expr::Op(
                Box::new(Expr::Integer(22)),
                Opcode::Mul,
                Box::new(Expr::Variable(Reference::new_scalar("cat"))),
            )),
            Opcode::Add,
            Box::new(Expr::Integer(66)),
        )),
        ConditionOpcode::Eq,
        Box::new(Expr::Integer(9)),
    ))
)]
#[case(
    "text == 'string'",
    Boolean::Condition(Condition::Op(
        Box::new(Expr::Variable(Reference::new_scalar("text"))),
        ConditionOpcode::Eq,
        Box::new(Expr::String("string".to_string())),
    ))
)]
#[case(
    "text == \"string\"",
    Boolean::Condition(Condition::Op(
        Box::new(Expr::Variable(Reference::new_scalar("text"))),
        ConditionOpcode::Eq,
        Box::new(Expr::String("string".to_string())),
    ))
)]
fn test_parse_condition(#[case] input: &str, #[case] expected: Boolean<Context>) {
    let fields = Context::get_fields();
    let expr = conditions::BooleanParser::new()
        .parse(&fields, input)
        .unwrap();
    assert_eq!(expr, expected);
}

#[rstest::rstest]
#[case(
    "not_found == \"string\"",
    ConditionError::FieldNotFound("not_found".to_string())
)]
fn test_parse_condition_error(#[case] input: &str, #[case] expected: ConditionError) {
    use lalrpop_util::ParseError;

    let fields = Context::get_fields();
    let expr = conditions::BooleanParser::new()
        .parse(&fields, input)
        .unwrap_err();
    let ParseError::User { error } = expr else {
        panic!("Expected a user error, got {expr:?}");
    };
    assert_eq!(error, expected);
}

#[rstest::rstest]
#[case(
    "10 == 10",
    Boolean::Condition(Condition::Op(
        Box::new(Expr::Integer(10)),
        ConditionOpcode::Eq,
        Box::new(Expr::Integer(10))
    ))
)]
#[case(
    "10 == 10",
    Boolean::Condition(Condition::Boolean(Box::new(Boolean::Condition(Condition::Boolean(
        Box::new(Boolean::Condition(Condition::Op(
            Box::new(Expr::Integer(10)),
            ConditionOpcode::Eq,
            Box::new(Expr::Integer(10))
        )))
    )))))
)]
#[case(
    "10 == 'string'",
    Boolean::Condition(Condition::Op(
        Box::new(Expr::Integer(10)),
        ConditionOpcode::Eq,
        Box::new(Expr::String("string".to_string()))
    ))
)]
#[case(
    "10 != 10",
    Boolean::Condition(Condition::Op(
        Box::new(Expr::Integer(10)),
        ConditionOpcode::NotEq,
        Box::new(Expr::Integer(10))
    ))
)]
#[case(
    "10 < 10",
    Boolean::Condition(Condition::Op(
        Box::new(Expr::Integer(10)),
        ConditionOpcode::Lt,
        Box::new(Expr::Integer(10))
    ))
)]
#[case(
    "10.0 < 10.0",
    Boolean::Condition(Condition::Op(
        Box::new(Expr::Float(10.0)),
        ConditionOpcode::Lt,
        Box::new(Expr::Float(10.0))
    ))
)]
#[case(
    "10 <= 10",
    Boolean::Condition(Condition::Op(
        Box::new(Expr::Integer(10)),
        ConditionOpcode::Lte,
        Box::new(Expr::Integer(10))
    ))
)]
#[case(
    "10 > 10",
    Boolean::Condition(Condition::Op(
        Box::new(Expr::Integer(10)),
        ConditionOpcode::Gt,
        Box::new(Expr::Integer(10))
    ))
)]
#[case(
    "10 >= 10",
    Boolean::Condition(Condition::Op(
        Box::new(Expr::Integer(10)),
        ConditionOpcode::Gte,
        Box::new(Expr::Integer(10))
    ))
)]
#[case(
    "not (10 == 10)",
    Boolean::Not(Condition::Op(
        Box::new(Expr::Integer(10)),
        ConditionOpcode::Eq,
        Box::new(Expr::Integer(10))
    ))
)]
#[case(
    "'food' in dog",
    Boolean::Condition(Condition::In(
        "food".to_string(),
        FieldRef::new("dog")
    ))
)]
#[case(
    "'food' not in dog",
    Boolean::Condition(Condition::NotIn(
        "food".to_string(),
        FieldRef::new("dog")
    ))
)]
#[case(
    "'food' in dog",
    Boolean::Condition(Condition::In(
        "food".to_string(),
        FieldRef::new("dog")
    ))
)]
#[case(
    "'food' not in dog",
    Boolean::Condition(Condition::NotIn(
        "food".to_string(),
        FieldRef::new("dog")
    ))
)]
#[case(
    "(10 == 10) and (11 == 11)",
    Boolean::And(
        Box::new(Boolean::Condition(Condition::Op(
            Box::new(Expr::Integer(10)),
            ConditionOpcode::Eq,
            Box::new(Expr::Integer(10))
        ))),
        Condition::Op(
            Box::new(Expr::Integer(11)),
            ConditionOpcode::Eq,
            Box::new(Expr::Integer(11))
        )
    )
)]
#[case(
    "((10 == 10) or (11 == 11)) and (12 == 12)",
    Boolean::And(
        Box::new(Boolean::Or(
            Box::new(Boolean::Condition(Condition::Op(
                Box::new(Expr::Integer(10)),
                ConditionOpcode::Eq,
                Box::new(Expr::Integer(10))
            ))),
            Condition::Op(
                Box::new(Expr::Integer(11)),
                ConditionOpcode::Eq,
                Box::new(Expr::Integer(11))
            )
        )),
        Condition::Op(
            Box::new(Expr::Integer(12)),
            ConditionOpcode::Eq,
            Box::new(Expr::Integer(12))
        ),
    )
)]
#[case(
    "(10 == 10) or ((11 == 11) and (12 == 12))",
    Boolean::Or(
        Box::new(Boolean::Condition(Condition::Op(
            Box::new(Expr::Integer(10)),
            ConditionOpcode::Eq,
            Box::new(Expr::Integer(10))
        ))),
        Condition::Boolean(Box::new(Boolean::And(
            Box::new(Boolean::Condition(Condition::Op(
                Box::new(Expr::Integer(11)),
                ConditionOpcode::Eq,
                Box::new(Expr::Integer(11))
            ))),
            Condition::Op(
                Box::new(Expr::Integer(12)),
                ConditionOpcode::Eq,
                Box::new(Expr::Integer(12))
            )
        )))
    )
)]
#[case(
    "((22 * cat) + 66) == 9",
    Boolean::Condition(Condition::Op(
        Box::new(Expr::Op(
            Box::new(Expr::Op(
                Box::new(Expr::Integer(22)),
                Opcode::Mul,
                Box::new(Expr::Variable(Reference::new_scalar("cat"))),
            )),
            Opcode::Add,
            Box::new(Expr::Integer(66)),
        )),
        ConditionOpcode::Eq,
        Box::new(Expr::Integer(9)),
    ))
)]
#[case(
    "text == 'string'",
    Boolean::Condition(Condition::Op(
        Box::new(Expr::Variable(Reference::new_scalar("text"))),
        ConditionOpcode::Eq,
        Box::new(Expr::String("string".to_string())),
    ))
)]
#[case(
    "text == 'string'",
    Boolean::Condition(Condition::Op(
        Box::new(Expr::Variable(Reference::new_scalar("text"))),
        ConditionOpcode::Eq,
        Box::new(Expr::String("string".to_string())),
    ))
)]
fn test_serialize_to_string(#[case] expected: &str, #[case] input: Boolean<Context>) {
    let result = input.serialize_to_string();
    assert_eq!(result, expected);
}

#[rstest::rstest]
#[case(
    Boolean::Condition(Condition::Op(
        Box::new(Expr::Integer(10)),
        ConditionOpcode::Eq,
        Box::new(Expr::Integer(10))
    )),
    Context {
        dog: HashSet::new(),
        text: "string".to_string(),
        cat: 10,
    },
    true
)]
#[case(
    Boolean::Condition(Condition::Op(
        Box::new(Expr::Integer(10)),
        ConditionOpcode::Eq,
        Box::new(Expr::Integer(11))
    )),
    Context {
        dog: HashSet::new(),
        text: "string".to_string(),
        cat: 10,
    },
    false
)]
#[case(
    Boolean::Condition(Condition::Op(
        Box::new(Expr::Integer(10)),
        ConditionOpcode::Eq,
        Box::new(Expr::Variable(Reference::new_scalar("cat")))
    )),
    Context {
        dog: HashSet::new(),
        text: "string".to_string(),
        cat: 10,
    },
    true
)]
#[case(
    Boolean::Condition(Condition::In(
        "food".to_string(),
        FieldRef::new("dog")
    )),
    Context {
        dog: vec!["food".to_string()].into_iter().collect(),
        text: "string".to_string(),
        cat: 10,
    },
    true
)]
#[case(
    Boolean::Condition(Condition::Op(
        Box::new(Expr::Op(
            Box::new(Expr::Op(
                Box::new(Expr::Integer(22)),
                Opcode::Mul,
                Box::new(Expr::Variable(Reference::new_scalar("cat"))),
            )),
            Opcode::Add,
            Box::new(Expr::Integer(66)),
        )),
        ConditionOpcode::Eq,
        Box::new(Expr::Integer(286)),
    )),
    Context {
        dog: vec!["food".to_string()].into_iter().collect(),
        text: "string".to_string(),
        cat: 10,
    },
    true
)]
fn test_eval(#[case] input: Boolean<Context>, #[case] context: Context, #[case] expected: bool) {
    let result = input.eval(&context).unwrap();
    assert_eq!(result, expected);
}

#[rstest::rstest]
#[case(
    Boolean::Condition(Condition::Op(
        Box::new(Expr::Integer(10)),
        ConditionOpcode::Eq,
        Box::new(Expr::Variable(Reference::new_scalar("text")))
    )),
    Context {
        dog: HashSet::new(),
        text: "string".to_string(),
        cat: 10,
    },
    Error::IncompatibleFieldTypes(Scalar::Integer(10), Scalar::String("string".to_string()))
)]
#[case(
    Boolean::Condition(Condition::Op(
        Box::new(Expr::Integer(10)),
        ConditionOpcode::Eq,
        Box::new(Expr::Variable(Reference::new_scalar("not-found")))
    )),
    Context {
        dog: HashSet::new(),
        text: "string".to_string(),
        cat: 10,
    },
    Error::InvalidFieldName("not-found".to_string())
)]
fn test_eval_error(
    #[case] input: Boolean<Context>,
    #[case] context: Context,
    #[case] expected: Error,
) {
    let result = input.eval(&context).unwrap_err();
    assert_eq!(result, expected);
}

#[test]
fn test_json_serialize() {
    let string = "{\"value\":\"10 == 10\"}";
    let parsed = serde_json::from_str::<JsonField>(string).unwrap();
    assert_eq!(
        parsed.value,
        Boolean::Condition(Condition::Op(
            Box::new(Expr::Integer(10)),
            ConditionOpcode::Eq,
            Box::new(Expr::Integer(10))
        ))
    );
}

#[test]
fn test_json_deserialize() {
    let value = Boolean::Condition(Condition::Op(
        Box::new(Expr::Integer(10)),
        ConditionOpcode::Eq,
        Box::new(Expr::Integer(10)),
    ));
    let json = JsonField { value };
    let serialized = serde_json::to_string(&json).unwrap();
    assert_eq!(serialized, "{\"value\":\"10 == 10\"}");
}
