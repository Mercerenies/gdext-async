
//! Provides proc macros for the `gdext_async` crate. Should generally
//! be used through that API.

mod async_func;

use proc_macro::TokenStream;

#[proc_macro_attribute]
pub fn async_func(meta: TokenStream, input: TokenStream) -> TokenStream {
  let input = syn::parse_macro_input!(input as syn::ItemFn);
  match async_func::compile_async_func(meta, input) {
    Ok(tokens) => tokens.into(),
    Err(err) => {
      err.into_compile_error().into()
    }
  }
}
