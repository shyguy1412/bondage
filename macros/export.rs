use proc_macro::{Span, TokenStream};
use quote::ToTokens;
use syn::{Meta, parse_macro_input, parse_str, punctuated::Punctuated};

use crate::declare::declare_item_fn;

#[inline(always)]
pub fn export_impl(args: TokenStream, input: TokenStream) -> TokenStream {
    let item = parse_macro_input!(input as syn::Item);

    match item {
        // syn::Item::Enum(item_enum) => export_item_enum(args, item_enum),
        // syn::Item::Struct(item_struct) => export_item_struct(args, item_struct),
        syn::Item::Fn(item_fn) => export_item_function(args, item_fn),
        _ => item.to_token_stream().into(),
    }
}

pub fn export_item_function(args: TokenStream, item_fn: syn::ItemFn) -> TokenStream {
    let args = parse_macro_input!(args with Punctuated::<Meta, syn::Token![,]>::parse_terminated);

    let ident_str = declare_item_fn(&item_fn, &args);

    let js_ident = syn::Ident::new(&ident_str, Span::call_site().into());
    let ident = &item_fn.sig.ident;

    let ret: syn::Type = match &item_fn.sig.output {
        syn::ReturnType::Default => parse_str("()").unwrap(),
        syn::ReturnType::Type(_, ret_type) => crate::get_generic(&*ret_type)
            .cloned()
            .or(parse_str("()").ok())
            .unwrap(),
    };

    let arg_ident: Vec<_> = item_fn
        .sig
        .inputs
        .iter()
        .skip(1)
        .filter_map(|arg| match arg {
            syn::FnArg::Receiver(_) => None,
            syn::FnArg::Typed(pat_type) => Some(pat_type),
        })
        .map(|ty| *ty.pat.clone())
        .collect();

    quote::quote! {
    use neon::prelude::*;
    #[neon::export]
    fn #js_ident<'cx>(
        ctx: &mut FunctionContext<'cx>,
    ) -> JsResult<'cx, <#ret as Sendable>::JsForm> {
        #item_fn

        #(
            let #arg_ident = {
                let intermediate = ctx.arg()?;
                Receivable::from_js(ctx, intermediate)?
            };
        )*

        let ret = #ident(ctx, #(#arg_ident),*)?.to_js(ctx);
        Ok(ret)
    }
    }
    .into()
}
