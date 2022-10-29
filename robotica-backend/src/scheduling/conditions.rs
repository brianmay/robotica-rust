// auto-generated: "lalrpop 0.19.8"
// sha3: 778ec4bf412af8dcfe9f869da85ab1eed81f3ec0fd12e28e9885990b09735b2a
use std::str::FromStr;
use std::collections::HashSet;
use crate::scheduling::ast::{Expr, Opcode, Condition, ConditionOpcode, Boolean, Fields, TypeError, ConditionsError, Reference};
use field_ref::FieldRef;
use lalrpop_util::ParseError;
#[allow(unused_extern_crates)]
extern crate lalrpop_util as __lalrpop_util;
#[allow(unused_imports)]
use self::__lalrpop_util::state_machine as __state_machine;
extern crate core;
extern crate alloc;

#[cfg_attr(rustfmt, rustfmt_skip)]
mod __parse__Boolean {
    #![allow(non_snake_case, non_camel_case_types, unused_mut, unused_variables, unused_imports, unused_parens, clippy::all)]

    use std::str::FromStr;
    use std::collections::HashSet;
    use crate::scheduling::ast::{Expr, Opcode, Condition, ConditionOpcode, Boolean, Fields, TypeError, ConditionsError, Reference};
    use field_ref::FieldRef;
    use lalrpop_util::ParseError;
    #[allow(unused_extern_crates)]
    extern crate lalrpop_util as __lalrpop_util;
    #[allow(unused_imports)]
    use self::__lalrpop_util::state_machine as __state_machine;
    extern crate core;
    extern crate alloc;
    use self::__lalrpop_util::lexer::Token;
    #[allow(dead_code)]
    pub(crate) enum __Symbol<'input, T>
     {
        Variant0(&'input str),
        Variant1(Boolean<T>),
        Variant2(Condition<T>),
        Variant3(ConditionOpcode),
        Variant4(String),
        Variant5(Box<Expr<T>>),
        Variant6(Opcode),
        Variant7(i32),
        Variant8(FieldRef<T, HashSet<String>>),
        Variant9(Reference<T>),
    }
    const __ACTION: &[i8] = &[
        // State 0
        0, 4, 0, 5, 6, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 7, 0, 0, 29, 30,
        // State 1
        31, 0, 0, 0, 0, 0, 0, 32, 33, 0, 34, 35, 36, 37, 38, 0, 0, 0, 0, 0, 0, 0,
        // State 2
        -19, 0, 39, 0, 0, -19, 40, -19, -19, 41, -19, -19, -19, -19, -19, -19, 0, 0, 0, -19, 0, 0,
        // State 3
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 43,
        // State 4
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 43,
        // State 5
        0, 4, 0, 5, 6, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 7, 0, 0, 29, 30,
        // State 6
        0, 4, 0, 5, 6, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 29, 30,
        // State 7
        0, 4, 0, 5, 6, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 29, 30,
        // State 8
        0, 4, 0, 5, 6, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 29, 30,
        // State 9
        0, 15, 0, 16, 17, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 29, 30,
        // State 10
        0, 15, 0, 16, 17, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 29, 30,
        // State 11
        0, 15, 0, 16, 17, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 29, 30,
        // State 12
        31, 0, 0, 0, 0, 53, 0, 32, 33, 0, 34, 35, 36, 37, 38, 0, 0, 0, 0, 0, 0, 0,
        // State 13
        0, 0, 0, 0, 0, -6, 0, 32, 33, 0, 0, 0, 0, 0, 0, -6, 0, 0, 0, -6, 0, 0,
        // State 14
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 43,
        // State 15
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 43,
        // State 16
        0, 15, 0, 16, 17, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 29, 30,
        // State 17
        -18, 0, 39, 0, 0, -18, 40, -18, -18, 41, -18, -18, -18, -18, -18, -18, 0, 0, 0, -18, 0, 0,
        // State 18
        0, 0, 0, 0, 0, 53, 0, 32, 33, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 19
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 59,
        // State 20
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 59,
        // State 21
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 59,
        // State 22
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 59,
        // State 23
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 8, 0, 0, 0, 9, 0, 0,
        // State 24
        0, 0, 0, 0, 0, -4, 0, 0, 0, 0, 0, 0, 0, 0, 0, -4, 0, 0, 0, -4, 0, 0,
        // State 25
        -29, 0, -29, 0, 0, -29, -29, -29, -29, -29, -29, -29, -29, -29, -29, -29, 0, 0, 0, -29, 0, 0,
        // State 26
        -23, 0, -23, 0, 0, -23, -23, -23, -23, -23, -23, -23, -23, -23, -23, -23, 0, 0, 0, -23, 0, 0,
        // State 27
        -32, 0, -32, 0, 0, -32, -32, -32, -32, -32, -32, -32, -32, -32, -32, -32, 0, 0, 0, -32, 0, 0,
        // State 28
        -27, 0, -27, 0, 0, -27, -27, -27, -27, -27, -27, -27, -27, -27, -27, -27, 0, 0, 0, -27, 0, 0,
        // State 29
        -34, 0, -34, 0, 0, -34, -34, -34, -34, -34, -34, -34, -34, -34, -34, -34, 0, 0, 0, -34, 0, 0,
        // State 30
        0, -12, 0, -12, -12, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, -12, -12,
        // State 31
        0, -20, 0, -20, -20, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, -20, -20,
        // State 32
        0, -21, 0, -21, -21, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, -21, -21,
        // State 33
        0, -13, 0, -13, -13, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, -13, -13,
        // State 34
        0, -14, 0, -14, -14, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, -14, -14,
        // State 35
        0, -11, 0, -11, -11, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, -11, -11,
        // State 36
        0, -15, 0, -15, -15, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, -15, -15,
        // State 37
        0, -16, 0, -16, -16, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, -16, -16,
        // State 38
        0, -26, 0, -26, -26, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, -26, -26,
        // State 39
        0, -24, 0, -24, -24, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, -24, -24,
        // State 40
        0, -25, 0, -25, -25, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, -25, -25,
        // State 41
        0, 50, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 42
        0, -17, 0, -17, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 43
        0, 0, 0, 51, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 44
        0, 0, 0, 0, 0, 52, 0, 0, 0, 0, 0, 0, 0, 0, 0, 8, 0, 0, 0, 9, 0, 0,
        // State 45
        0, 0, 0, 0, 0, -1, 0, 0, 0, 0, 0, 0, 0, 0, 0, -1, 0, 0, 0, -1, 0, 0,
        // State 46
        0, 0, 0, 0, 0, -2, 0, 0, 0, 0, 0, 0, 0, 0, 0, -2, 0, 0, 0, -2, 0, 0,
        // State 47
        0, 0, 0, 0, 0, -3, 0, 0, 0, 0, 0, 0, 0, 0, 0, -3, 0, 0, 0, -3, 0, 0,
        // State 48
        -22, 0, -22, 0, 0, -22, -22, -22, -22, -22, -22, -22, -22, -22, -22, -22, 0, 0, 0, -22, 0, 0,
        // State 49
        -31, 0, -31, 0, 0, -31, -31, -31, -31, -31, -31, -31, -31, -31, -31, 0, 20, 0, 21, 0, 0, 0,
        // State 50
        -30, 0, -30, 0, 0, -30, -30, -30, -30, -30, -30, -30, -30, -30, -30, 0, 22, 0, 23, 0, 0, 0,
        // State 51
        0, 0, 0, 0, 0, -5, 0, 0, 0, 0, 0, 0, 0, 0, 0, -5, 0, 0, 0, -5, 0, 0,
        // State 52
        -33, 0, -33, 0, 0, -33, -33, -33, -33, -33, -33, -33, -33, -33, -33, -33, 0, 0, 0, -33, 0, 0,
        // State 53
        0, 56, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 54
        0, 0, 0, 57, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // State 55
        -31, 0, -31, 0, 0, -31, -31, -31, -31, -31, -31, -31, -31, -31, -31, -31, 0, 0, 0, -31, 0, 0,
        // State 56
        -30, 0, -30, 0, 0, -30, -30, -30, -30, -30, -30, -30, -30, -30, -30, -30, 0, 0, 0, -30, 0, 0,
        // State 57
        0, 0, 0, 0, 0, -8, 0, 0, 0, 0, 0, 0, 0, 0, 0, -8, 0, 0, 0, -8, 0, 0,
        // State 58
        0, 0, 0, 0, 0, -28, 0, 0, 0, 0, 0, 0, 0, 0, 0, -28, 0, 0, 0, -28, 0, 0,
        // State 59
        0, 0, 0, 0, 0, -10, 0, 0, 0, 0, 0, 0, 0, 0, 0, -10, 0, 0, 0, -10, 0, 0,
        // State 60
        0, 0, 0, 0, 0, -7, 0, 0, 0, 0, 0, 0, 0, 0, 0, -7, 0, 0, 0, -7, 0, 0,
        // State 61
        0, 0, 0, 0, 0, -9, 0, 0, 0, 0, 0, 0, 0, 0, 0, -9, 0, 0, 0, -9, 0, 0,
    ];
    fn __action(state: i8, integer: usize) -> i8 {
        __ACTION[(state as usize) * 22 + integer]
    }
    const __EOF_ACTION: &[i8] = &[
        // State 0
        0,
        // State 1
        0,
        // State 2
        -19,
        // State 3
        0,
        // State 4
        0,
        // State 5
        0,
        // State 6
        0,
        // State 7
        0,
        // State 8
        0,
        // State 9
        0,
        // State 10
        0,
        // State 11
        0,
        // State 12
        0,
        // State 13
        -6,
        // State 14
        0,
        // State 15
        0,
        // State 16
        0,
        // State 17
        -18,
        // State 18
        0,
        // State 19
        0,
        // State 20
        0,
        // State 21
        0,
        // State 22
        0,
        // State 23
        -35,
        // State 24
        -4,
        // State 25
        -29,
        // State 26
        -23,
        // State 27
        -32,
        // State 28
        -27,
        // State 29
        -34,
        // State 30
        0,
        // State 31
        0,
        // State 32
        0,
        // State 33
        0,
        // State 34
        0,
        // State 35
        0,
        // State 36
        0,
        // State 37
        0,
        // State 38
        0,
        // State 39
        0,
        // State 40
        0,
        // State 41
        0,
        // State 42
        0,
        // State 43
        0,
        // State 44
        0,
        // State 45
        -1,
        // State 46
        -2,
        // State 47
        -3,
        // State 48
        -22,
        // State 49
        0,
        // State 50
        0,
        // State 51
        -5,
        // State 52
        -33,
        // State 53
        0,
        // State 54
        0,
        // State 55
        -31,
        // State 56
        -30,
        // State 57
        -8,
        // State 58
        -28,
        // State 59
        -10,
        // State 60
        -7,
        // State 61
        -9,
    ];
    fn __goto(state: i8, nt: usize) -> i8 {
        match nt {
            0 => match state {
                5 => 44,
                _ => 23,
            },
            1 => match state {
                6 => 45,
                7 => 46,
                8 => 47,
                _ => 24,
            },
            2 => 9,
            3 => match state {
                4 => 43,
                14 => 53,
                15 => 54,
                _ => 41,
            },
            4 => match state {
                5 => 12,
                9 => 13,
                16 => 18,
                _ => 1,
            },
            5 => 10,
            6 => match state {
                10 => 17,
                _ => 2,
            },
            7 => 11,
            8 => 25,
            9 => match state {
                20 => 59,
                21 => 60,
                22 => 61,
                _ => 57,
            },
            10 => match state {
                11 => 48,
                _ => 26,
            },
            11 => 27,
            _ => 0,
        }
    }
    fn __expected_tokens(__state: i8) -> alloc::vec::Vec<alloc::string::String> {
        const __TERMINAL: &[&str] = &[
            r###""!=""###,
            r###""\"""###,
            r###""%""###,
            r###""'""###,
            r###""(""###,
            r###"")""###,
            r###""*""###,
            r###""+""###,
            r###""-""###,
            r###""/""###,
            r###""<""###,
            r###""<=""###,
            r###""==""###,
            r###"">""###,
            r###"">=""###,
            r###""and""###,
            r###""in""###,
            r###""not""###,
            r###""not in""###,
            r###""or""###,
            r###"r#"[0-9]+"#"###,
            r###"r#"[a-zA-Z_][a-zA-Z0-9_]*"#"###,
        ];
        __TERMINAL.iter().enumerate().filter_map(|(index, terminal)| {
            let next_state = __action(__state, index);
            if next_state == 0 {
                None
            } else {
                Some(alloc::string::ToString::to_string(terminal))
            }
        }).collect()
    }
    pub(crate) struct __StateMachine<'input, '__2, T>
    where T: '__2
    {
        fields: &'__2 Fields<T>,
        input: &'input str,
        __phantom: core::marker::PhantomData<(&'input (), T)>,
    }
    impl<'input, '__2, T> __state_machine::ParserDefinition for __StateMachine<'input, '__2, T>
    where T: '__2
    {
        type Location = usize;
        type Error = ConditionsError;
        type Token = Token<'input>;
        type TokenIndex = usize;
        type Symbol = __Symbol<'input, T>;
        type Success = Boolean<T>;
        type StateIndex = i8;
        type Action = i8;
        type ReduceIndex = i8;
        type NonterminalIndex = usize;

        #[inline]
        fn start_location(&self) -> Self::Location {
              Default::default()
        }

        #[inline]
        fn start_state(&self) -> Self::StateIndex {
              0
        }

        #[inline]
        fn token_to_index(&self, token: &Self::Token) -> Option<usize> {
            __token_to_integer(token, core::marker::PhantomData::<(&(), T)>)
        }

        #[inline]
        fn action(&self, state: i8, integer: usize) -> i8 {
            __action(state, integer)
        }

        #[inline]
        fn error_action(&self, state: i8) -> i8 {
            __action(state, 22 - 1)
        }

        #[inline]
        fn eof_action(&self, state: i8) -> i8 {
            __EOF_ACTION[state as usize]
        }

        #[inline]
        fn goto(&self, state: i8, nt: usize) -> i8 {
            __goto(state, nt)
        }

        fn token_to_symbol(&self, token_index: usize, token: Self::Token) -> Self::Symbol {
            __token_to_symbol(token_index, token, core::marker::PhantomData::<(&(), T)>)
        }

        fn expected_tokens(&self, state: i8) -> alloc::vec::Vec<alloc::string::String> {
            __expected_tokens(state)
        }

        #[inline]
        fn uses_error_recovery(&self) -> bool {
            false
        }

        #[inline]
        fn error_recovery_symbol(
            &self,
            recovery: __state_machine::ErrorRecovery<Self>,
        ) -> Self::Symbol {
            panic!("error recovery not enabled for this grammar")
        }

        fn reduce(
            &mut self,
            action: i8,
            start_location: Option<&Self::Location>,
            states: &mut alloc::vec::Vec<i8>,
            symbols: &mut alloc::vec::Vec<__state_machine::SymbolTriple<Self>>,
        ) -> Option<__state_machine::ParseResult<Self>> {
            __reduce(
                self.fields,
                self.input,
                action,
                start_location,
                states,
                symbols,
                core::marker::PhantomData::<(&(), T)>,
            )
        }

        fn simulate_reduce(&self, action: i8) -> __state_machine::SimulatedReduce<Self> {
            panic!("error recovery not enabled for this grammar")
        }
    }
    fn __token_to_integer<
        'input,
        T,
    >(
        __token: &Token<'input>,
        _: core::marker::PhantomData<(&'input (), T)>,
    ) -> Option<usize>
    {
        match *__token {
            Token(2, _) if true => Some(0),
            Token(3, _) if true => Some(1),
            Token(4, _) if true => Some(2),
            Token(5, _) if true => Some(3),
            Token(6, _) if true => Some(4),
            Token(7, _) if true => Some(5),
            Token(8, _) if true => Some(6),
            Token(9, _) if true => Some(7),
            Token(10, _) if true => Some(8),
            Token(11, _) if true => Some(9),
            Token(12, _) if true => Some(10),
            Token(13, _) if true => Some(11),
            Token(14, _) if true => Some(12),
            Token(15, _) if true => Some(13),
            Token(16, _) if true => Some(14),
            Token(17, _) if true => Some(15),
            Token(18, _) if true => Some(16),
            Token(19, _) if true => Some(17),
            Token(20, _) if true => Some(18),
            Token(21, _) if true => Some(19),
            Token(0, _) if true => Some(20),
            Token(1, _) if true => Some(21),
            _ => None,
        }
    }
    fn __token_to_symbol<
        'input,
        T,
    >(
        __token_index: usize,
        __token: Token<'input>,
        _: core::marker::PhantomData<(&'input (), T)>,
    ) -> __Symbol<'input, T>
    {
        match __token_index {
            0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 | 11 | 12 | 13 | 14 | 15 | 16 | 17 | 18 | 19 | 20 | 21 => match __token {
                Token(2, __tok0) | Token(3, __tok0) | Token(4, __tok0) | Token(5, __tok0) | Token(6, __tok0) | Token(7, __tok0) | Token(8, __tok0) | Token(9, __tok0) | Token(10, __tok0) | Token(11, __tok0) | Token(12, __tok0) | Token(13, __tok0) | Token(14, __tok0) | Token(15, __tok0) | Token(16, __tok0) | Token(17, __tok0) | Token(18, __tok0) | Token(19, __tok0) | Token(20, __tok0) | Token(21, __tok0) | Token(0, __tok0) | Token(1, __tok0) if true => __Symbol::Variant0(__tok0),
                _ => unreachable!(),
            },
            _ => unreachable!(),
        }
    }
    pub struct BooleanParser {
        builder: __lalrpop_util::lexer::MatcherBuilder,
        _priv: (),
    }

    impl BooleanParser {
        pub fn new() -> BooleanParser {
            let __builder = super::__intern_token::new_builder();
            BooleanParser {
                builder: __builder,
                _priv: (),
            }
        }

        #[allow(dead_code)]
        pub fn parse<
            'input,
            T,
        >(
            &self,
            fields: &Fields<T>,
            input: &'input str,
        ) -> Result<Boolean<T>, __lalrpop_util::ParseError<usize, Token<'input>, ConditionsError>>
        {
            let mut __tokens = self.builder.matcher(input);
            __state_machine::Parser::drive(
                __StateMachine {
                    fields,
                    input,
                    __phantom: core::marker::PhantomData::<(&(), T)>,
                },
                __tokens,
            )
        }
    }
    pub(crate) fn __reduce<
        'input,
        T,
    >(
        fields: &Fields<T>,
        input: &'input str,
        __action: i8,
        __lookahead_start: Option<&usize>,
        __states: &mut alloc::vec::Vec<i8>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input, T>,usize)>,
        _: core::marker::PhantomData<(&'input (), T)>,
    ) -> Option<Result<Boolean<T>,__lalrpop_util::ParseError<usize, Token<'input>, ConditionsError>>>
    {
        let (__pop_states, __nonterminal) = match __action {
            0 => {
                __reduce0(fields, input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), T)>)
            }
            1 => {
                __reduce1(fields, input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), T)>)
            }
            2 => {
                __reduce2(fields, input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), T)>)
            }
            3 => {
                __reduce3(fields, input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), T)>)
            }
            4 => {
                __reduce4(fields, input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), T)>)
            }
            5 => {
                // Condition = Expr, ConditionOpcode, Expr => ActionFn(6);
                assert!(__symbols.len() >= 3);
                let __sym2 = __pop_Variant5(__symbols);
                let __sym1 = __pop_Variant3(__symbols);
                let __sym0 = __pop_Variant5(__symbols);
                let __start = __sym0.0.clone();
                let __end = __sym2.2.clone();
                let __nt = match super::__action6::<T>(fields, input, __sym0, __sym1, __sym2) {
                    Ok(v) => v,
                    Err(e) => return Some(Err(e)),
                };
                __symbols.push((__start, __Symbol::Variant2(__nt), __end));
                (3, 1)
            }
            6 => {
                __reduce6(fields, input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), T)>)
            }
            7 => {
                __reduce7(fields, input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), T)>)
            }
            8 => {
                __reduce8(fields, input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), T)>)
            }
            9 => {
                __reduce9(fields, input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), T)>)
            }
            10 => {
                __reduce10(fields, input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), T)>)
            }
            11 => {
                __reduce11(fields, input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), T)>)
            }
            12 => {
                __reduce12(fields, input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), T)>)
            }
            13 => {
                __reduce13(fields, input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), T)>)
            }
            14 => {
                __reduce14(fields, input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), T)>)
            }
            15 => {
                __reduce15(fields, input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), T)>)
            }
            16 => {
                __reduce16(fields, input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), T)>)
            }
            17 => {
                __reduce17(fields, input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), T)>)
            }
            18 => {
                __reduce18(fields, input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), T)>)
            }
            19 => {
                __reduce19(fields, input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), T)>)
            }
            20 => {
                __reduce20(fields, input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), T)>)
            }
            21 => {
                __reduce21(fields, input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), T)>)
            }
            22 => {
                __reduce22(fields, input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), T)>)
            }
            23 => {
                __reduce23(fields, input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), T)>)
            }
            24 => {
                __reduce24(fields, input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), T)>)
            }
            25 => {
                __reduce25(fields, input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), T)>)
            }
            26 => {
                __reduce26(fields, input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), T)>)
            }
            27 => {
                // SetVariable = r#"[a-zA-Z_][a-zA-Z0-9_]*"# => ActionFn(34);
                let __sym0 = __pop_Variant0(__symbols);
                let __start = __sym0.0.clone();
                let __end = __sym0.2.clone();
                let __nt = match super::__action34::<T>(fields, input, __sym0) {
                    Ok(v) => v,
                    Err(e) => return Some(Err(e)),
                };
                __symbols.push((__start, __Symbol::Variant8(__nt), __end));
                (1, 9)
            }
            28 => {
                __reduce28(fields, input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), T)>)
            }
            29 => {
                __reduce29(fields, input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), T)>)
            }
            30 => {
                __reduce30(fields, input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), T)>)
            }
            31 => {
                __reduce31(fields, input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), T)>)
            }
            32 => {
                __reduce32(fields, input, __lookahead_start, __symbols, core::marker::PhantomData::<(&(), T)>)
            }
            33 => {
                // Variable = r#"[a-zA-Z_][a-zA-Z0-9_]*"# => ActionFn(33);
                let __sym0 = __pop_Variant0(__symbols);
                let __start = __sym0.0.clone();
                let __end = __sym0.2.clone();
                let __nt = match super::__action33::<T>(fields, input, __sym0) {
                    Ok(v) => v,
                    Err(e) => return Some(Err(e)),
                };
                __symbols.push((__start, __Symbol::Variant9(__nt), __end));
                (1, 11)
            }
            34 => {
                // __Boolean = Boolean => ActionFn(0);
                let __sym0 = __pop_Variant1(__symbols);
                let __start = __sym0.0.clone();
                let __end = __sym0.2.clone();
                let __nt = super::__action0::<T>(fields, input, __sym0);
                return Some(Ok(__nt));
            }
            _ => panic!("invalid action code {}", __action)
        };
        let __states_len = __states.len();
        __states.truncate(__states_len - __pop_states);
        let __state = *__states.last().unwrap();
        let __next_state = __goto(__state, __nonterminal);
        __states.push(__next_state);
        None
    }
    #[inline(never)]
    fn __symbol_type_mismatch() -> ! {
        panic!("symbol type mismatch")
    }
    fn __pop_Variant1<
      'input,
      T,
    >(
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input, T>,usize)>
    ) -> (usize, Boolean<T>, usize)
     {
        match __symbols.pop() {
            Some((__l, __Symbol::Variant1(__v), __r)) => (__l, __v, __r),
            _ => __symbol_type_mismatch()
        }
    }
    fn __pop_Variant5<
      'input,
      T,
    >(
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input, T>,usize)>
    ) -> (usize, Box<Expr<T>>, usize)
     {
        match __symbols.pop() {
            Some((__l, __Symbol::Variant5(__v), __r)) => (__l, __v, __r),
            _ => __symbol_type_mismatch()
        }
    }
    fn __pop_Variant2<
      'input,
      T,
    >(
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input, T>,usize)>
    ) -> (usize, Condition<T>, usize)
     {
        match __symbols.pop() {
            Some((__l, __Symbol::Variant2(__v), __r)) => (__l, __v, __r),
            _ => __symbol_type_mismatch()
        }
    }
    fn __pop_Variant3<
      'input,
      T,
    >(
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input, T>,usize)>
    ) -> (usize, ConditionOpcode, usize)
     {
        match __symbols.pop() {
            Some((__l, __Symbol::Variant3(__v), __r)) => (__l, __v, __r),
            _ => __symbol_type_mismatch()
        }
    }
    fn __pop_Variant8<
      'input,
      T,
    >(
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input, T>,usize)>
    ) -> (usize, FieldRef<T, HashSet<String>>, usize)
     {
        match __symbols.pop() {
            Some((__l, __Symbol::Variant8(__v), __r)) => (__l, __v, __r),
            _ => __symbol_type_mismatch()
        }
    }
    fn __pop_Variant6<
      'input,
      T,
    >(
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input, T>,usize)>
    ) -> (usize, Opcode, usize)
     {
        match __symbols.pop() {
            Some((__l, __Symbol::Variant6(__v), __r)) => (__l, __v, __r),
            _ => __symbol_type_mismatch()
        }
    }
    fn __pop_Variant9<
      'input,
      T,
    >(
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input, T>,usize)>
    ) -> (usize, Reference<T>, usize)
     {
        match __symbols.pop() {
            Some((__l, __Symbol::Variant9(__v), __r)) => (__l, __v, __r),
            _ => __symbol_type_mismatch()
        }
    }
    fn __pop_Variant4<
      'input,
      T,
    >(
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input, T>,usize)>
    ) -> (usize, String, usize)
     {
        match __symbols.pop() {
            Some((__l, __Symbol::Variant4(__v), __r)) => (__l, __v, __r),
            _ => __symbol_type_mismatch()
        }
    }
    fn __pop_Variant7<
      'input,
      T,
    >(
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input, T>,usize)>
    ) -> (usize, i32, usize)
     {
        match __symbols.pop() {
            Some((__l, __Symbol::Variant7(__v), __r)) => (__l, __v, __r),
            _ => __symbol_type_mismatch()
        }
    }
    fn __pop_Variant0<
      'input,
      T,
    >(
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input, T>,usize)>
    ) -> (usize, &'input str, usize)
     {
        match __symbols.pop() {
            Some((__l, __Symbol::Variant0(__v), __r)) => (__l, __v, __r),
            _ => __symbol_type_mismatch()
        }
    }
    pub(crate) fn __reduce0<
        'input,
        T,
    >(
        fields: &Fields<T>,
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input, T>,usize)>,
        _: core::marker::PhantomData<(&'input (), T)>,
    ) -> (usize, usize)
    {
        // Boolean = "not", Condition => ActionFn(1);
        assert!(__symbols.len() >= 2);
        let __sym1 = __pop_Variant2(__symbols);
        let __sym0 = __pop_Variant0(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym1.2.clone();
        let __nt = super::__action1::<T>(fields, input, __sym0, __sym1);
        __symbols.push((__start, __Symbol::Variant1(__nt), __end));
        (2, 0)
    }
    pub(crate) fn __reduce1<
        'input,
        T,
    >(
        fields: &Fields<T>,
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input, T>,usize)>,
        _: core::marker::PhantomData<(&'input (), T)>,
    ) -> (usize, usize)
    {
        // Boolean = Boolean, "and", Condition => ActionFn(2);
        assert!(__symbols.len() >= 3);
        let __sym2 = __pop_Variant2(__symbols);
        let __sym1 = __pop_Variant0(__symbols);
        let __sym0 = __pop_Variant1(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym2.2.clone();
        let __nt = super::__action2::<T>(fields, input, __sym0, __sym1, __sym2);
        __symbols.push((__start, __Symbol::Variant1(__nt), __end));
        (3, 0)
    }
    pub(crate) fn __reduce2<
        'input,
        T,
    >(
        fields: &Fields<T>,
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input, T>,usize)>,
        _: core::marker::PhantomData<(&'input (), T)>,
    ) -> (usize, usize)
    {
        // Boolean = Boolean, "or", Condition => ActionFn(3);
        assert!(__symbols.len() >= 3);
        let __sym2 = __pop_Variant2(__symbols);
        let __sym1 = __pop_Variant0(__symbols);
        let __sym0 = __pop_Variant1(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym2.2.clone();
        let __nt = super::__action3::<T>(fields, input, __sym0, __sym1, __sym2);
        __symbols.push((__start, __Symbol::Variant1(__nt), __end));
        (3, 0)
    }
    pub(crate) fn __reduce3<
        'input,
        T,
    >(
        fields: &Fields<T>,
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input, T>,usize)>,
        _: core::marker::PhantomData<(&'input (), T)>,
    ) -> (usize, usize)
    {
        // Boolean = Condition => ActionFn(4);
        let __sym0 = __pop_Variant2(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action4::<T>(fields, input, __sym0);
        __symbols.push((__start, __Symbol::Variant1(__nt), __end));
        (1, 0)
    }
    pub(crate) fn __reduce4<
        'input,
        T,
    >(
        fields: &Fields<T>,
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input, T>,usize)>,
        _: core::marker::PhantomData<(&'input (), T)>,
    ) -> (usize, usize)
    {
        // Condition = "(", Boolean, ")" => ActionFn(5);
        assert!(__symbols.len() >= 3);
        let __sym2 = __pop_Variant0(__symbols);
        let __sym1 = __pop_Variant1(__symbols);
        let __sym0 = __pop_Variant0(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym2.2.clone();
        let __nt = super::__action5::<T>(fields, input, __sym0, __sym1, __sym2);
        __symbols.push((__start, __Symbol::Variant2(__nt), __end));
        (3, 1)
    }
    pub(crate) fn __reduce6<
        'input,
        T,
    >(
        fields: &Fields<T>,
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input, T>,usize)>,
        _: core::marker::PhantomData<(&'input (), T)>,
    ) -> (usize, usize)
    {
        // Condition = "'", ConstantString, "'", "in", SetVariable => ActionFn(7);
        assert!(__symbols.len() >= 5);
        let __sym4 = __pop_Variant8(__symbols);
        let __sym3 = __pop_Variant0(__symbols);
        let __sym2 = __pop_Variant0(__symbols);
        let __sym1 = __pop_Variant4(__symbols);
        let __sym0 = __pop_Variant0(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym4.2.clone();
        let __nt = super::__action7::<T>(fields, input, __sym0, __sym1, __sym2, __sym3, __sym4);
        __symbols.push((__start, __Symbol::Variant2(__nt), __end));
        (5, 1)
    }
    pub(crate) fn __reduce7<
        'input,
        T,
    >(
        fields: &Fields<T>,
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input, T>,usize)>,
        _: core::marker::PhantomData<(&'input (), T)>,
    ) -> (usize, usize)
    {
        // Condition = "\"", ConstantString, "\"", "in", SetVariable => ActionFn(8);
        assert!(__symbols.len() >= 5);
        let __sym4 = __pop_Variant8(__symbols);
        let __sym3 = __pop_Variant0(__symbols);
        let __sym2 = __pop_Variant0(__symbols);
        let __sym1 = __pop_Variant4(__symbols);
        let __sym0 = __pop_Variant0(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym4.2.clone();
        let __nt = super::__action8::<T>(fields, input, __sym0, __sym1, __sym2, __sym3, __sym4);
        __symbols.push((__start, __Symbol::Variant2(__nt), __end));
        (5, 1)
    }
    pub(crate) fn __reduce8<
        'input,
        T,
    >(
        fields: &Fields<T>,
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input, T>,usize)>,
        _: core::marker::PhantomData<(&'input (), T)>,
    ) -> (usize, usize)
    {
        // Condition = "'", ConstantString, "'", "not in", SetVariable => ActionFn(9);
        assert!(__symbols.len() >= 5);
        let __sym4 = __pop_Variant8(__symbols);
        let __sym3 = __pop_Variant0(__symbols);
        let __sym2 = __pop_Variant0(__symbols);
        let __sym1 = __pop_Variant4(__symbols);
        let __sym0 = __pop_Variant0(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym4.2.clone();
        let __nt = super::__action9::<T>(fields, input, __sym0, __sym1, __sym2, __sym3, __sym4);
        __symbols.push((__start, __Symbol::Variant2(__nt), __end));
        (5, 1)
    }
    pub(crate) fn __reduce9<
        'input,
        T,
    >(
        fields: &Fields<T>,
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input, T>,usize)>,
        _: core::marker::PhantomData<(&'input (), T)>,
    ) -> (usize, usize)
    {
        // Condition = "\"", ConstantString, "\"", "not in", SetVariable => ActionFn(10);
        assert!(__symbols.len() >= 5);
        let __sym4 = __pop_Variant8(__symbols);
        let __sym3 = __pop_Variant0(__symbols);
        let __sym2 = __pop_Variant0(__symbols);
        let __sym1 = __pop_Variant4(__symbols);
        let __sym0 = __pop_Variant0(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym4.2.clone();
        let __nt = super::__action10::<T>(fields, input, __sym0, __sym1, __sym2, __sym3, __sym4);
        __symbols.push((__start, __Symbol::Variant2(__nt), __end));
        (5, 1)
    }
    pub(crate) fn __reduce10<
        'input,
        T,
    >(
        fields: &Fields<T>,
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input, T>,usize)>,
        _: core::marker::PhantomData<(&'input (), T)>,
    ) -> (usize, usize)
    {
        // ConditionOpcode = "==" => ActionFn(11);
        let __sym0 = __pop_Variant0(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action11::<T>(fields, input, __sym0);
        __symbols.push((__start, __Symbol::Variant3(__nt), __end));
        (1, 2)
    }
    pub(crate) fn __reduce11<
        'input,
        T,
    >(
        fields: &Fields<T>,
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input, T>,usize)>,
        _: core::marker::PhantomData<(&'input (), T)>,
    ) -> (usize, usize)
    {
        // ConditionOpcode = "!=" => ActionFn(12);
        let __sym0 = __pop_Variant0(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action12::<T>(fields, input, __sym0);
        __symbols.push((__start, __Symbol::Variant3(__nt), __end));
        (1, 2)
    }
    pub(crate) fn __reduce12<
        'input,
        T,
    >(
        fields: &Fields<T>,
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input, T>,usize)>,
        _: core::marker::PhantomData<(&'input (), T)>,
    ) -> (usize, usize)
    {
        // ConditionOpcode = "<" => ActionFn(13);
        let __sym0 = __pop_Variant0(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action13::<T>(fields, input, __sym0);
        __symbols.push((__start, __Symbol::Variant3(__nt), __end));
        (1, 2)
    }
    pub(crate) fn __reduce13<
        'input,
        T,
    >(
        fields: &Fields<T>,
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input, T>,usize)>,
        _: core::marker::PhantomData<(&'input (), T)>,
    ) -> (usize, usize)
    {
        // ConditionOpcode = "<=" => ActionFn(14);
        let __sym0 = __pop_Variant0(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action14::<T>(fields, input, __sym0);
        __symbols.push((__start, __Symbol::Variant3(__nt), __end));
        (1, 2)
    }
    pub(crate) fn __reduce14<
        'input,
        T,
    >(
        fields: &Fields<T>,
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input, T>,usize)>,
        _: core::marker::PhantomData<(&'input (), T)>,
    ) -> (usize, usize)
    {
        // ConditionOpcode = ">" => ActionFn(15);
        let __sym0 = __pop_Variant0(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action15::<T>(fields, input, __sym0);
        __symbols.push((__start, __Symbol::Variant3(__nt), __end));
        (1, 2)
    }
    pub(crate) fn __reduce15<
        'input,
        T,
    >(
        fields: &Fields<T>,
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input, T>,usize)>,
        _: core::marker::PhantomData<(&'input (), T)>,
    ) -> (usize, usize)
    {
        // ConditionOpcode = ">=" => ActionFn(16);
        let __sym0 = __pop_Variant0(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action16::<T>(fields, input, __sym0);
        __symbols.push((__start, __Symbol::Variant3(__nt), __end));
        (1, 2)
    }
    pub(crate) fn __reduce16<
        'input,
        T,
    >(
        fields: &Fields<T>,
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input, T>,usize)>,
        _: core::marker::PhantomData<(&'input (), T)>,
    ) -> (usize, usize)
    {
        // ConstantString = r#"[a-zA-Z_][a-zA-Z0-9_]*"# => ActionFn(32);
        let __sym0 = __pop_Variant0(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action32::<T>(fields, input, __sym0);
        __symbols.push((__start, __Symbol::Variant4(__nt), __end));
        (1, 3)
    }
    pub(crate) fn __reduce17<
        'input,
        T,
    >(
        fields: &Fields<T>,
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input, T>,usize)>,
        _: core::marker::PhantomData<(&'input (), T)>,
    ) -> (usize, usize)
    {
        // Expr = Expr, ExprOp, Factor => ActionFn(17);
        assert!(__symbols.len() >= 3);
        let __sym2 = __pop_Variant5(__symbols);
        let __sym1 = __pop_Variant6(__symbols);
        let __sym0 = __pop_Variant5(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym2.2.clone();
        let __nt = super::__action17::<T>(fields, input, __sym0, __sym1, __sym2);
        __symbols.push((__start, __Symbol::Variant5(__nt), __end));
        (3, 4)
    }
    pub(crate) fn __reduce18<
        'input,
        T,
    >(
        fields: &Fields<T>,
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input, T>,usize)>,
        _: core::marker::PhantomData<(&'input (), T)>,
    ) -> (usize, usize)
    {
        // Expr = Factor => ActionFn(18);
        let __sym0 = __pop_Variant5(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action18::<T>(fields, input, __sym0);
        __symbols.push((__start, __Symbol::Variant5(__nt), __end));
        (1, 4)
    }
    pub(crate) fn __reduce19<
        'input,
        T,
    >(
        fields: &Fields<T>,
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input, T>,usize)>,
        _: core::marker::PhantomData<(&'input (), T)>,
    ) -> (usize, usize)
    {
        // ExprOp = "+" => ActionFn(19);
        let __sym0 = __pop_Variant0(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action19::<T>(fields, input, __sym0);
        __symbols.push((__start, __Symbol::Variant6(__nt), __end));
        (1, 5)
    }
    pub(crate) fn __reduce20<
        'input,
        T,
    >(
        fields: &Fields<T>,
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input, T>,usize)>,
        _: core::marker::PhantomData<(&'input (), T)>,
    ) -> (usize, usize)
    {
        // ExprOp = "-" => ActionFn(20);
        let __sym0 = __pop_Variant0(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action20::<T>(fields, input, __sym0);
        __symbols.push((__start, __Symbol::Variant6(__nt), __end));
        (1, 5)
    }
    pub(crate) fn __reduce21<
        'input,
        T,
    >(
        fields: &Fields<T>,
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input, T>,usize)>,
        _: core::marker::PhantomData<(&'input (), T)>,
    ) -> (usize, usize)
    {
        // Factor = Factor, FactorOp, Term => ActionFn(21);
        assert!(__symbols.len() >= 3);
        let __sym2 = __pop_Variant5(__symbols);
        let __sym1 = __pop_Variant6(__symbols);
        let __sym0 = __pop_Variant5(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym2.2.clone();
        let __nt = super::__action21::<T>(fields, input, __sym0, __sym1, __sym2);
        __symbols.push((__start, __Symbol::Variant5(__nt), __end));
        (3, 6)
    }
    pub(crate) fn __reduce22<
        'input,
        T,
    >(
        fields: &Fields<T>,
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input, T>,usize)>,
        _: core::marker::PhantomData<(&'input (), T)>,
    ) -> (usize, usize)
    {
        // Factor = Term => ActionFn(22);
        let __sym0 = __pop_Variant5(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action22::<T>(fields, input, __sym0);
        __symbols.push((__start, __Symbol::Variant5(__nt), __end));
        (1, 6)
    }
    pub(crate) fn __reduce23<
        'input,
        T,
    >(
        fields: &Fields<T>,
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input, T>,usize)>,
        _: core::marker::PhantomData<(&'input (), T)>,
    ) -> (usize, usize)
    {
        // FactorOp = "*" => ActionFn(23);
        let __sym0 = __pop_Variant0(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action23::<T>(fields, input, __sym0);
        __symbols.push((__start, __Symbol::Variant6(__nt), __end));
        (1, 7)
    }
    pub(crate) fn __reduce24<
        'input,
        T,
    >(
        fields: &Fields<T>,
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input, T>,usize)>,
        _: core::marker::PhantomData<(&'input (), T)>,
    ) -> (usize, usize)
    {
        // FactorOp = "/" => ActionFn(24);
        let __sym0 = __pop_Variant0(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action24::<T>(fields, input, __sym0);
        __symbols.push((__start, __Symbol::Variant6(__nt), __end));
        (1, 7)
    }
    pub(crate) fn __reduce25<
        'input,
        T,
    >(
        fields: &Fields<T>,
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input, T>,usize)>,
        _: core::marker::PhantomData<(&'input (), T)>,
    ) -> (usize, usize)
    {
        // FactorOp = "%" => ActionFn(25);
        let __sym0 = __pop_Variant0(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action25::<T>(fields, input, __sym0);
        __symbols.push((__start, __Symbol::Variant6(__nt), __end));
        (1, 7)
    }
    pub(crate) fn __reduce26<
        'input,
        T,
    >(
        fields: &Fields<T>,
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input, T>,usize)>,
        _: core::marker::PhantomData<(&'input (), T)>,
    ) -> (usize, usize)
    {
        // Num = r#"[0-9]+"# => ActionFn(31);
        let __sym0 = __pop_Variant0(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action31::<T>(fields, input, __sym0);
        __symbols.push((__start, __Symbol::Variant7(__nt), __end));
        (1, 8)
    }
    pub(crate) fn __reduce28<
        'input,
        T,
    >(
        fields: &Fields<T>,
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input, T>,usize)>,
        _: core::marker::PhantomData<(&'input (), T)>,
    ) -> (usize, usize)
    {
        // Term = Num => ActionFn(26);
        let __sym0 = __pop_Variant7(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action26::<T>(fields, input, __sym0);
        __symbols.push((__start, __Symbol::Variant5(__nt), __end));
        (1, 10)
    }
    pub(crate) fn __reduce29<
        'input,
        T,
    >(
        fields: &Fields<T>,
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input, T>,usize)>,
        _: core::marker::PhantomData<(&'input (), T)>,
    ) -> (usize, usize)
    {
        // Term = "'", ConstantString, "'" => ActionFn(27);
        assert!(__symbols.len() >= 3);
        let __sym2 = __pop_Variant0(__symbols);
        let __sym1 = __pop_Variant4(__symbols);
        let __sym0 = __pop_Variant0(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym2.2.clone();
        let __nt = super::__action27::<T>(fields, input, __sym0, __sym1, __sym2);
        __symbols.push((__start, __Symbol::Variant5(__nt), __end));
        (3, 10)
    }
    pub(crate) fn __reduce30<
        'input,
        T,
    >(
        fields: &Fields<T>,
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input, T>,usize)>,
        _: core::marker::PhantomData<(&'input (), T)>,
    ) -> (usize, usize)
    {
        // Term = "\"", ConstantString, "\"" => ActionFn(28);
        assert!(__symbols.len() >= 3);
        let __sym2 = __pop_Variant0(__symbols);
        let __sym1 = __pop_Variant4(__symbols);
        let __sym0 = __pop_Variant0(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym2.2.clone();
        let __nt = super::__action28::<T>(fields, input, __sym0, __sym1, __sym2);
        __symbols.push((__start, __Symbol::Variant5(__nt), __end));
        (3, 10)
    }
    pub(crate) fn __reduce31<
        'input,
        T,
    >(
        fields: &Fields<T>,
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input, T>,usize)>,
        _: core::marker::PhantomData<(&'input (), T)>,
    ) -> (usize, usize)
    {
        // Term = Variable => ActionFn(29);
        let __sym0 = __pop_Variant9(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym0.2.clone();
        let __nt = super::__action29::<T>(fields, input, __sym0);
        __symbols.push((__start, __Symbol::Variant5(__nt), __end));
        (1, 10)
    }
    pub(crate) fn __reduce32<
        'input,
        T,
    >(
        fields: &Fields<T>,
        input: &'input str,
        __lookahead_start: Option<&usize>,
        __symbols: &mut alloc::vec::Vec<(usize,__Symbol<'input, T>,usize)>,
        _: core::marker::PhantomData<(&'input (), T)>,
    ) -> (usize, usize)
    {
        // Term = "(", Expr, ")" => ActionFn(30);
        assert!(__symbols.len() >= 3);
        let __sym2 = __pop_Variant0(__symbols);
        let __sym1 = __pop_Variant5(__symbols);
        let __sym0 = __pop_Variant0(__symbols);
        let __start = __sym0.0.clone();
        let __end = __sym2.2.clone();
        let __nt = super::__action30::<T>(fields, input, __sym0, __sym1, __sym2);
        __symbols.push((__start, __Symbol::Variant5(__nt), __end));
        (3, 10)
    }
}
pub use self::__parse__Boolean::BooleanParser;
#[cfg_attr(rustfmt, rustfmt_skip)]
mod __intern_token {
    #![allow(unused_imports)]
    use std::str::FromStr;
    use std::collections::HashSet;
    use crate::scheduling::ast::{Expr, Opcode, Condition, ConditionOpcode, Boolean, Fields, TypeError, ConditionsError, Reference};
    use field_ref::FieldRef;
    use lalrpop_util::ParseError;
    #[allow(unused_extern_crates)]
    extern crate lalrpop_util as __lalrpop_util;
    #[allow(unused_imports)]
    use self::__lalrpop_util::state_machine as __state_machine;
    extern crate core;
    extern crate alloc;
    pub fn new_builder() -> __lalrpop_util::lexer::MatcherBuilder {
        let __strs: &[(&str, bool)] = &[
            ("^([0-9]+)", false),
            ("^([A-Z_a-z][0-9A-Z_a-z]*)", false),
            ("^(!=)", false),
            ("^(\")", false),
            ("^(%)", false),
            ("^(')", false),
            ("^(\\()", false),
            ("^(\\))", false),
            ("^(\\*)", false),
            ("^(\\+)", false),
            ("^(\\-)", false),
            ("^(/)", false),
            ("^(<)", false),
            ("^(<=)", false),
            ("^(==)", false),
            ("^(>)", false),
            ("^(>=)", false),
            ("^(and)", false),
            ("^(in)", false),
            ("^(not)", false),
            ("^(not in)", false),
            ("^(or)", false),
            (r"^(\s*)", true),
        ];
        __lalrpop_util::lexer::MatcherBuilder::new(__strs.iter().copied()).unwrap()
    }
}
pub(crate) use self::__lalrpop_util::lexer::Token;

#[allow(unused_variables)]
fn __action0<
    'input,
    T,
>(
    fields: &Fields<T>,
    input: &'input str,
    (_, __0, _): (usize, Boolean<T>, usize),
) -> Boolean<T>
{
    __0
}

#[allow(unused_variables)]
fn __action1<
    'input,
    T,
>(
    fields: &Fields<T>,
    input: &'input str,
    (_, _, _): (usize, &'input str, usize),
    (_, __0, _): (usize, Condition<T>, usize),
) -> Boolean<T>
{
    Boolean::<T>::Not(__0)
}

#[allow(unused_variables)]
fn __action2<
    'input,
    T,
>(
    fields: &Fields<T>,
    input: &'input str,
    (_, left, _): (usize, Boolean<T>, usize),
    (_, _, _): (usize, &'input str, usize),
    (_, right, _): (usize, Condition<T>, usize),
) -> Boolean<T>
{
    Boolean::<T>::And(Box::new(left), right)
}

#[allow(unused_variables)]
fn __action3<
    'input,
    T,
>(
    fields: &Fields<T>,
    input: &'input str,
    (_, left, _): (usize, Boolean<T>, usize),
    (_, _, _): (usize, &'input str, usize),
    (_, right, _): (usize, Condition<T>, usize),
) -> Boolean<T>
{
    Boolean::<T>::Or(Box::new(left), right)
}

#[allow(unused_variables)]
fn __action4<
    'input,
    T,
>(
    fields: &Fields<T>,
    input: &'input str,
    (_, __0, _): (usize, Condition<T>, usize),
) -> Boolean<T>
{
    Boolean::<T>::Cond(__0)
}

#[allow(unused_variables)]
fn __action5<
    'input,
    T,
>(
    fields: &Fields<T>,
    input: &'input str,
    (_, _, _): (usize, &'input str, usize),
    (_, __0, _): (usize, Boolean<T>, usize),
    (_, _, _): (usize, &'input str, usize),
) -> Condition<T>
{
    Condition::<T>::Boolean(Box::new(__0))
}

#[allow(unused_variables)]
fn __action6<
    'input,
    T,
>(
    fields: &Fields<T>,
    input: &'input str,
    (_, l, _): (usize, Box<Expr<T>>, usize),
    (_, op, _): (usize, ConditionOpcode, usize),
    (_, r, _): (usize, Box<Expr<T>>, usize),
) -> Result<Condition<T>,__lalrpop_util::ParseError<usize,Token<'input>,ConditionsError>>
{
    {
        let f = || -> Result<Condition<T>, ConditionsError> {
            let l_type = l.type_of()?;
            let r_type = r.type_of()?;

            if l_type == r_type {
                Ok(Condition::<T>::Op(l, op, r))
            } else {
                Err(TypeError::TypeMismatch(l_type, r_type).into())
            }
        };
        let result = f();
        result.map_err(|e| e.into())
    }
}

#[allow(unused_variables)]
fn __action7<
    'input,
    T,
>(
    fields: &Fields<T>,
    input: &'input str,
    (_, _, _): (usize, &'input str, usize),
    (_, __0, _): (usize, String, usize),
    (_, _, _): (usize, &'input str, usize),
    (_, _, _): (usize, &'input str, usize),
    (_, __1, _): (usize, FieldRef<T, HashSet<String>>, usize),
) -> Condition<T>
{
    Condition::<T>::In(__0, __1)
}

#[allow(unused_variables)]
fn __action8<
    'input,
    T,
>(
    fields: &Fields<T>,
    input: &'input str,
    (_, _, _): (usize, &'input str, usize),
    (_, __0, _): (usize, String, usize),
    (_, _, _): (usize, &'input str, usize),
    (_, _, _): (usize, &'input str, usize),
    (_, __1, _): (usize, FieldRef<T, HashSet<String>>, usize),
) -> Condition<T>
{
    Condition::<T>::In(__0, __1)
}

#[allow(unused_variables)]
fn __action9<
    'input,
    T,
>(
    fields: &Fields<T>,
    input: &'input str,
    (_, _, _): (usize, &'input str, usize),
    (_, __0, _): (usize, String, usize),
    (_, _, _): (usize, &'input str, usize),
    (_, _, _): (usize, &'input str, usize),
    (_, __1, _): (usize, FieldRef<T, HashSet<String>>, usize),
) -> Condition<T>
{
    Condition::<T>::NotIn(__0, __1)
}

#[allow(unused_variables)]
fn __action10<
    'input,
    T,
>(
    fields: &Fields<T>,
    input: &'input str,
    (_, _, _): (usize, &'input str, usize),
    (_, __0, _): (usize, String, usize),
    (_, _, _): (usize, &'input str, usize),
    (_, _, _): (usize, &'input str, usize),
    (_, __1, _): (usize, FieldRef<T, HashSet<String>>, usize),
) -> Condition<T>
{
    Condition::<T>::NotIn(__0, __1)
}

#[allow(unused_variables)]
fn __action11<
    'input,
    T,
>(
    fields: &Fields<T>,
    input: &'input str,
    (_, __0, _): (usize, &'input str, usize),
) -> ConditionOpcode
{
    ConditionOpcode::Eq
}

#[allow(unused_variables)]
fn __action12<
    'input,
    T,
>(
    fields: &Fields<T>,
    input: &'input str,
    (_, __0, _): (usize, &'input str, usize),
) -> ConditionOpcode
{
    ConditionOpcode::NotEq
}

#[allow(unused_variables)]
fn __action13<
    'input,
    T,
>(
    fields: &Fields<T>,
    input: &'input str,
    (_, __0, _): (usize, &'input str, usize),
) -> ConditionOpcode
{
    ConditionOpcode::Lt
}

#[allow(unused_variables)]
fn __action14<
    'input,
    T,
>(
    fields: &Fields<T>,
    input: &'input str,
    (_, __0, _): (usize, &'input str, usize),
) -> ConditionOpcode
{
    ConditionOpcode::Lte
}

#[allow(unused_variables)]
fn __action15<
    'input,
    T,
>(
    fields: &Fields<T>,
    input: &'input str,
    (_, __0, _): (usize, &'input str, usize),
) -> ConditionOpcode
{
    ConditionOpcode::Gt
}

#[allow(unused_variables)]
fn __action16<
    'input,
    T,
>(
    fields: &Fields<T>,
    input: &'input str,
    (_, __0, _): (usize, &'input str, usize),
) -> ConditionOpcode
{
    ConditionOpcode::Gte
}

#[allow(unused_variables)]
fn __action17<
    'input,
    T,
>(
    fields: &Fields<T>,
    input: &'input str,
    (_, __0, _): (usize, Box<Expr<T>>, usize),
    (_, __1, _): (usize, Opcode, usize),
    (_, __2, _): (usize, Box<Expr<T>>, usize),
) -> Box<Expr<T>>
{
    Box::new(Expr::<T>::Op(__0, __1, __2))
}

#[allow(unused_variables)]
fn __action18<
    'input,
    T,
>(
    fields: &Fields<T>,
    input: &'input str,
    (_, __0, _): (usize, Box<Expr<T>>, usize),
) -> Box<Expr<T>>
{
    __0
}

#[allow(unused_variables)]
fn __action19<
    'input,
    T,
>(
    fields: &Fields<T>,
    input: &'input str,
    (_, __0, _): (usize, &'input str, usize),
) -> Opcode
{
    Opcode::Add
}

#[allow(unused_variables)]
fn __action20<
    'input,
    T,
>(
    fields: &Fields<T>,
    input: &'input str,
    (_, __0, _): (usize, &'input str, usize),
) -> Opcode
{
    Opcode::Sub
}

#[allow(unused_variables)]
fn __action21<
    'input,
    T,
>(
    fields: &Fields<T>,
    input: &'input str,
    (_, __0, _): (usize, Box<Expr<T>>, usize),
    (_, __1, _): (usize, Opcode, usize),
    (_, __2, _): (usize, Box<Expr<T>>, usize),
) -> Box<Expr<T>>
{
    Box::new(Expr::<T>::Op(__0, __1, __2))
}

#[allow(unused_variables)]
fn __action22<
    'input,
    T,
>(
    fields: &Fields<T>,
    input: &'input str,
    (_, __0, _): (usize, Box<Expr<T>>, usize),
) -> Box<Expr<T>>
{
    __0
}

#[allow(unused_variables)]
fn __action23<
    'input,
    T,
>(
    fields: &Fields<T>,
    input: &'input str,
    (_, __0, _): (usize, &'input str, usize),
) -> Opcode
{
    Opcode::Mul
}

#[allow(unused_variables)]
fn __action24<
    'input,
    T,
>(
    fields: &Fields<T>,
    input: &'input str,
    (_, __0, _): (usize, &'input str, usize),
) -> Opcode
{
    Opcode::Div
}

#[allow(unused_variables)]
fn __action25<
    'input,
    T,
>(
    fields: &Fields<T>,
    input: &'input str,
    (_, __0, _): (usize, &'input str, usize),
) -> Opcode
{
    Opcode::Remainder
}

#[allow(unused_variables)]
fn __action26<
    'input,
    T,
>(
    fields: &Fields<T>,
    input: &'input str,
    (_, __0, _): (usize, i32, usize),
) -> Box<Expr<T>>
{
    Box::new(Expr::<T>::Number(__0))
}

#[allow(unused_variables)]
fn __action27<
    'input,
    T,
>(
    fields: &Fields<T>,
    input: &'input str,
    (_, _, _): (usize, &'input str, usize),
    (_, __0, _): (usize, String, usize),
    (_, _, _): (usize, &'input str, usize),
) -> Box<Expr<T>>
{
    Box::new(Expr::<T>::String(__0))
}

#[allow(unused_variables)]
fn __action28<
    'input,
    T,
>(
    fields: &Fields<T>,
    input: &'input str,
    (_, _, _): (usize, &'input str, usize),
    (_, __0, _): (usize, String, usize),
    (_, _, _): (usize, &'input str, usize),
) -> Box<Expr<T>>
{
    Box::new(Expr::<T>::String(__0))
}

#[allow(unused_variables)]
fn __action29<
    'input,
    T,
>(
    fields: &Fields<T>,
    input: &'input str,
    (_, __0, _): (usize, Reference<T>, usize),
) -> Box<Expr<T>>
{
    Box::new(Expr::<T>::Variable(__0))
}

#[allow(unused_variables)]
fn __action30<
    'input,
    T,
>(
    fields: &Fields<T>,
    input: &'input str,
    (_, _, _): (usize, &'input str, usize),
    (_, __0, _): (usize, Box<Expr<T>>, usize),
    (_, _, _): (usize, &'input str, usize),
) -> Box<Expr<T>>
{
    __0
}

#[allow(unused_variables)]
fn __action31<
    'input,
    T,
>(
    fields: &Fields<T>,
    input: &'input str,
    (_, __0, _): (usize, &'input str, usize),
) -> i32
{
    i32::from_str(__0).unwrap()
}

#[allow(unused_variables)]
fn __action32<
    'input,
    T,
>(
    fields: &Fields<T>,
    input: &'input str,
    (_, __0, _): (usize, &'input str, usize),
) -> String
{
    String::from(__0)
}

#[allow(unused_variables)]
fn __action33<
    'input,
    T,
>(
    fields: &Fields<T>,
    input: &'input str,
    (_, r, _): (usize, &'input str, usize),
) -> Result<Reference<T>,__lalrpop_util::ParseError<usize,Token<'input>,ConditionsError>>
{
    match fields.any.get(r) {
            Some(v) => Ok((*v).clone()),
            None => Err(ParseError::User{error: ConditionsError::FieldNotFound(r.to_string())}),
        }
}

#[allow(unused_variables)]
fn __action34<
    'input,
    T,
>(
    fields: &Fields<T>,
    input: &'input str,
    (_, r, _): (usize, &'input str, usize),
) -> Result<FieldRef<T, HashSet<String>>,__lalrpop_util::ParseError<usize,Token<'input>,ConditionsError>>
{
    match fields.sets.get(r) {
            Some(v) => Ok(*v),
            None => Err(ParseError::User{error: ConditionsError::FieldNotFound(r.to_string())}),
        }
}

pub trait __ToTriple<'input, T, >
{
    fn to_triple(value: Self) -> Result<(usize,Token<'input>,usize), __lalrpop_util::ParseError<usize, Token<'input>, ConditionsError>>;
}

impl<'input, T, > __ToTriple<'input, T, > for (usize, Token<'input>, usize)
{
    fn to_triple(value: Self) -> Result<(usize,Token<'input>,usize), __lalrpop_util::ParseError<usize, Token<'input>, ConditionsError>> {
        Ok(value)
    }
}
impl<'input, T, > __ToTriple<'input, T, > for Result<(usize, Token<'input>, usize), ConditionsError>
{
    fn to_triple(value: Self) -> Result<(usize,Token<'input>,usize), __lalrpop_util::ParseError<usize, Token<'input>, ConditionsError>> {
        match value {
            Ok(v) => Ok(v),
            Err(error) => Err(__lalrpop_util::ParseError::User { error }),
        }
    }
}
