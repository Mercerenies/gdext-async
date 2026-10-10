
use proc_macro::TokenStream;
use proc_macro2::{TokenStream as TokenStream2};
use quote::{ToTokens, quote, format_ident};

struct AsyncFuncAttr {
  gd_self: bool,
}

fn detect_gd_self(meta: TokenStream) -> Result<bool, syn::Error> {
  if meta.is_empty() {
    return Ok(false);
  };
  let Ok(ident) = syn::parse::<syn::Ident>(meta.clone()) else {
    return Err(syn::Error::new_spanned(TokenStream2::from(meta), "expected a single identifier"));
  };
  if ident == "gd_self" {
    Ok(true)
  } else {
    Err(syn::Error::new_spanned(TokenStream2::from(meta), "expected 'gd_self'"))
  }
}

fn parse_meta_attr(attr: &syn::Attribute) -> Result<Option<AsyncFuncAttr>, syn::Error> {
  let (path, args) = match &attr.meta {
    syn::Meta::Path(path) => (path, None),
    syn::Meta::List(list) => (&list.path, Some(&list.tokens)),
    syn::Meta::NameValue(_) => {
      return Ok(None)
    }
  };
  if !is_async_func_attr(&path) {
    return Ok(None);
  }
  let Some(args) = args else {
    // No args, so use defaults.
    return Ok(Some(AsyncFuncAttr { gd_self: false }));
  };
  let gd_self = detect_gd_self(args.clone().into())?;
  Ok(Some(AsyncFuncAttr { gd_self }))
}

fn is_async_func_attr(path: &syn::Path) -> bool {
  if path.segments.len() != 1 {
    return false;
  }
  path.segments[0].ident == "async_func"
}

fn remove_async_func_meta_attr(func: &mut syn::ImplItem) {
  let syn::ImplItem::Fn(func) = func else { return; };
  let Some(matching_index) = func.attrs.iter().position(|attr| is_async_func_attr(attr.path())) else { return; };
  func.attrs.remove(matching_index);
}

pub(crate) fn compile_gdext_async(mut input: syn::ItemImpl) -> Result<TokenStream, syn::Error> {
  let self_ty = input.self_ty.clone();

  let mut delegators = Vec::new();
  for item in &input.items {
    let Some(meta) = needs_delegating_function(item)? else {
      continue;
    };
    let syn::ImplItem::Fn(item) = item else {
      // TODO Just return this from the needs_delegating_function call.
      unreachable!()
    };
    let delegator = compile_delegating_function(&meta, item)?;
    delegators.push(delegator);
  }

  // Remove our attributes from the input.
  for item in &mut input.items {
    remove_async_func_meta_attr(item);
  }

  Ok(quote! {
    // Original Rust-side implementation
    #input

    // GDScript delegators
    #[::godot::prelude::godot_api(secondary)]
    impl #self_ty {
      #(#delegators)*
    }
  }.into())
}

fn needs_delegating_function(item: &syn::ImplItem) -> Result<Option<AsyncFuncAttr>, syn::Error> {
  let syn::ImplItem::Fn(item) = item else { return Ok(None); };
  for attr in &item.attrs {
    let meta = parse_meta_attr(attr)?;
    if let Some(meta) = meta {
      return Ok(Some(meta));
    }
  }
  Ok(None)
}

fn compile_delegating_function(meta: &AsyncFuncAttr, original_func: &syn::ImplItemFn) -> Result<TokenStream2, syn::Error> {
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

  let outer_func_attribute = if meta.gd_self {
    quote! { #[func(gd_self, rename = #original_func_name)] }
  } else {
    quote! { #[func(rename = #original_func_name)] }
  };

  Ok(quote! {
    #outer_func_attribute
    fn #delegating_func_name(#(#rust_func_formal_params,)*) -> ::godot::builtin::Variant {
      ::gdext_async::godot_async! {
        Self::#original_func_name(
          #(#rust_func_arg_names,)*
        ).await
      }
    }
  })
}
