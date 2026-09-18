
use godot::prelude::*;

use std::cell::OnceCell;
use std::rc::Rc;

#[derive(GodotClass, Debug)]
#[class(no_init)]
#[doc(hidden)] // Only public for use in macro.
pub struct AsyncAwaitableTask {
  base: Base<RefCounted>,
  known_result: Rc<OnceCell<Variant>>,
}

impl AsyncAwaitableTask {
  pub fn create<F>(future: F) -> Gd<Self>
  where F: IntoFuture + 'static,
        <F as IntoFuture>::Output: ToGodot {
    let known_result = Rc::new(OnceCell::new());
    let inst = Gd::from_init_fn(|base| {
      AsyncAwaitableTask { base, known_result: Rc::clone(&known_result) }
    });
    {
      let inst = inst.clone(); // RefCounted: Cheap clone
      let _task_handle = godot::task::spawn(async move {
        let res = future.into_future().await;
        known_result.set(res.to_variant())
          .expect("known_result was set twice!");
        inst.signals().completed().emit(&res.to_variant());
      });
    }
    inst
  }

  /// If the task is finished already, return its result. Else, return
  /// a signal that will fire when it finishes.
  pub fn get_maybe_awaitable_value(&mut self) -> Variant {
    if let Some(res) = self.known_result.get() {
      res.clone()
    } else {
      self.signals().completed().to_untyped().to_variant()
    }
  }
}

#[godot_api]
impl AsyncAwaitableTask {
  #[signal]
  pub fn completed(result: Variant);
}

/// If the object is a GDScriptFunctionState, await it. If not, return
/// it unmodified. Note that, unlike GDScript, this function does NOT
/// attempt to detect and await signals. Only actual
/// GDScriptFunctionState objects will be awaited.
pub async fn resolve_gdscript_coroutine(variant: Variant) -> Variant {
  // This function is adapted from
  // [https://github.com/godot-rust/gdext/pull/1645/changes] and uses
  // undocumented Godot trickery to adapt GDScript's `await` keyword
  // into Rust.
  if let Some(state) = to_gdscript_function_state(&variant) {
    let signal = Signal::from_object_signal(&state, "completed");
    let (result,) = signal.to_future::<(Variant,)>().await;
    drop(state);
    result
  } else {
    variant
  }
}

fn to_gdscript_function_state(variant: &Variant) -> Option<Gd<Object>> {
  let state = variant.try_to::<Gd<Object>>().ok()?;
  if state.get_class() == "GDScriptFunctionState" {
    Some(state)
  } else {
    None
  }
}

/// Convert a block of Rust async code into a Godot task whose
/// completion can safely be awaited from GDScript.
#[macro_export]
macro_rules! godot_async {
  ($($block: tt)+) => {
    $crate::AsyncAwaitableTask::create(async move {
      $($block)+
    }).bind_mut().get_maybe_awaitable_value()
  }
}

/// Await (in Rust) a Godot value, using semantics similar to
/// GDScript's `await`. GDScript function state objects will be
/// awaited, while any other value (**including signals**) will be
/// returned verbatim.
#[macro_export]
macro_rules! godot_await {
  ($($block: tt)+) => {
    $crate::resolve_gdscript_coroutine({ $($block)+ }).await
  }
}
