fn main() {
    // memory-serve's load_assets! embeds frontend/dist at compile time; without
    // this, a trunk rebuild with new hashed filenames leaves a stale asset map.
    println!("cargo:rerun-if-changed=../frontend/dist");
}
