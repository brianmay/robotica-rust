use super::error;
use proc_macro::TokenStream;
use quote::quote;
use syn::{punctuated::Punctuated, LitInt, Path, Token};

pub(crate) fn hms_to_seconds(value: Punctuated<LitInt, Token![:]>) -> TokenStream {
    if value.len() != 3 {
        return error!("Expected exactly three arguments but got {}", value.len());
    }

    let Ok(hours) = value[0].base10_parse::<u64>() else {
        return error!("Hours must be an integer");
    };

    let Ok(minutes) = value[1].base10_parse::<u64>() else {
        return error!("Minutes must be an integer");
    };

    let Ok(seconds) = value[2].base10_parse::<u64>() else {
        return error!("Seconds must be an integer");
    };

    if !(0..60).contains(&minutes) {
        return error!("Minutes must be between 0 and 59");
    }

    if !(0..60).contains(&seconds) {
        return error!("Seconds must be between 0 and 59");
    }

    let total_seconds = hours * 3600 + minutes * 60 + seconds;

    TokenStream::from(quote! {
        std::time::Duration::from_secs(#total_seconds)
    })
}

struct ValueUnits {
    value: LitInt,
    unit: Path,
}

impl syn::parse::Parse for ValueUnits {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        Ok(ValueUnits {
            value: input.parse()?,
            unit: input.parse()?,
        })
    }
}

pub(crate) fn units_to_seconds(input: TokenStream) -> TokenStream {
    let value_units: ValueUnits = match syn::parse(input) {
        Ok(value_units) => value_units,
        Err(err) => return proc_macro::TokenStream::from(err.to_compile_error()),
    };

    let Ok(number) = value_units.value.base10_parse::<u64>() else {
        return error!("Number must be an integer");
    };

    let Some(unit) = value_units.unit.get_ident().map(|x| x.to_string()) else {
        return error!("Expected a unit but got a path");
    };

    let total_seconds = match unit.as_str() {
        "days" => number * 24 * 3600,
        "hours" => number * 3600,
        "minutes" => number * 60,
        _ => return error!("Invalid time unit {unit}"),
    };

    TokenStream::from(quote! {
        std::time::Duration::from_secs(#total_seconds)
    })
}
