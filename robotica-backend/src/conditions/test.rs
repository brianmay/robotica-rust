#![allow(clippy::unwrap_used)]

use std::collections::HashSet;

use crate::conditions::ast::FieldRef;

use super::ast::{
    BooleanExpr, Condition, ConditionError, ConditionOpcode, Error, Expr, Fields, GetValues,
    Opcode, Reference, Scalar,
};
use super::conditions;

#[derive(Debug, PartialEq, Eq)]
struct Context {
    dog: HashSet<String>,
    text: String,
    cat: i32,
    is_a_duck: bool,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct JsonField {
    value: BooleanExpr<Context>,
}

impl GetValues for Context {
    fn get_fields() -> Fields<Context> {
        let mut fields: Fields<Context> = Fields::default();
        fields.scalars.insert("cat".to_string());
        fields.scalars.insert("text".to_string());
        fields.scalars.insert("is_a_duck".to_string());
        fields.hash_sets.insert("dog".to_string());
        fields
    }

    fn get_scalar(&self, field: &Reference<Self>) -> Option<super::ast::Scalar> {
        match field.get_name() {
            "cat" => Some(super::ast::Scalar::Integer(self.cat)),
            "text" => Some(super::ast::Scalar::String(self.text.clone())),
            "is_a_duck" => Some(super::ast::Scalar::Boolean(self.is_a_duck)),
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
    BooleanExpr::Condition(Condition::Op(
        Box::new(Expr::Integer(10)),
        ConditionOpcode::Eq,
        Box::new(Expr::Integer(10))
    ))
)]
#[case(
    "(10 == 10)",
    BooleanExpr::Condition(Condition::BooleanExpr(Box::new(BooleanExpr::Condition(
        Condition::Op(
            Box::new(Expr::Integer(10)),
            ConditionOpcode::Eq,
            Box::new(Expr::Integer(10))
        )
    ))))
)]
#[case(
    "10 == 'string'",
    BooleanExpr::Condition(Condition::Op(
        Box::new(Expr::Integer(10)),
        ConditionOpcode::Eq,
        Box::new(Expr::String("string".to_string()))
    ))
)]
#[case(
    "10 != 10",
    BooleanExpr::Condition(Condition::Op(
        Box::new(Expr::Integer(10)),
        ConditionOpcode::NotEq,
        Box::new(Expr::Integer(10))
    ))
)]
#[case(
    "10 < 10",
    BooleanExpr::Condition(Condition::Op(
        Box::new(Expr::Integer(10)),
        ConditionOpcode::Lt,
        Box::new(Expr::Integer(10))
    ))
)]
#[case(
    "10.0 < 10.0",
    BooleanExpr::Condition(Condition::Op(
        Box::new(Expr::Float(10.0)),
        ConditionOpcode::Lt,
        Box::new(Expr::Float(10.0))
    ))
)]
#[case(
    "10 <= 10",
    BooleanExpr::Condition(Condition::Op(
        Box::new(Expr::Integer(10)),
        ConditionOpcode::Lte,
        Box::new(Expr::Integer(10))
    ))
)]
#[case(
    "10 > 10",
    BooleanExpr::Condition(Condition::Op(
        Box::new(Expr::Integer(10)),
        ConditionOpcode::Gt,
        Box::new(Expr::Integer(10))
    ))
)]
#[case(
    "10 >= 10",
    BooleanExpr::Condition(Condition::Op(
        Box::new(Expr::Integer(10)),
        ConditionOpcode::Gte,
        Box::new(Expr::Integer(10))
    ))
)]
#[case(
    "not 10 == 10",
    BooleanExpr::Not(Condition::Op(
        Box::new(Expr::Integer(10)),
        ConditionOpcode::Eq,
        Box::new(Expr::Integer(10))
    ))
)]
#[case(
    "'food' in dog",
    BooleanExpr::Condition(Condition::In(
        "food".to_string(),
        FieldRef::new("dog")
    ))
)]
#[case(
    "'food' not in dog",
    BooleanExpr::Condition(Condition::NotIn(
        "food".to_string(),
        FieldRef::new("dog")
    ))
)]
#[case(
    "\"food\" in dog",
    BooleanExpr::Condition(Condition::In(
        "food".to_string(),
        FieldRef::new("dog")
    ))
)]
#[case(
    "\"food\" not in dog",
    BooleanExpr::Condition(Condition::NotIn(
        "food".to_string(),
        FieldRef::new("dog")
    ))
)]
#[case(
    "10 == 10 and 11 == 11",
    BooleanExpr::And(
        Box::new(BooleanExpr::Condition(Condition::Op(
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
    BooleanExpr::And(
        Box::new(BooleanExpr::Or(
            Box::new(BooleanExpr::Condition(Condition::Op(
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
    BooleanExpr::Or(
        Box::new(BooleanExpr::Condition(Condition::Op(
            Box::new(Expr::Integer(10)),
            ConditionOpcode::Eq,
            Box::new(Expr::Integer(10))
        ))),
        Condition::BooleanExpr(Box::new(BooleanExpr::And(
            Box::new(BooleanExpr::Condition(Condition::Op(
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
    BooleanExpr::Condition(Condition::Op(
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
    BooleanExpr::Condition(Condition::Op(
        Box::new(Expr::Variable(Reference::new_scalar("text"))),
        ConditionOpcode::Eq,
        Box::new(Expr::String("string".to_string())),
    ))
)]
#[case(
    "text == \"string\"",
    BooleanExpr::Condition(Condition::Op(
        Box::new(Expr::Variable(Reference::new_scalar("text"))),
        ConditionOpcode::Eq,
        Box::new(Expr::String("string".to_string())),
    ))
)]
#[case(
    "is_a_duck == true",
    BooleanExpr::Condition(Condition::Op(
        Box::new(Expr::Variable(Reference::new_scalar("is_a_duck"))),
        ConditionOpcode::Eq,
        Box::new(Expr::Boolean(true)),
    ))
)]
#[case(
    "is_a_duck == false",
    BooleanExpr::Condition(Condition::Op(
        Box::new(Expr::Variable(Reference::new_scalar("is_a_duck"))),
        ConditionOpcode::Eq,
        Box::new(Expr::Boolean(false)),
    ))
)]
fn test_parse_condition(#[case] input: &str, #[case] expected: BooleanExpr<Context>) {
    let fields = Context::get_fields();
    let expr = conditions::BooleanExprParser::new()
        .parse(&fields, input)
        .unwrap();
    assert_eq!(expr, expected);
}

#[rstest::rstest]
#[case(
    "not_found == \"string\"",
    ConditionError::FieldNotFound("not_found".to_string())
)]
#[case(
    "is_a_duck == yes",
    ConditionError::FieldNotFound("yes".to_string())
)]
fn test_parse_condition_error(#[case] input: &str, #[case] expected: ConditionError) {
    use lalrpop_util::ParseError;

    let fields = Context::get_fields();
    let expr = conditions::BooleanExprParser::new()
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
    BooleanExpr::Condition(Condition::Op(
        Box::new(Expr::Integer(10)),
        ConditionOpcode::Eq,
        Box::new(Expr::Integer(10))
    ))
)]
#[case(
    "10 == 10",
    BooleanExpr::Condition(Condition::BooleanExpr(Box::new(BooleanExpr::Condition(
        Condition::BooleanExpr(Box::new(BooleanExpr::Condition(Condition::Op(
            Box::new(Expr::Integer(10)),
            ConditionOpcode::Eq,
            Box::new(Expr::Integer(10))
        ))))
    ))))
)]
#[case(
    "10 == 'string'",
    BooleanExpr::Condition(Condition::Op(
        Box::new(Expr::Integer(10)),
        ConditionOpcode::Eq,
        Box::new(Expr::String("string".to_string()))
    ))
)]
#[case(
    "10 != 10",
    BooleanExpr::Condition(Condition::Op(
        Box::new(Expr::Integer(10)),
        ConditionOpcode::NotEq,
        Box::new(Expr::Integer(10))
    ))
)]
#[case(
    "10 < 10",
    BooleanExpr::Condition(Condition::Op(
        Box::new(Expr::Integer(10)),
        ConditionOpcode::Lt,
        Box::new(Expr::Integer(10))
    ))
)]
#[case(
    "10.0 < 10.0",
    BooleanExpr::Condition(Condition::Op(
        Box::new(Expr::Float(10.0)),
        ConditionOpcode::Lt,
        Box::new(Expr::Float(10.0))
    ))
)]
#[case(
    "10 <= 10",
    BooleanExpr::Condition(Condition::Op(
        Box::new(Expr::Integer(10)),
        ConditionOpcode::Lte,
        Box::new(Expr::Integer(10))
    ))
)]
#[case(
    "10 > 10",
    BooleanExpr::Condition(Condition::Op(
        Box::new(Expr::Integer(10)),
        ConditionOpcode::Gt,
        Box::new(Expr::Integer(10))
    ))
)]
#[case(
    "10 >= 10",
    BooleanExpr::Condition(Condition::Op(
        Box::new(Expr::Integer(10)),
        ConditionOpcode::Gte,
        Box::new(Expr::Integer(10))
    ))
)]
#[case(
    "not (10 == 10)",
    BooleanExpr::Not(Condition::Op(
        Box::new(Expr::Integer(10)),
        ConditionOpcode::Eq,
        Box::new(Expr::Integer(10))
    ))
)]
#[case(
    "'food' in dog",
    BooleanExpr::Condition(Condition::In(
        "food".to_string(),
        FieldRef::new("dog")
    ))
)]
#[case(
    "'food' not in dog",
    BooleanExpr::Condition(Condition::NotIn(
        "food".to_string(),
        FieldRef::new("dog")
    ))
)]
#[case(
    "'food' in dog",
    BooleanExpr::Condition(Condition::In(
        "food".to_string(),
        FieldRef::new("dog")
    ))
)]
#[case(
    "'food' not in dog",
    BooleanExpr::Condition(Condition::NotIn(
        "food".to_string(),
        FieldRef::new("dog")
    ))
)]
#[case(
    "(10 == 10) and (11 == 11)",
    BooleanExpr::And(
        Box::new(BooleanExpr::Condition(Condition::Op(
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
    BooleanExpr::And(
        Box::new(BooleanExpr::Or(
            Box::new(BooleanExpr::Condition(Condition::Op(
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
    BooleanExpr::Or(
        Box::new(BooleanExpr::Condition(Condition::Op(
            Box::new(Expr::Integer(10)),
            ConditionOpcode::Eq,
            Box::new(Expr::Integer(10))
        ))),
        Condition::BooleanExpr(Box::new(BooleanExpr::And(
            Box::new(BooleanExpr::Condition(Condition::Op(
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
    BooleanExpr::Condition(Condition::Op(
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
    BooleanExpr::Condition(Condition::Op(
        Box::new(Expr::Variable(Reference::new_scalar("text"))),
        ConditionOpcode::Eq,
        Box::new(Expr::String("string".to_string())),
    ))
)]
#[case(
    "text == 'string'",
    BooleanExpr::Condition(Condition::Op(
        Box::new(Expr::Variable(Reference::new_scalar("text"))),
        ConditionOpcode::Eq,
        Box::new(Expr::String("string".to_string())),
    ))
)]
#[case(
    "is_a_duck == true",
    BooleanExpr::Condition(Condition::Op(
        Box::new(Expr::Variable(Reference::new_scalar("is_a_duck"))),
        ConditionOpcode::Eq,
        Box::new(Expr::Boolean(true)),
    ))
)]
#[case(
    "is_a_duck == false",
    BooleanExpr::Condition(Condition::Op(
        Box::new(Expr::Variable(Reference::new_scalar("is_a_duck"))),
        ConditionOpcode::Eq,
        Box::new(Expr::Boolean(false)),
    ))
)]
fn test_serialize_to_string(#[case] expected: &str, #[case] input: BooleanExpr<Context>) {
    let result = input.serialize_to_string();
    assert_eq!(result, expected);
}

#[rstest::rstest]
#[case(
    BooleanExpr::Condition(Condition::Op(
        Box::new(Expr::Integer(10)),
        ConditionOpcode::Eq,
        Box::new(Expr::Integer(10))
    )),
    Context {
        dog: HashSet::new(),
        text: "string".to_string(),
        cat: 10,
        is_a_duck: true,
    },
    true
)]
#[case(
    BooleanExpr::Condition(Condition::Op(
        Box::new(Expr::Integer(10)),
        ConditionOpcode::Eq,
        Box::new(Expr::Integer(11))
    )),
    Context {
        dog: HashSet::new(),
        text: "string".to_string(),
        cat: 10,
        is_a_duck: true,
    },
    false
)]
#[case(
    BooleanExpr::Condition(Condition::Op(
        Box::new(Expr::Integer(10)),
        ConditionOpcode::Eq,
        Box::new(Expr::Variable(Reference::new_scalar("cat")))
    )),
    Context {
        dog: HashSet::new(),
        text: "string".to_string(),
        cat: 10,
        is_a_duck: true,
    },
    true
)]
#[case(
    BooleanExpr::Condition(Condition::In(
        "food".to_string(),
        FieldRef::new("dog")
    )),
    Context {
        dog: vec!["food".to_string()].into_iter().collect(),
        text: "string".to_string(),
        cat: 10,
        is_a_duck: true,
    },
    true
)]
#[case(
    BooleanExpr::Condition(Condition::Op(
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
        is_a_duck: true,
    },
    true
)]
#[case(
    BooleanExpr::Condition(Condition::Op(
        Box::new(Expr::Variable(Reference::new_scalar("is_a_duck"))),
        ConditionOpcode::Eq,
        Box::new(Expr::Boolean(true)),
    )),
    Context {
        dog: vec!["food".to_string()].into_iter().collect(),
        text: "string".to_string(),
        cat: 10,
        is_a_duck: true,
    },
    true
)]
#[case(
    BooleanExpr::Condition(Condition::Op(
        Box::new(Expr::Variable(Reference::new_scalar("is_a_duck"))),
        ConditionOpcode::Eq,
        Box::new(Expr::Boolean(false)),
    )),
    Context {
        dog: vec!["food".to_string()].into_iter().collect(),
        text: "string".to_string(),
        cat: 10,
        is_a_duck: true,
    },
    false
)]
fn test_eval(
    #[case] input: BooleanExpr<Context>,
    #[case] context: Context,
    #[case] expected: bool,
) {
    let result = input.eval(&context).unwrap();
    assert_eq!(result, expected);
}

#[rstest::rstest]
#[case(
    BooleanExpr::Condition(Condition::Op(
        Box::new(Expr::Integer(10)),
        ConditionOpcode::Eq,
        Box::new(Expr::Variable(Reference::new_scalar("text")))
    )),
    Context {
        dog: HashSet::new(),
        text: "string".to_string(),
        cat: 10,
        is_a_duck: true,
    },
    Error::InvalidOperationForTypes("==".to_string(), Scalar::Integer(10), Scalar::String("string".to_string()))
)]
#[case(
    BooleanExpr::Condition(Condition::Op(
        Box::new(Expr::Integer(10)),
        ConditionOpcode::Eq,
        Box::new(Expr::Variable(Reference::new_scalar("not-found")))
    )),
    Context {
        dog: HashSet::new(),
        text: "string".to_string(),
        cat: 10,
        is_a_duck: true,
    },
    Error::InvalidFieldName("not-found".to_string())
)]
fn test_eval_error(
    #[case] input: BooleanExpr<Context>,
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
        BooleanExpr::Condition(Condition::Op(
            Box::new(Expr::Integer(10)),
            ConditionOpcode::Eq,
            Box::new(Expr::Integer(10))
        ))
    );
}

#[test]
fn test_json_deserialize() {
    let value = BooleanExpr::Condition(Condition::Op(
        Box::new(Expr::Integer(10)),
        ConditionOpcode::Eq,
        Box::new(Expr::Integer(10)),
    ));
    let json = JsonField { value };
    let serialized = serde_json::to_string(&json).unwrap();
    assert_eq!(serialized, "{\"value\":\"10 == 10\"}");
}
