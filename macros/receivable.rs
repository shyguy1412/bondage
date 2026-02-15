use proc_macro::TokenStream;
use syn::{parse_macro_input, parse_str};

use crate::declare::declare_item_struct;

pub fn receivable_derive_impl(input: TokenStream) -> TokenStream {
    let item_struct = parse_macro_input!(input as syn::ItemStruct);
    declare_item_struct(&item_struct);
    let ident = item_struct.ident;
    let members: Vec<_> = item_struct.fields.iter().map(|f| f.clone()).collect();
    let member_idents: Vec<_> = members.iter().filter_map(|m| m.ident.clone()).collect();
    let member_from_js_statements: Vec<syn::Stmt> = members
        .iter()
        .flat_map(|m| {
            let ident = m.ident.clone().unwrap();
            let ident_str = ident.to_string();

            vec![
                parse_str(&format!(
                    "let {ident_str} = object.get(ctx, \"{ident_str}\")?;"
                ))
                .unwrap(),
                parse_str(&format!(
                    "let {ident_str} = Receivable::from_js(ctx, {ident_str})?;"
                ))
                .unwrap(),
            ]
        })
        .collect();

    quote::quote! {
        use neon::prelude::*;
        #[automatically_derived]
        impl Receivable for #ident{
            type JsForm = JsObject;

            fn from_js<'cx>(ctx: &mut Cx<'cx>, object: Handle<'cx, JsObject>) -> NeonResult<Self> {

                #(#member_from_js_statements);*

                Ok(Self { #(#member_idents),* })
            }
    }
    }
    .into()
}
