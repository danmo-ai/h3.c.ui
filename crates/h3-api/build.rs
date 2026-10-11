use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let repo_root = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .expect("repo root")
        .to_path_buf();
    let dylib = repo_root.join("libh3.dylib");
    println!("cargo:rerun-if-changed={}", dylib.display());

    // LC_RPATH must be set on the final binary (h3-api), not only h3-sys.
    println!("cargo:rustc-link-arg=-Wl,-rpath,@loader_path");
    println!(
        "cargo:rustc-link-arg=-Wl,-rpath,{}",
        repo_root.display()
    );

    let profile = env::var("PROFILE").unwrap_or_else(|_| "debug".into());
    let dest_dir = repo_root.join("crates/target").join(&profile);
    if dylib.is_file() {
        let _ = fs::create_dir_all(&dest_dir);
        let dest = dest_dir.join("libh3.dylib");
        let _ = fs::copy(&dylib, &dest);
        if let Ok(out) = env::var("OUT_DIR") {
            let out = PathBuf::from(out);
            if let Some(debug_dir) = out.ancestors().nth(3) {
                let _ = fs::copy(&dylib, debug_dir.join("libh3.dylib"));
            }
        }
    }
}
