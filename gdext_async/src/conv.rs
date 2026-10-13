
//! This hidden module is used during expansion of `godot_async!` and
//! related macros. The API in this module IS NOT stable and SHALL NOT
//! be relied upon.
//!
//! The problem: we want a `#[async_func]` (or equivalently
//! `godot_async!` function) to be able to return the same types as a
//! regular `#[func]` one. [`ToGodot`] is close to the trait we want,
//! but `Result` does not implement `ToGodot`. Internally to
//! `godot-rust`, the trait `FuncReturn` is the one we want, but it
//! doesn't have a stable and public API. So we emulate `FuncReturn`
//! by defining a type that behaves correctly for: Anything that
//! implements `ToGodot`, and `Result<T, E>` where `T: ToGodot` and
//! `E: ErrorToGodot`.

use godot::prelude::*;
use godot::meta::error::{CallOutcome, ErrorToGodot};

pub struct AsyncToGodotWrapper<T>(pub T);

pub trait AsyncToGodot {
  // Fallback impl, for any type that is not `Result`.
  fn async_to_godot_impl(self) -> Result<Variant, String>;
}

impl<T: ToGodot, E: ErrorToGodot<T>> AsyncToGodotWrapper<Result<T, E>> {
  // High-priority impl, for `Result` only.
  pub fn async_to_godot_impl(self) -> Result<Variant, String> {
    match <E as ErrorToGodot<T>>::result_to_godot(self.0) {
      CallOutcome::Return(r) => Ok(r.to_variant()),
      CallOutcome::CallFailed(err) => Err(err),
    }
  }
}

impl<T: ToGodot> AsyncToGodot for AsyncToGodotWrapper<T> {
  fn async_to_godot_impl(self) -> Result<Variant, String> {
    Ok(self.0.to_variant())
  }
}
