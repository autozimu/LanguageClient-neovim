use convert_case::{Case, Casing};
use proc_macro2::TokenStream;
use quote::quote;
use syn::{export::quote::quote_spanned, DeriveInput, FieldsNamed, Lit, NestedMeta};
use syn::{spanned::Spanned, Meta};

#[proc_macro_derive(VimEval, attributes(vim_eval))]
pub fn lcn_config(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = syn::parse_macro_input!(input as DeriveInput);
    let struct_name = input.ident;
    let mut body = quote! { map };
    if let syn::Data::Struct(s) = input.data {
        if let syn::Fields::Named(f) = s.fields {
            body = get_body(f);
        }
    }

    let expanded = quote! {
        impl #struct_name {
            pub fn to_vim_evaluatable() -> String {
                return vec![
                    "{",
                    #body
                    "}",
                ].join("").to_string();
            }
        }
    };

    proc_macro::TokenStream::from(expanded)
}

fn get_body(fields: FieldsNamed) -> TokenStream {
    let ret = fields.named.iter().map(|f| {
        let field_name = f.ident.clone().unwrap().to_string();
        let mut evaluator = get_default_eval(&field_name);
        for attr in f.attrs.iter() {
            if attr.path.get_ident().unwrap().to_string().as_str() == "vim_eval" {
                let meta = attr.parse_meta().unwrap();
                evaluator = get_evaluator(&field_name, &meta);
            }
        }

        let val = format!(r#""{}": {},"#, field_name, evaluator);
        quote_spanned! { f.span()=>
            #val,
        }
    });

    quote! {
        #(#ret)*
    }
}

fn get_evaluator(field_name: &str, meta: &Meta) -> String {
    let mut field_name = format!("'LanguageClient_{}'", field_name.to_case(Case::Camel));
    let mut default = "v:null".to_owned();
    if let Meta::List(list) = meta {
        for nested_meta in &list.nested {
            if let NestedMeta::Meta(meta) = nested_meta {
                if let Meta::NameValue(nv) = meta {
                    if nv.path.segments.len() > 0
                        && nv.path.segments.first().unwrap().ident.to_string().as_str()
                            == "eval_with"
                    {
                        if let Lit::Str(val) = &nv.lit {
                            // return early if eval_with is included, as that should take
                            // precedence over rename and default.
                            return val.value();
                        }
                    }
                }

                if let Meta::NameValue(nv) = meta {
                    if nv.path.segments.len() > 0
                        && nv.path.segments.first().unwrap().ident.to_string().as_str() == "rename"
                    {
                        if let Lit::Str(val) = &nv.lit {
                            field_name = val.value();
                        }
                    }
                }

                if let Meta::NameValue(nv) = meta {
                    if nv.path.segments.len() > 0
                        && nv.path.segments.first().unwrap().ident.to_string().as_str() == "default"
                    {
                        if let Lit::Str(val) = &nv.lit {
                            default = val.value();
                        }
                    }
                }
            }
        }

        return format!("get(g:, {}, {})", field_name, default);
    }

    return format!("get(g:, {})", field_name);
}

fn get_default_eval(field_name: &str) -> String {
    return format!(
        "get(g:, 'LanguageClient_{}', v:null)",
        field_name.to_case(Case::Camel)
    );
}
