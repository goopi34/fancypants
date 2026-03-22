fn main() {
    let version = std::env::var("FANCYPANTS_VERSION")
        .unwrap_or_else(|_| env!("CARGO_PKG_VERSION").to_string());
    println!("cargo:rustc-env=FANCYPANTS_VERSION={}", version);
    println!("cargo:rerun-if-env-changed=FANCYPANTS_VERSION");
}
