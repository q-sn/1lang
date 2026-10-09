fn main() {
    // Icons are embedded into the binary: rebuild when they change.
    println!("cargo:rerun-if-changed=icons");
    tauri_build::build()
}
