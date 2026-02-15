use std::{
    io::Write,
    sync::{RwLock, RwLockWriteGuard},
};

use quote::ToTokens;
use syn::{Expr, ExprLit, Lit, Meta, parse_str, punctuated::Punctuated, token::Comma};

use crate::get_generic;

pub enum DeclType {
    FunctionDecl(String, Vec<(String, String)>, String),
    TypeDecl(String, String),
}

pub static DELCS: RwLock<Vec<DeclType>> = RwLock::new(vec![]);

pub trait DeclarationsTrait {
    fn commit(self)
    where
        Self: Sized,
    {
        drop(self);
        if is_io_allowed() {
            let mut file = dts_file();
            let dts = dts_content();
            file.write_all(dts.as_bytes()).unwrap();
        }
    }
}

impl DeclarationsTrait for RwLockWriteGuard<'_, Vec<DeclType>> {}

fn dts_file() -> std::fs::File {
    std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open("./src/core/package/core.d.ts")
        .unwrap()
}

fn dts_content() -> String {
    fn format_function_decl(name: &String, args: &Vec<(String, String)>, ret: &String) -> String {
        format!(
            "  function {}({}): {};",
            name,
            args.iter()
                .map(|(name, ty)| format!("{name}: {ty}"))
                .fold("".to_string(), |prev, cur| format!("{prev}{cur}, ")),
            ret
        )
    }

    let decls = DELCS
        .read()
        .unwrap()
        .iter()
        .filter_map(|decl_type| match decl_type {
            DeclType::FunctionDecl(name, args, ret) => Some(format_function_decl(name, args, ret)),
            DeclType::TypeDecl(name, decl) => Some(format!("  type {name} = {decl};")),
        })
        .reduce(|prev, cur| format!("{}\n{}", prev, cur))
        .unwrap_or("".to_string());

    format!("declare module \"@core\"{{\n{}\n}}", decls)
}

fn is_io_allowed() -> bool {
    match std::env::var("TS_DECL_GEN") {
        Ok(_) => true,
        Err(_) => false,
    }
}

///! There are still a bunch of cases missing here I bet
fn rust_type_to_js(rust_type: &str) -> String {
    match rust_type {
        "f64" | "JsNumber" => "number",
        "String" | "JsString" => "string",
        "JsBoolean" | "bool" => "boolean",
        "JsValue" => "any",
        "JsObject" => "object",
        "Root < JsFunction >" => "(...args:unknown) => unknown",
        _ => {
            let ty: syn::TypePath = parse_str(rust_type).expect("All types should be paths");

            let ident = ty.path.segments.first().unwrap().ident.to_string();

            let inner = crate::get_generic(&syn::Type::Path(ty)).map(|inner| match inner {
                syn::Type::Path(type_path) => {
                    rust_type_to_js(&type_path.path.to_token_stream().to_string())
                }

                _ => "NOT A PATH :(".to_string(),
            });

            return match inner {
                Some(inner) => format!("{}<{}>", ident, inner),
                None => ident,
            };
        }
    }
    .to_string()
}

pub fn declare_item_struct(item_struct: &syn::ItemStruct) {
    let mut guard = DELCS.write().unwrap();
    let ident = item_struct.ident.to_string();

    if guard.iter().any(|decl| match decl {
        DeclType::TypeDecl(name, _) => *name == ident,
        _ => false,
    }) {
        return;
    }

    let props = item_struct
        .fields
        .iter()
        .filter_map(|field| field.ident.as_ref().map(|ident| (ident, &field.ty)))
        .filter_map(|(ident, ty)| match ty {
            syn::Type::Path(type_path) => Some((ident, type_path)),
            _ => None,
        })
        .map(|(ident, ty)| {
            (
                ident,
                rust_type_to_js(&ty.path.to_token_stream().to_string()),
            )
        })
        .fold("".to_string(), |prev, (ident, ty)| {
            format!("{}\n    {}: {}", prev, ident, ty)
        });

    guard.push(DeclType::TypeDecl(ident, format!("{{{}\n  }}", props)));

    guard.commit()
}

pub fn declare_item_fn(item_fn: &syn::ItemFn, args: &Punctuated<Meta, Comma>) -> String {
    fn capitalize(word: &str) -> String {
        word.chars()
            .enumerate()
            .map(|(i, char)| i.eq(&0).then(|| char.to_ascii_uppercase()).unwrap_or(char))
            .collect()
    }

    let js_name: String = item_fn
        .sig
        .ident
        .to_string()
        .split("_")
        .enumerate()
        .map(|(i, word)| match i {
            0 => word.to_string(),
            _ => capitalize(word),
        })
        .collect();

    let mut guard = DELCS.write().unwrap();
    if guard.iter().any(|decl| match decl {
        DeclType::FunctionDecl(name, ..) => *name == js_name,
        _ => false,
    }) {
        return js_name;
    }

    let override_args: &Vec<(_, _)> = &args
        .iter()
        .map(|arg| {
            let Ok(value) = arg.require_name_value() else {
                panic!("Invalid macro argument");
            };

            let Some(ident) = value.path.get_ident() else {
                panic!("Invalid macro argument");
            };

            let Expr::Lit(ExprLit {
                lit: Lit::Str(ref decl),
                ..
            }) = value.value
            else {
                panic!("Invalid macro argument");
            };

            (ident.to_string(), decl.value())
        })
        .collect();

    let ret = match &item_fn.sig.output {
        syn::ReturnType::Default => "undefined".to_string(),
        syn::ReturnType::Type(_, ret_type) => get_generic(&*ret_type)
            .and_then(|ty| match ty {
                syn::Type::Path(type_path) => type_path.path.get_ident(),
                _ => None,
            })
            .map(|ident| rust_type_to_js(&ident.to_string()))
            .unwrap_or("undefined".to_string()),
    };

    fn apply_override(
        ident: String,
        decl: String,
        overrides: &Vec<(String, String)>,
    ) -> (String, String) {
        overrides
            .iter()
            .find(|(override_ident, ..)| *override_ident == ident)
            .map(|decl| decl.clone())
            .unwrap_or((ident, decl))
    }

    let args: Vec<(_, _)> = item_fn
        .sig
        .inputs
        .iter()
        .filter_map(|arg| match arg {
            syn::FnArg::Receiver(_) => None,
            syn::FnArg::Typed(pat_type) => Some(pat_type),
        })
        .map(|ty| {
            apply_override(
                ty.pat.to_token_stream().to_string(),
                rust_type_to_js(&ty.ty.to_token_stream().to_string()),
                override_args,
            )
        })
        .collect();

    guard.push(DeclType::FunctionDecl(js_name.clone(), args, ret));
    guard.commit();

    js_name
}

pub fn declare_item_enum(item_enum: &syn::ItemEnum) {
    let ident = item_enum.ident.to_string();

    let mut guard = DELCS.write().unwrap();
    if guard.iter().any(|decl| match decl {
        DeclType::TypeDecl(name, ..) => *name == ident,
        _ => false,
    }) {
        return;
    }

    let variants: String = item_enum
        .variants
        .iter()
        .filter_map(|variant| match &variant.fields {
            syn::Fields::Unnamed(fields_unnamed) => Some(fields_unnamed),
            _ => panic!("Can only have unamed fields for now"),
        })
        .map(|fields_unnamed| match fields_unnamed.unnamed.len() {
            1 => fields_unnamed.unnamed.first().unwrap(),
            _ => panic!("Must only have one field"),
        })
        .map(|field| &field.ty)
        .map(|ty| rust_type_to_js(&ty.to_token_stream().to_string()))
        .fold("".to_string(), |prev, cur| format!("{prev}{cur}|"));

    //remove trailing |
    let variants = variants[0..variants.len() - 1].to_string();

    guard.push(DeclType::TypeDecl(ident, variants));
    guard.commit()
}

pub fn declare_type(ident: String, decl: String) {
    let mut guard = DELCS.write().unwrap();

    if guard.iter().any(|decl| match decl {
        DeclType::TypeDecl(name, ..) => *name == ident,
        _ => false,
    }) {
        return;
    }
    guard.push(DeclType::TypeDecl(ident, decl));
    guard.commit()
}
