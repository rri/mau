//! rust-embed needs `web/build` to exist at compile time. Create an empty one
//! so `cargo check` works before the web app has been built.
fn main() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../web/build");
    if !dir.exists() {
        std::fs::create_dir_all(&dir).expect("creating web/build");
    }
    println!("cargo:rerun-if-changed=../../web/build");
}
