mod declare;
mod export;
mod receivable;
mod sendable;

use proc_macro::TokenStream;
use syn::{FnArg, Pat, PatIdent, PatType, parse_macro_input};

use declare::{DELCS, DeclType, DeclarationsTrait};

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
    let item_fn = parse_macro_input!(input as syn::ItemFn);
    let sig = &item_fn.sig;
    let vis = &item_fn.vis;
    let fn_ident = &item_fn.sig.ident;
    let FnArg::Typed(PatType { pat, .. }) = item_fn
        .sig
        .inputs
        .first()
        .expect("Main function must take ModuleContext as argument")
    else {
        panic!("Main function can not be a method");
    };

    let Pat::Ident(PatIdent {
        ident: ref arg_ident,
        ..
    }) = **pat
    else {
        panic!("Arg must have an identifer");
    };

    let mut guard = DELCS.write().unwrap();

    guard.push(DeclType::TypeDecl(
        "Option<T>".to_string(),
        "T|undefined".to_string(),
    ));

    guard.push(DeclType::TypeDecl("Vec<T>".to_string(), "T[]".to_string()));
    guard.commit();
    quote::quote! {
    #[neon::main]
    #vis #sig{
        neon::registered().export(&mut #arg_ident)?;

        #item_fn
        let channel = #arg_ident.channel();
        let _ = JS_CHANNEL.write().map(|cell| cell.set(channel));
        #fn_ident(#arg_ident)
    }
    }
    .into()
}

#[proc_macro_attribute]
pub fn with_context(_: TokenStream, body: TokenStream) -> TokenStream {
    let item_fn = parse_macro_input!(body as syn::ItemFn);
    let ident = &item_fn.sig.ident;
    let args: Vec<_> = item_fn.sig.inputs.iter().skip(1).collect();
    let generics = &item_fn.sig.generics;
    let arg_idents: Vec<_> = item_fn
        .sig
        .inputs
        .iter()
        .skip(1)
        .map(|arg| match arg {
            FnArg::Receiver(_) => panic!("Not an item function"),
            FnArg::Typed(PatType { pat, .. }) => pat,
        })
        .map(|pat| match pat.as_ref() {
            Pat::Ident(PatIdent { ident, .. }) => ident,
            _ => panic!("Patterns are not supported when using with_context"),
        })
        .collect();
    let vis = &item_fn.vis;

    quote::quote! {
        #vis fn #ident #generics(#(#args),*) {
            #item_fn

            let channel_lock = match bondage::JS_CHANNEL.read() {
                Ok(lock) => lock,
                Err(error) => error.into_inner(),
            };

            let _ = channel_lock
                .wait()
                .send(move |mut ctx| #ident(&mut ctx, #(#arg_idents),*))
                .join();
        }
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
