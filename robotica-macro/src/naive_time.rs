use super::error;
use proc_macro::TokenStream;
use quote::quote;
use syn::{punctuated::Punctuated, LitInt, Token};

pub(crate) fn hms_to_seconds(input: TokenStream) -> TokenStream {
    let value = match syn::parse::Parser::parse(
        Punctuated::<LitInt, Token![:]>::parse_terminated,
        input.clone(),
    ) {
        Ok(value) => value,
        Err(err) => return proc_macro::TokenStream::from(err.to_compile_error()),
    };

    if value.len() != 3 {
        return error!("Expected exactly three arguments but got {}", value.len());
    }

    let Ok(hours) = value[0].base10_parse::<u32>() else {
        return error!("Hours must be an integer");
    };

    let Ok(minutes) = value[1].base10_parse::<u32>() else {
        return error!("Minutes must be an integer");
    };

    let Ok(seconds) = value[2].base10_parse::<u32>() else {
        return error!("Seconds must be an integer");
    };

    if !(0..24).contains(&hours) {
        return error!("Hours must be between 0 and 23");
    }

    if !(0..60).contains(&minutes) {
        return error!("Minutes must be between 0 and 59");
    }

    if !(0..60).contains(&seconds) {
        return error!("Seconds must be between 0 and 59");
    }

    let total_seconds = hours * 3600 + minutes * 60 + seconds;

    // This check should not actually fail because we did checks already.
    if !(0..86400).contains(&total_seconds) {
        return error!("Total seconds must be between 0 and 86399");
    }

    TokenStream::from(quote! {
        match chrono::NaiveTime::from_num_seconds_from_midnight_opt(#total_seconds, 0) {
            Some(time) => time,
            // This should never happen, but we need to handle it.
            None => panic!("Invalid time"),
        }
    })
}
