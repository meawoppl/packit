fn main() {
    memory_serve::load_directory("../frontend/dist");
    // embed_migrations! only tracks existing files, not new migration folders.
    println!("cargo:rerun-if-changed=migrations");
}
