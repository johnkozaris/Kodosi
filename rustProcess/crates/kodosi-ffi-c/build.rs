#![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]

use std::env;
use std::path::PathBuf;

fn main() {
    let crate_dir =
        env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR must be set by cargo");

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR must be set by cargo"));
    let out_path = out_dir.join("kodosi_runtime.h");

    let config = cbindgen::Config::from_file(PathBuf::from(&crate_dir).join("cbindgen.toml"))
        .unwrap_or_else(|e| panic!("cbindgen: failed to load cbindgen.toml: {e}"));

    let bindings = cbindgen::Builder::new()
        .with_crate(&crate_dir)
        .with_config(config)
        .generate()
        .unwrap_or_else(|e| panic!("cbindgen: generation failed: {e}"));

    let mut header = Vec::new();
    bindings.write(&mut header);
    let header = String::from_utf8(header).expect("cbindgen header must be UTF-8");
    let header = header
        .replace("#endif // __cplusplus", "#endif")
        .replace("}  // extern \"C\"", "}")
        .replace("#endif  // __cplusplus", "#endif")
        .replace("#endif  /* KODOSI_RUNTIME_H */", "#endif");
    let temporary = out_dir.join(format!(".kodosi_runtime.h.{}", std::process::id()));
    std::fs::write(&temporary, header).expect("write generated C header");
    std::fs::rename(&temporary, &out_path).expect("publish generated C header");
    println!("cargo:rerun-if-changed=src/lib.rs");
    println!("cargo:rerun-if-changed=cbindgen.toml");
}
