
//! Provides proc macros for the `gdext_async` crate. Should generally
//! be used through that API.

mod async_func;

use proc_macro::TokenStream;

#[proc_macro_attribute]
pub fn gdext_async(_meta: TokenStream, input: TokenStream) -> TokenStream {
  let input = syn::parse_macro_input!(input as syn::ItemImpl);

  if let Some((_, trait_, _)) = input.trait_ {
    let err = syn::Error::new_spanned(trait_, "`gdext_async` macro not supported on trait impls");
    return err.into_compile_error().into();
  }

  match async_func::compile_gdext_async(input) {
    Ok(tokens) => tokens.into(),
    Err(err) => {
      err.into_compile_error().into()
    }
  }
}
