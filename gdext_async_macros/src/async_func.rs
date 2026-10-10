
use proc_macro::TokenStream;
use proc_macro2::{TokenStream as TokenStream2};
use quote::{ToTokens, quote, format_ident};

struct GdSelf;

fn detect_gd_self(meta: TokenStream) -> Result<Option<GdSelf>, syn::Error> {
  if meta.is_empty() {
    return Ok(None);
  };
  let Ok(ident) = syn::parse::<syn::Ident>(meta.clone()) else {
    return Err(syn::Error::new_spanned(TokenStream2::from(meta), "expected a single identifier"));
  };
  if ident == "gd_self" {
    Ok(Some(GdSelf))
  } else {
    Err(syn::Error::new_spanned(TokenStream2::from(meta), "expected 'gd_self'"))
  }
}

pub(crate) fn compile_async_func(meta: TokenStream, input: syn::ItemFn) -> Result<TokenStream, syn::Error> {
  let gd_self = detect_gd_self(meta.clone())?;
  if gd_self.is_none() {
    let mut err = syn::Error::new_spanned(TokenStream2::from(meta.clone()), "expected 'gd_self' in #[gd_async]");
    err.combine(syn::Error::new_spanned(TokenStream2::from(meta), "Non-`gd_self` methods may be supported in the future"));
    return Err(err);
  }

  let delegating_function = compile_delegating_function(&input)?;

  Ok(quote! {
    // Original Rust-side implementation
    #input

    // GDScript delegator
    #delegating_function
  }.into())
}

fn compile_delegating_function(original_func: &syn::ItemFn) -> Result<TokenStream2, syn::Error> {
  let original_func_name = original_func.sig.ident.clone();
  let delegating_func_name = format_ident!("__godot_async_func_delegate__{original_func_name}");
  let args = original_func.sig.inputs.clone();

  let rust_func_arg_names: Vec<_> = (0..args.len())
    .map(|n| format_ident!("arg{n}").into_token_stream())
    .collect();
  let rust_func_formal_params = args.iter().zip(rust_func_arg_names.iter())
    .map(|(arg_data, arg_name)| {
      let syn::FnArg::Typed(arg_data) = arg_data else {
        return Err(syn::Error::new_spanned(arg_data, "`self` argument not allowed in `#[async_func(gd_self)]`"));
      };
      let ty = arg_data.ty.clone();
      Ok(quote! { #arg_name: #ty })
    })
    .collect::<Result<Vec<_>, _>>()?;

  Ok(quote! {
    #[func(rename = #original_func_name)]
    fn #delegating_func_name(#(#rust_func_formal_params,)*) -> ::godot::builtin::Variant {
      ::gdext_async::godot_async! {
        Self::#original_func_name(
          #(#rust_func_arg_names,)*
        ).await
      }
    }
  })
}
