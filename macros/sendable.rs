use proc_macro::TokenStream;
use syn::{ItemEnum, ItemStruct, parse_macro_input, parse_str};

use crate::declare::{declare_item_enum, declare_item_struct};

#[inline(always)]
pub fn sendable_derive_impl(input: TokenStream) -> TokenStream {
    let item = parse_macro_input!(input as syn::Item);

    match item {
        syn::Item::Enum(item_enum) => sendable_derive_enum(&item_enum),
        syn::Item::Struct(item_struct) => sendable_derive_struct(&item_struct),
        _ => panic!("Epected Enum or Struct"),
    }
}

fn sendable_derive_enum(item_enum: &ItemEnum) -> TokenStream {
    declare_item_enum(item_enum);
    let ident = &item_enum.ident;
    let variants: Vec<_> = item_enum.variants.iter().map(|f| f.ident.clone()).collect();

    quote::quote! {
    use neon::prelude::*;
    #[automatically_derived]
    impl Sendable for #ident {
    type JsForm = JsValue;

    fn to_js<'cx>(&self, ctx: &mut Cx<'cx>) -> Handle<'cx, Self::JsForm> {
        use #ident::*;
        match self {
            #(
                #variants(val) => {
                    Sendable::to_js(val, ctx).as_value(ctx)
                }
            )*
        }
    }
    }
    }
    .into()
}

fn sendable_derive_struct(item_struct: &ItemStruct) -> TokenStream {
    declare_item_struct(item_struct);
    let ident = &item_struct.ident;
    let members: Vec<_> = item_struct.fields.iter().map(|f| f.clone()).collect();
    let member_to_js_statements: Vec<syn::Stmt> = members
        .iter()
        .flat_map(|m| {
            let ident = m.ident.clone().unwrap();
            let ident_str = ident.to_string();
            vec![
                parse_str(&format!(
                    "let {ident_str} = Sendable::to_js(&self.{ident_str}, ctx);"
                ))
                .unwrap(),
                parse_str(&format!(
                    "let _ = object.set(ctx, \"{ident_str}\", {ident_str});"
                ))
                .unwrap(),
            ]
        })
        .collect();

    quote::quote! {
    use neon::prelude::*;
    #[automatically_derived]
    impl Sendable for #ident {
        type JsForm = JsObject;

        fn to_js<'cx>(&self, ctx: &mut Cx<'cx>) -> Handle<'cx, JsObject> {
            let object = ctx.empty_object();

            #(#member_to_js_statements);*

            object
        }
    }
    }
    .into()
}
