use proc_macro::TokenStream;
use quote::{ToTokens, quote};
use std::{io::Write, sync::RwLock};
use syn::{Token, parse::Parser, parse_macro_input, parse_str};

enum DeclType {
    FunctionDecl(String, Vec<String>, String),
    ExportDecl(String, String),
    TypeDecl(String, String),
    EventDecl(String, String),
}

static DELCS: RwLock<Vec<DeclType>> = RwLock::new(vec![]);

fn dts_file() -> std::fs::File {
    std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open("./src/core/package/core.d.ts")
        .unwrap()
}

fn dts_content() -> String {
    let mut events: Vec<(String, String)> = vec![];
    let mut decls = DELCS
        .read()
        .unwrap()
        .iter()
        .filter_map(|decl_type| match decl_type {
            DeclType::FunctionDecl(name, args, ret) => Some(format!(
                "  function {}({}): {};",
                name,
                args.iter()
                    .enumerate()
                    .map(|(i, arg)| format!("arg_{}: {}", i, arg))
                    .fold("".to_string(), |prev, cur| format!("{}, {}", prev, cur)),
                ret
            )),
            DeclType::ExportDecl(name, decl) => Some(format!("  const {}: {};", name, decl)),
            DeclType::TypeDecl(name, decl) => Some(format!("  type {} = {};", name, decl)),
            DeclType::EventDecl(name, decl) => {
                events.push((name.clone(), decl.clone()));
                None
            }
        })
        .reduce(|prev, cur| format!("{}\n{}", prev, cur))
        .unwrap_or("".to_string());

    decls.push_str("\n  type Events = {");

    decls.push_str(&events.iter().fold("  ".to_string(), |prev, (name, decl)| {
        format!("{}\n    {}: {}", prev, name, decl)
    }));

    decls.push_str("\n  };");

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
        _ => {
            let ty: syn::TypePath = parse_str(rust_type).expect("All types should be paths");

            let ident = ty.path.segments.first().unwrap().ident.to_string();

            let inner = get_generic(&syn::Type::Path(ty)).map(|inner| match inner {
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

fn declare_item_struct(item_struct: &syn::ItemStruct) {
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

    drop(guard);
}

#[proc_macro_attribute]
pub fn main(_: TokenStream, input: TokenStream) -> TokenStream {
    let body = parse_macro_input!(input as syn::ItemFn);

    if body.sig.ident.to_string() != "main" {
        panic!("main macro must be on main function"); //it doesn't really but it should
    }

    let block = body.block;

    let mut guard = DELCS.write().unwrap();
    guard.push(DeclType::ExportDecl(
        "setEventListener".to_string(),
        "(callback:(event:keyof Events, data: Events[keyof Events]) => void) => void".to_string(),
    ));

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
    fn main(mut ctx: ModuleContext) -> NeonResult<()> {
        //Init the EventSystem
        let event_system = EVENT_SYSTEM.lock().unwrap();
        event_system
            .set(EventSystem::new(ctx.channel()))
            .expect("Need to be able to initialize EventSystem");
        drop(event_system);

        //Export all functions collected with the export macro
        for (name, function) in JS_EXPORTS {
            ctx.export_function(name, function)?;
        }

        //Export "on" function to register event handlers
        ctx.export_function("setEventListener", |mut ctx: FunctionContext| {
            // let event = ctx.argument::<JsString>(0)?.value(&mut ctx);
            let callback = ctx.argument::<JsFunction>(0)?.root(&mut ctx);

            EVENT_SYSTEM.set_event_listener(callback);

            Ok(ctx.undefined())
        })?;

        #block

        Ok(())
    }
    }
    .into()
}

#[proc_macro_attribute]
pub fn export(args: TokenStream, input: TokenStream) -> TokenStream {
    let item = parse_macro_input!(input as syn::Item);

    match item {
        // syn::Item::Enum(item_enum) => export_item_enum(args, item_enum),
        // syn::Item::Struct(item_struct) => export_item_struct(args, item_struct),
        syn::Item::Fn(item_fn) => export_item_function(args, item_fn),
        _ => TokenStream::new(),
    }
}

// fn export_item_struct(args: TokenStream, item_enum: syn::ItemStruct) -> TokenStream {
//     TokenStream::new()
// }

// fn export_item_enum(args: TokenStream, item_enum: syn::ItemEnum) -> TokenStream {
//     TokenStream::new()
// }

fn get_generic<'a>(ty: &'a syn::Type) -> Option<&'a syn::Type> {
    match ty {
        syn::Type::Path(type_path) => match &type_path.path.segments[0].arguments {
            syn::PathArguments::AngleBracketed(ty) => match &ty.args[0] {
                syn::GenericArgument::Type(ty) => Some(ty),
                _ => None,
            },
            _ => None,
        },
        _ => None,
    }
}
fn export_item_function(args: TokenStream, item_fn: syn::ItemFn) -> TokenStream {
    let args: Vec<String> = syn::punctuated::Punctuated::<syn::LitStr, Token![,]>::parse_terminated
        .parse(args)
        .unwrap_or(syn::punctuated::Punctuated::default())
        .into_iter()
        .map(|arg| arg.value())
        .collect();

    let name: String = item_fn
        .sig
        .ident
        .to_string()
        .split("_")
        .enumerate()
        .map(|(i, word)| {
            i.eq(&0).then(|| word.to_string()).unwrap_or(
                word.chars()
                    .enumerate()
                    .map(|(i, char)| i.eq(&0).then(|| char.to_ascii_uppercase()).unwrap_or(char))
                    .collect(),
            )
        })
        .collect();

    let ret = match &item_fn.sig.output {
        syn::ReturnType::Default => "undefined".to_string(),
        syn::ReturnType::Type(_, ret_type) => get_generic(&*ret_type)
            // .and_then(get_generic)
            .and_then(|ty| match ty {
                syn::Type::Path(type_path) => type_path.path.get_ident(),
                _ => None,
            })
            .map(|ident| rust_type_to_js(&ident.to_string()))
            .unwrap_or("undefined".to_string()),
    };

    let mut guard = DELCS.write().unwrap();
    guard.push(DeclType::FunctionDecl(name.clone(), args, ret));
    drop(guard);

    let item_fn_ident = item_fn.sig.ident.clone();

    let mangled_ident: syn::Ident =
        parse_str(&format!("JS_EXPORTS_{}", name)).expect("Guranteed by args");

    if is_io_allowed() {
        let mut file = dts_file();
        let dts = dts_content();
        file.write_all(dts.as_bytes()).unwrap();
    }
    quote::quote! {
        #[linkme::distributed_slice(JS_EXPORTS)]
        static #mangled_ident: (&str, fn(FunctionContext) -> JsResult<JsValue>) =
            (#name, |mut arg:FunctionContext|#item_fn_ident(&mut arg).map(|ret| Sendable::to_js(&ret, &mut arg)).map(|ret| ret.upcast()));

       #item_fn

    }
    .into()
}

#[proc_macro_attribute]
pub fn event_name(_: TokenStream, input: TokenStream) -> TokenStream {
    input
}

#[proc_macro_derive(Event)]
pub fn event_derive(input: TokenStream) -> TokenStream {
    let item_struct = parse_macro_input!(input as syn::ItemStruct);
    declare_item_struct(&item_struct);
    let item_struct_ident = &item_struct.ident;

    let event_name = item_struct
        .attrs
        .iter()
        .find(|attr| {
            attr.path()
                .segments
                .first()
                .map(|seg| seg.ident.to_string() == "event_name")
                .unwrap_or(false)
        })
        .and_then(|attr| attr.parse_args().ok())
        .map(|arg: syn::LitStr| arg.value())
        .unwrap_or_else(|| {
            item_struct
                .ident
                .to_string()
                .replace("Event", "")
                .chars()
                .flat_map(|char| match char.is_ascii_uppercase() {
                    true => vec!['-', char.to_ascii_lowercase()],
                    false => vec![char],
                })
                .skip(1)
                .collect()
        });

    let mut guard = DELCS.write().unwrap();
    guard.push(DeclType::EventDecl(
        event_name.clone(),
        item_struct_ident.to_string(),
    ));
    drop(guard);

    if is_io_allowed() {
        let mut file = dts_file();
        let dts = dts_content();
        file.write_all(dts.as_bytes()).unwrap();
    }

    // let data: Vec<syn::Expr> = item_struct
    //     .fields
    //     .iter()
    //     .filter_map(|field| field.ident.as_ref())
    //     .map(|ident| parse_quote!(self.#ident.to_js(ctx)?.as_value(ctx)))
    //     .collect();

    quote! {
    //should generate
    impl bondage::Event for #item_struct_ident {
        fn name(&self) -> &str {
            #event_name
        }

        fn data<'cx>(&self, ctx: &mut Cx<'cx>) -> NeonResult<Handle<'cx, JsValue>> {
            Ok(self.to_js(ctx)?.as_value(ctx))
        }
    }
    }
    .into()
}

#[proc_macro_derive(Transferable)]
pub fn transferable_jsobject_derive(input: TokenStream) -> TokenStream {
    let item_struct = parse_macro_input!(input as syn::ItemStruct);
    declare_item_struct(&item_struct);
    let ident = item_struct.ident;

    if is_io_allowed() {
        let mut file = dts_file();
        let dts = dts_content();
        file.write_all(dts.as_bytes()).unwrap();
    }

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
                    "let {ident_str} = Transferable::from_js(ctx, {ident_str})?;"
                ))
                .unwrap(),
            ]
        })
        .collect();

    let member_to_js_statements: Vec<syn::Stmt> = members
        .iter()
        .flat_map(|m| {
            let ident = m.ident.clone().unwrap();
            let ident_str = ident.to_string();
            vec![
                parse_str(&format!("let {ident_str} = self.{ident_str}.to_js(ctx)?;")).unwrap(),
                parse_str(&format!("object.set(ctx, \"{ident_str}\", {ident_str})?;")).unwrap(),
            ]
        })
        .collect();

    quote::quote! {
        #[automatically_derived]
        impl Transferable for #ident{
            type JsForm = JsObject;
            fn to_js<'cx>(&self, ctx: &mut Cx<'cx>) -> NeonResult<Handle<'cx, JsObject>> {
                let object = ctx.empty_object();

                #(#member_to_js_statements);*

                Ok(object)
            }

            fn from_js<'cx>(ctx: &mut Cx<'cx>, object: Handle<'cx, JsObject>) -> NeonResult<Self> {

                #(#member_from_js_statements);*

                Ok(Self { #(#member_idents),* })
            }
    }
    }
    .into()
}
