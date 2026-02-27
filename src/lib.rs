use std::sync::{OnceLock, RwLock};

pub use bondage_macros::*;
use neon::prelude::*;

use crate as bondage;

// #[linkme::distributed_slice]
// pub static JS_EXPORTS: [(&str, fn(FunctionContext) -> JsResult<JsValue>)];

pub static JS_CHANNEL: RwLock<OnceLock<Channel>> = RwLock::new(OnceLock::new());

pub trait Transferable:
    Sendable<JsForm = <Self as Transferable>::JsForm>
    + Receivable<JsForm = <Self as Transferable>::JsForm>
{
    type JsForm: Value;
}

impl<JS: Value, T: Receivable<JsForm = JS> + Sendable<JsForm = JS>> Transferable for T {
    type JsForm = JS;
}

pub trait Receivable {
    type JsForm: Value;
    fn from_js<'cx>(ctx: &mut Cx<'cx>, object: Handle<'cx, Self::JsForm>) -> NeonResult<Self>
    where
        Self: Sized;
}

pub trait Sendable {
    type JsForm: Value;
    fn to_js<'cx>(&self, ctx: &mut Cx<'cx>) -> Handle<'cx, Self::JsForm>;
}

impl<V: Value> Sendable for Handle<'_, V> {
    type JsForm = JsValue;

    fn to_js<'cx>(&self, cx: &mut Cx<'cx>) -> Handle<'cx, Self::JsForm> {
        self.as_value(cx)
    }
}

impl<V: Object> Receivable for Root<V> {
    type JsForm = V;

    fn from_js<'cx>(cx: &mut Cx<'cx>, value: Handle<'cx, Self::JsForm>) -> NeonResult<Self> {
        Ok(value.root(cx))
    }
}

macro_rules! primitive {
    ($($js:ident into $rust:ty)*) => ($(
        impl Receivable for $rust {
            type JsForm = $js;
            fn from_js<'cx>(ctx: &mut Cx<'cx>, object: Handle<'cx, $js>) -> NeonResult<Self> {
                Ok(object.value(ctx))
            }
        }
    )*);
    ($($js:ident from $rust:ty)*) => ($(
        impl Sendable for $rust {
            type JsForm = $js;
            fn to_js<'cx>(&self, ctx: &mut Cx<'cx>) -> Handle<'cx, $js> {
                $js::new(ctx, self.to_owned())
            }
        }
    )*);
    ($($js:ident via $rust:ty)*) => ($(
        primitive!($js into $rust);
        primitive!($js from $rust);
    )*);
}

primitive! {
    JsString via String
    JsNumber via f64
    JsBoolean via bool
}

primitive! {
    JsString from &str
    JsNumber from f32
    JsNumber from u8
    JsNumber from u16
    JsNumber from u32
}

impl Sendable for () {
    type JsForm = JsUndefined;

    fn to_js<'cx>(&self, ctx: &mut Cx<'cx>) -> Handle<'cx, Self::JsForm> {
        ctx.undefined()
    }
}

impl Receivable for () {
    type JsForm = JsUndefined;

    fn from_js<'cx>(_: &mut Cx<'cx>, _: Handle<'cx, Self::JsForm>) -> NeonResult<Self>
    where
        Self: Sized,
    {
        Ok(())
    }
}

impl<T> Sendable for Vec<T>
where
    T: Sendable,
{
    type JsForm = JsArray;
    fn to_js<'cx>(&self, ctx: &mut Cx<'cx>) -> Handle<'cx, JsArray> {
        let arr = JsArray::new(ctx, self.len());

        self.iter().enumerate().for_each(|(i, el)| {
            let el = el.to_js(ctx).as_value(ctx);
            let _ = arr.set(ctx, i.to_string().as_str(), el);
        });

        arr
    }
}

impl<T> Receivable for Vec<T>
where
    T: Receivable,
{
    type JsForm = JsArray;
    fn from_js<'cx>(ctx: &mut Cx<'cx>, array: Handle<'cx, JsArray>) -> NeonResult<Self> {
        let vec = array.to_vec(ctx)?;

        let vec: Vec<_> = vec
            .iter()
            .filter_map(|el| {
                el.downcast::<T::JsForm, Cx>(ctx)
                    .ok()
                    .and_then(|el| T::from_js(ctx, el).ok())
            })
            .collect();

        Ok(vec)
    }
}

impl<T> Sendable for Option<T>
where
    T: Sendable,
{
    type JsForm = JsValue;
    fn to_js<'cx>(&self, ctx: &mut Cx<'cx>) -> Handle<'cx, JsValue> {
        match self {
            Some(value) => value.to_js(ctx).as_value(ctx),
            None => ctx.undefined().upcast::<JsValue>(),
        }
    }
}

impl<T> Receivable for Option<T>
where
    T: Receivable,
{
    type JsForm = JsValue;
    fn from_js<'cx>(ctx: &mut Cx<'cx>, value: Handle<'cx, JsValue>) -> NeonResult<Self> {
        let value = match value.is_a::<T::JsForm, _>(ctx) {
            true => value.downcast::<T::JsForm, _>(ctx).unwrap(),
            false => return Ok(None),
        };

        Receivable::from_js(ctx, value).map(|v| Some(v))
    }
}

#[with_context]
pub fn console_log<'cx, T: Sendable + Send + 'static>(ctx: &mut Cx<'cx>, msg: T) -> NeonResult<()> {
    let msg = msg.to_js(ctx);

    let Some(mut log) = ctx
        .global::<JsObject>("console")
        .and_then(|console| console.method(ctx, "log"))
        .ok()
    else {
        return Ok(());
    };

    let _ = log.arg(msg);

    let _ = log.call::<()>();
    Ok(())
}
