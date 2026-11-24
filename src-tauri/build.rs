fn main() {
    // Sem isso, trocar os ícones não refaz o recurso embutido no .exe.
    println!("cargo:rerun-if-changed=icons");
    tauri_build::build()
}
