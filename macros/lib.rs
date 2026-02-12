mod declare;
mod export;
mod receivable;
mod sendable;

use proc_macro::TokenStream;
use std::io::Write;
use syn::parse_macro_input;

use declare::{DELCS, DeclType, dts_content, dts_file, is_io_allowed};

pub(crate) fn get_generic<'a>(ty: &'a syn::Type) -> Option<&'a syn::Type> {
    let syn::Type::Path(type_path) = ty else {
        return None;
    };

    let syn::PathArguments::AngleBracketed(generics) = &type_path.path.segments.last()?.arguments
    else {
        return None;
    };

    let syn::GenericArgument::Type(first_generic) = &generics.args.first()? else {
        return None;
    };

    Some(first_generic)
}

#[proc_macro_attribute]
pub fn main(_: TokenStream, input: TokenStream) -> TokenStream {
    let body = parse_macro_input!(input as syn::ItemFn);

    let mut guard = DELCS.write().unwrap();

    guard.push(DeclType::TypeDecl(
        "Option<T>".to_string(),
        "T|undefined".to_string(),
    ));

    guard.push(DeclType::TypeDecl("Vec<T>".to_string(), "T[]".to_string()));

    drop(guard);

    if is_io_allowed() {
        let mut file = dts_file();
        let dts = dts_content();
        file.write_all(dts.as_bytes()).unwrap();
    }

    quote::quote! {
    #[neon::main]
    #body
    }
    .into()
}

#[proc_macro_attribute]
pub fn export(args: TokenStream, body: TokenStream) -> TokenStream {
    export::export_impl(args, body)
}

#[proc_macro_derive(Sendable)]
pub fn sendable_derive(input: TokenStream) -> TokenStream {
    sendable::sendable_derive_impl(input)
}

#[proc_macro_derive(Receivable)]
pub fn receivable_derive(input: TokenStream) -> TokenStream {
    receivable::receivable_derive_impl(input)
}
