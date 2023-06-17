#![feature(proc_macro_span)]
#![feature(track_path)]


use std::{fs, path::Path};

use proc_macro::{TokenStream, tracked_path};
use quote::quote;



#[proc_macro]
pub fn include_wgsl(input: TokenStream) -> TokenStream {
    // Validate input and get the path argument as string.
    let mut it = input.into_iter();
    let Some(first_token) = it.next() else {
        return compile_err("empty input, but expected string literal");
    };
    if it.next().is_some() {
        return compile_err("expected single string literal, but found additional token");
    }
    let wgsl_path = match litrs::StringLit::try_from(&first_token) {
        Ok(string_lit) => string_lit.into_value().into_owned(),
        Err(e) => return e.to_compile_error(),
    };


    // Figure out paths and load file
    let ref_path = first_token.span().source_file().path();
    let ref_dir = ref_path.parent().expect("source file path has no parent");
    let wgsl = match load(&ref_dir, Path::new(&wgsl_path)) {
        Ok(loaded) => loaded,
        Err(e) => return compile_err(&e),
    };


    // Create output
    let full_path = ref_dir.join(wgsl_path).display().to_string();
    quote! {{
        wgpu::ShaderModuleDescriptor {
            label: std::option::Option::Some(#full_path),
            source: wgpu::ShaderSource::Wgsl(#wgsl.into()),
        }
    }}.into()
}


fn load(ref_dir: &Path, path: &Path) -> Result<String, String> {
    let resolved_path = ref_dir.join(path);
    tracked_path::path(resolved_path.to_str().unwrap());
    let wgsl = fs::read_to_string(&resolved_path)
        .map_err(|e| format!("could not load file '{}': {}", resolved_path.display(), e))?;

    Ok(wgsl)
}

fn compile_err(message: &str) -> TokenStream {
    quote! {{
        compile_error!(#message);
    }}.into()
}
