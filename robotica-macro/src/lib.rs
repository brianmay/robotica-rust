mod duration;
mod naive_time;
mod time_delta;

use proc_macro::{Delimiter, Group, Ident, Literal, Punct, Spacing, Span, TokenStream, TokenTree};
use syn::{punctuated::Punctuated, LitInt, Token};

fn error_from_string(msg: String) -> TokenStream {
    let error = vec![
        TokenTree::Ident(Ident::new("compile_error", Span::mixed_site())),
        TokenTree::Punct(Punct::new('!', Spacing::Alone)),
        TokenTree::Group(Group::new(
            Delimiter::Parenthesis,
            [TokenTree::Literal(Literal::string(&msg))]
                .into_iter()
                .collect(),
        )),
    ];
    TokenStream::from_iter(error)
}

macro_rules! error {
    ( $($stuff: expr),+) => {
        $crate::error_from_string(format!($($stuff),+))
    }
}
pub(crate) use error;

#[proc_macro]
pub fn time_delta_constant(input: TokenStream) -> TokenStream {
    if let Ok(value) = syn::parse::Parser::parse(
        Punctuated::<LitInt, Token![:]>::parse_terminated,
        input.clone(),
    ) {
        return time_delta::hms_to_seconds(value);
    };

    time_delta::units_to_seconds(input)
}

#[proc_macro]
pub fn duration_constant(input: TokenStream) -> TokenStream {
    if let Ok(value) = syn::parse::Parser::parse(
        Punctuated::<LitInt, Token![:]>::parse_terminated,
        input.clone(),
    ) {
        return duration::hms_to_seconds(value);
    };

    duration::units_to_seconds(input)
}

#[proc_macro]
pub fn naive_time_constant(input: TokenStream) -> TokenStream {
    naive_time::hms_to_seconds(input)
}
