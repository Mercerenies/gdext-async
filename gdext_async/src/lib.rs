
//! Provides utilities for bidirectional support between Rust and
//! GDScript for `async` and `await`.
//!
//! This crate provides two macros, both intended for interoperability
//! with GDScript: [`godot_async`] and [`godot_await`]. If you are
//! writing a pure-Rust project that does not need to inter-operate
//! with GDScript, then you probably do NOT need this crate.
//!
//! # Features
//!
//! * `macros` - Exposes the attribute macro `#[gdext_async]`. Enabled
//! by default. This requires the `syn` crate as a dependency.

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

/// If the object is a `GDScriptFunctionState`, await it. If not,
/// return it unmodified. Note that, unlike GDScript, this function
/// does NOT attempt to detect and await signals. Only actual
/// `GDScriptFunctionState` objects will be awaited.
///
/// This is the function used under-the-hood by the [`godot_await!`]
/// macro, but it can also be used independently, for instance, if you
/// need to store the future (as a [`Future`]) and not evaluate it
/// yet.
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
///
/// This macro returns a `Variant` which can be awaited on the
/// GDScript side. If this macro is the entire function body, your
/// function must return `Variant`.
///
/// It is strongly recommended that functions which use this macro
/// take `this: Gd<Self>` via `#[func(gd_self)]` (as opposed to
/// `&self` or `&mut self`) in order to avoid holding a `Gd` binding
/// across an `await` point.
///
/// # Example Usage
///
/// ```rust
/// # use godot::prelude::*;
/// # use gdext_async::godot_async;
/// // Rust
///
/// #[derive(GodotClass)]
/// #[class(init, base=Node)]
/// struct MyRustNode {}
///
/// #[godot_api]
/// impl MyRustNode {
///   #[func(gd_self)]
///   pub fn wait_three_secs(this: Gd<Self>) -> Variant {
///     godot_async! {
///       godot_print!("Before Timer! (2)");
///       let timer = this.get_tree().create_timer(3.0);
///       timer.signals().timeout().to_future().await;
///       godot_print!("After Timer! (3)");
///     }
///   }
/// }
/// ```
///
/// ```gdscript
/// # GDScript
/// extends Node
///
/// func _ready():
///     var node = MyRustNode.new()
///     add_child(node)
///     print("Before Timer! (1)")
///     await node.wait_three_secs()
///     print("After Timer! (4)")
/// ```
#[macro_export]
macro_rules! godot_async {
  ($($block: tt)+) => {
    $crate::AsyncAwaitableTask::create(async move {
      $($block)+
    }).bind_mut().get_maybe_awaitable_value()
  }
}

/// Await (in Rust) a Godot value, using semantics similar to
/// GDScript's `await`.
///
/// GDScript function state objects will be awaited, while any other
/// value (**including signals**) will be returned verbatim.
///
/// # Example Usage
///
/// ```rust
/// # use godot::prelude::*;
/// # use gdext_async::godot_await;
/// // Rust
///
/// // Note: Ordinary Rust async function, NOT exposed to GDScript.
/// pub async fn fade_out_enemy(mut enemy_node: Gd<Node2D>) {
///   godot_print!("Freeing enemy node ...");
///   godot_await! { enemy_node.call("fade_out", &[]) };
///   godot_print!("Enemy node freed.");
/// }
///
/// ```
///
/// ```gdscript
/// # GDScript
/// extends Node2D
///
/// func fade_out():
///     var tween = create_tween()
///     tween.tween_property(self, "modulate:a", 0.0, 1.0)
///     await tween.finished
///     queue_free()
/// ```
#[macro_export]
macro_rules! godot_await {
  ($($block: tt)+) => {
    $crate::resolve_gdscript_coroutine({ $($block)+ }).await
  }
}

/// Attribute macro that can be applied to `impl` blocks (similar to
/// [`godot_api`]) to enable optional functionality.
///
/// This macro can only be applied to an inherent impl block, not to a
/// trait impl. This macro generates `#[godot_api(secondary)]` blocks,
/// so the relevant struct must have a primary (i.e. non-`secondary`)
/// `#[godot_api]` block.
///
/// ## Async Functions (`#[async_func]` / `#[async_func(gd_self)]`)
///
/// A function in a `gdext_async`-enabled impl block which is tagged
/// with `#[async_func]` will be exported to Godot, similar to
/// `#[func]` on `#[godot_api]`. The exported Godot-side function will
/// be `await`-able in GDScript.
///
/// The annotated function must be `async` and its arguments and
/// return type must be Godot-compatible. That is, arguments must be
/// [`AsArg`](godot::meta::AsArg) and the result type must be
/// [`ToGodot`]. Note that this is a stronger guarantee than
/// `#[func]`, as it is not currently possible to return, for example,
/// `Result<T, E>` from a `#[async_func]` since the type does not
/// implement `ToGodot`.
///
/// `#[async_func]` can be called as `#[async_func(gd_self)]`, which
/// causes the generated GDScript-function to accept `Gd<Self>` rather
/// than an explicit `&self` or `&mut self`. If you are accepting a
/// `self` argument, you almost always want `gd_self`, as holding a
/// `Gd` binding across an `await` point is generally a very bad idea.
///
/// The function will be available both from Rust and from GDScript
/// and can be awaited using the respective `await` keyword in each
/// language.
///
/// `#[rpc]` functions are NOT currently supported by this macro.
///
/// ### Example
///
/// ```
/// # use godot::prelude::*;
/// # use gdext_async_macros::gdext_async;
///
/// #[derive(GodotClass)]
/// #[class(init, base=Node2D)]
/// pub struct Player {}
///
/// #[gdext_async]
/// #[godot_api]
/// impl Player {
///   #[async_func(gd_self)]
///   pub async fn play_item_get_animation(this: Gd<Self>, item_id: i32) -> i32 {
///     // Function body ...
///     unimplemented!()
///   }
///
///   // Also works with static functions
///   #[async_func]
///   pub async fn calculate_item_damage(item_id: i32) -> f32 {
///     unimplemented!()
///   }
/// }
/// ```
#[doc(alias = "async_func")]
#[cfg(feature = "macros")]
#[cfg_attr(docsrs, doc(cfg(feature = "macros")))]
pub use gdext_async_macros::gdext_async;
