
use std::{fs, path::Path, collections::HashMap, fmt::Write, unreachable};

use proc_macro::TokenStream;
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
    let arg = match litrs::StringLit::try_from(&first_token) {
        Ok(string_lit) => string_lit.into_value(),
        Err(e) => return e.to_compile_error(),
    };


    // Figure out paths, load file and validate it.
    let wgsl_path = {
        let ref_path = first_token.span().local_file().expect("no local path");
        let ref_dir = ref_path.parent().expect("source file path has no parent");
        ref_dir.join(&arg).to_string()
    };
    let (wgsl, tracked_paths) = match load(&wgsl_path) {
        Ok(loaded) => loaded,
        Err(e) => return compile_err(&e),
    };
    if let Err(e) = validate(&wgsl_path, &wgsl) {
        return compile_err(&e);
    }


    // Create output
    let tracked_paths = tracked_paths.into_iter().chain([arg]);
    quote! {{
        #( include_bytes!(#tracked_paths); )*
        wgpu::ShaderModuleDescriptor {
            label: std::option::Option::Some(#wgsl_path),
            source: wgpu::ShaderSource::Wgsl(#wgsl.into()),
        }
    }}.into()
}


fn load(path: &str) -> Result<(String, Vec<String>), String> {
    let mut files = HashMap::new();
    load_impl(path, &mut files)?;
    let Some(LoadState::Loaded(code)) = files.remove(path) else { unreachable!() };
    let paths = files.keys().cloned().collect::<Vec<_>>();

    Ok((code, paths))
}

enum LoadState {
    Loading,
    Loaded(String),
}


fn load_impl<'a>(path: &str, files: &'a mut HashMap<String, LoadState>) -> Result<(), String> {
    match files.get(path) {
        None => {}
        // TODO: improve error message
        Some(LoadState::Loading) => return Err(format!("circular include in {path}")),
        Some(LoadState::Loaded(_)) => return Ok(()),
    }
    files.insert(path.into(), LoadState::Loading);

    let raw = fs::read_to_string(&path)
        .map_err(|e| format!("could not load file '{}': {}", path, e))?;

    let needle = "#include \"";
    let mut out = String::new();
    for (i, line) in raw.lines().enumerate() {
        let line_num = i + 1;
        if !line.starts_with(needle) {
            write!(out, "{line}\n").unwrap();
            continue;
        }

        let start_path = needle.len();
        let end_path = line[start_path..].find('"').ok_or_else(|| {
            // TODO
            format!("undelimited include (missing \") in '{path}:{line_num}'")
        })?;

        // Resolve path
        let included_path = &line[start_path..][..end_path];
        let resolved_included_path = Path::new(path).parent().unwrap()
            .join(included_path)
            .canonicalize()
            .map_err(|e| format!("failed to canonicalize path: {e}"))?
            .to_string();

        // Load included file
        load_impl(&resolved_included_path, files)?;
        let content = match &files[&resolved_included_path] {
            LoadState::Loaded(c) => c,
            _ => unreachable!(),
        };
        write!(out, "{content}\n").unwrap();
    };

    files.insert(path.into(), LoadState::Loaded(out));
    Ok(())
}

fn validate(path: &str, wgsl: &str) -> Result<(), String> {
    let module = naga::front::wgsl::parse_str(&wgsl).map_err(|e| {
        // This is tricky: We currently print to stderr immediately which
        // results in nicer errors with better colors. However, this completely
        // bypasses rustc, meaning that this error will likely not be shown in
        // IDEs.
        e.emit_to_stderr_with_path(&wgsl, &path);
        format!("Parse errors occured in '{path}'")
    })?;

    let mut validator = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        // TODO: this might be something to let the user configure later.
        naga::valid::Capabilities::all(),
    );
    validator.validate(&module).map_err(|e| {
        e.emit_to_stderr_with_path(&wgsl, &path);
        format!("Validation errors occured in '{path}'")
    })?;

    Ok(())
}

fn compile_err(message: &str) -> TokenStream {
    quote! {{
        compile_error!(#message);

        // We still create a value to prevent type errors down the line. They
        // are just not useful.
        wgpu::ShaderModuleDescriptor {
            label: std::option::Option::Some("dummy error placeholder"),
            source: wgpu::ShaderSource::Wgsl("".into()),
        }
    }}.into()
}

trait PathExt {
    fn to_string(&self) -> String;
}

impl PathExt for Path {
    fn to_string(&self) -> String {
        self.to_str().expect("path is not valid UTF8").to_owned()
    }
}
