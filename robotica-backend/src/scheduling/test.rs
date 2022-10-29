use field_ref::field_ref_of;
use lalrpop_util::ParseError;
use std::collections::{HashMap, HashSet};

use crate::scheduling::ast::ConditionsError;

use super::ast::{
    Boolean, Condition, ConditionOpcode, Expr, Fields, Opcode, Reference, Type, TypeError,
};
use super::conditions;

#[derive(Debug, PartialEq, Eq)]
struct Context {
    dog: HashSet<String>,
    text: String,
    cat: i32,
}

fn get_fields() -> Fields<Context> {
    let mut fields: Fields<Context> = Fields {
        any: HashMap::new(),
        sets: HashMap::new(),
    };
    fields.any.insert(
        "cat".to_string(),
        Reference::Int(field_ref_of!(Context => cat)),
    );
    fields.any.insert(
        "text".to_string(),
        Reference::String(field_ref_of!(Context => text)),
    );
    fields
        .sets
        .insert("dog".to_string(), field_ref_of!(Context => dog));
    fields
}

#[test]
fn test_parser_single() {
    let fields = get_fields();
    let expr = conditions::BooleanParser::new()
        .parse(&fields, "10 == 10")
        .unwrap();
    assert_eq!(
        expr,
        Boolean::Cond(Condition::Op(
            Box::new(Expr::Number(10)),
            ConditionOpcode::Eq,
            Box::new(Expr::Number(10))
        ))
    );
}

#[test]
fn test_parser_mismatch() {
    let fields = get_fields();
    let expr = conditions::BooleanParser::new()
        .parse(&fields, "10 == 'string'")
        .unwrap_err();
    assert!(matches!(
        expr,
        ParseError::User {
            error: ConditionsError::TypeError(TypeError::TypeMismatch(Type::Int, Type::String))
        }
    ));
}

#[test]
fn test_parser_not() {
    let fields = get_fields();
    let expr = conditions::BooleanParser::new()
        .parse(&fields, "not 10 == 10")
        .unwrap();
    assert_eq!(
        expr,
        Boolean::Not(Condition::Op(
            Box::new(Expr::Number(10)),
            ConditionOpcode::Eq,
            Box::new(Expr::Number(10))
        ))
    );
}

#[test]
fn test_parser_in() {
    let fields = get_fields();
    let expr = conditions::BooleanParser::new()
        .parse(&fields, "'food' in dog")
        .unwrap();
    assert_eq!(
        expr,
        Boolean::Cond(Condition::In(
            "food".to_string(),
            field_ref_of!(Context => dog)
        ))
    );
}

#[test]
fn test_parser_not_in() {
    let fields = get_fields();
    let expr = conditions::BooleanParser::new()
        .parse(&fields, "'food' not in dog")
        .unwrap();
    assert_eq!(
        expr,
        Boolean::Cond(Condition::NotIn(
            "food".to_string(),
            field_ref_of!(Context => dog)
        ))
    );
}

#[test]
fn test_parser_and() {
    let fields = get_fields();
    let expr = conditions::BooleanParser::new()
        .parse(&fields, "10 == 10 and 11 == 11")
        .unwrap();
    assert_eq!(
        expr,
        Boolean::And(
            Box::new(Boolean::Cond(Condition::Op(
                Box::new(Expr::Number(10)),
                ConditionOpcode::Eq,
                Box::new(Expr::Number(10))
            ))),
            Condition::Op(
                Box::new(Expr::Number(11)),
                ConditionOpcode::Eq,
                Box::new(Expr::Number(11))
            )
        )
    );
}

#[test]
fn test_parser_or_and() {
    let fields = get_fields();
    let expr = conditions::BooleanParser::new()
        .parse(&fields, "9 == 9 or 10 == 10 and 11 == 11")
        .unwrap();
    assert_eq!(
        expr,
        Boolean::And(
            Box::new(Boolean::Or(
                Box::new(Boolean::Cond(Condition::Op(
                    Box::new(Expr::Number(9)),
                    ConditionOpcode::Eq,
                    Box::new(Expr::Number(9))
                ))),
                Condition::Op(
                    Box::new(Expr::Number(10)),
                    ConditionOpcode::Eq,
                    Box::new(Expr::Number(10))
                )
            )),
            Condition::Op(
                Box::new(Expr::Number(11)),
                ConditionOpcode::Eq,
                Box::new(Expr::Number(11))
            )
        )
    );
}

#[test]
fn test_parser_or_and_override() {
    let fields = get_fields();
    let expr = conditions::BooleanParser::new()
        .parse(&fields, "9 == 9 or (10 == 10 and 11 == 11)")
        .unwrap();
    assert_eq!(
        expr,
        Boolean::Or(
            Box::new(Boolean::Cond(Condition::Op(
                Box::new(Expr::Number(9)),
                ConditionOpcode::Eq,
                Box::new(Expr::Number(9))
            ))),
            Condition::Boolean(Box::new(Boolean::And(
                Box::new(Boolean::Cond(Condition::Op(
                    Box::new(Expr::Number(10)),
                    ConditionOpcode::Eq,
                    Box::new(Expr::Number(10))
                ))),
                Condition::Op(
                    Box::new(Expr::Number(11)),
                    ConditionOpcode::Eq,
                    Box::new(Expr::Number(11))
                )
            ))),
        )
    );
}

#[test]
fn test_parser_expression() {
    let fields = get_fields();
    let expr = conditions::BooleanParser::new()
        .parse(&fields, "22 * cat + 66 == 9")
        .unwrap();
    assert_eq!(
        expr,
        Boolean::Cond(Condition::Op(
            Box::new(Expr::Op(
                Box::new(Expr::Op(
                    Box::new(Expr::Number(22)),
                    Opcode::Mul,
                    Box::new(Expr::Variable(Reference::Int(
                        field_ref_of!(Context => cat)
                    ))),
                )),
                Opcode::Add,
                Box::new(Expr::Number(66)),
            )),
            ConditionOpcode::Eq,
            Box::new(Expr::Number(9)),
        ))
    );
}

#[test]
fn test_parser_string() {
    let fields = get_fields();
    let expr = conditions::BooleanParser::new()
        .parse(&fields, "text == 'string'")
        .unwrap();
    assert_eq!(
        expr,
        Boolean::Cond(Condition::Op(
            Box::new(Expr::Variable(Reference::String(
                field_ref_of!(Context => text)
            ))),
            ConditionOpcode::Eq,
            Box::new(Expr::String("string".to_string())),
        ))
    );
}
