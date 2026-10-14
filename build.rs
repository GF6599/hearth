use std::env;
use std::fs;
use std::path::{Path, PathBuf};

/// Lists every `.rs` file under `src` and `tests` into a table of
/// `include_str!` entries, so the typing scene can type the crate's own
/// source without a checkout at run time.
fn main() {
    let (Ok(root), Ok(out)) = (env::var("CARGO_MANIFEST_DIR"), env::var("OUT_DIR")) else {
        eprintln!("build.rs: cargo did not set CARGO_MANIFEST_DIR and OUT_DIR");
        std::process::exit(1);
    };
    let root = PathBuf::from(root);
    let mut paths = Vec::new();
    for dir in ["src", "tests"] {
        println!("cargo:rerun-if-changed={dir}");
        collect(&root.join(dir), &root, &mut paths);
    }
    paths.sort();

    let mut code = String::from("static EMBEDDED: &[(&str, &str)] = &[\n");
    for relative in &paths {
        let absolute = root.join(relative).display().to_string();
        code.push_str(&format!(
            "    ({relative:?}, include_str!({absolute:?})),\n"
        ));
    }
    code.push_str("];\n");
    if let Err(e) = fs::write(PathBuf::from(out).join("embedded.rs"), code) {
        eprintln!("build.rs: cannot write embedded.rs: {e}");
        std::process::exit(1);
    }
}

fn collect(dir: &Path, root: &Path, out: &mut Vec<String>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, root, out);
        } else if path.extension().is_some_and(|ext| ext == "rs")
            && let Ok(relative) = path.strip_prefix(root)
        {
            let relative: Vec<String> = relative
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect();
            out.push(relative.join("/"));
        }
    }
}
