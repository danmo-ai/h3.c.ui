use std::env;
use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let repo_root = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .expect("crates/h3-sys -> repo root")
        .to_path_buf();

    let header = repo_root.join("h3.h");
    let dylib = repo_root.join("libh3.dylib");

    println!("cargo:rerun-if-changed={}", header.display());
    println!("cargo:rerun-if-changed={}", dylib.display());
    println!("cargo:rustc-link-search=native={}", repo_root.display());
    println!("cargo:rustc-link-lib=dylib=h3");

    // Metal / system frameworks used by libh3
    println!("cargo:rustc-link-lib=framework=Foundation");
    println!("cargo:rustc-link-lib=framework=Metal");
    println!("cargo:rustc-link-lib=framework=MetalPerformanceShaders");
    println!("cargo:rustc-link-lib=framework=MetalPerformanceShadersGraph");
    println!("cargo:rustc-link-lib=framework=Accelerate");
    println!("cargo:rustc-link-lib=icucore");

    // Runtime search: beside the binary, then the repo root (dev builds).
    println!("cargo:rustc-link-arg=-Wl,-rpath,@loader_path");
    println!(
        "cargo:rustc-link-arg=-Wl,-rpath,{}",
        repo_root.display()
    );

    let bindings = bindgen::Builder::default()
        .header(header.to_string_lossy())
        .allowlist_function("h3_.*")
        .allowlist_type("h3_.*")
        .allowlist_var("H3_.*")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate()
        .expect("Unable to generate h3 bindings");

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings");
}
