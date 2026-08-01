fn optional_env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn emit_compile_time_env(name: &str) {
    if let Some(value) = optional_env(name) {
        println!("cargo:rustc-env={name}={value}");
        println!("cargo:rerun-if-env-changed={name}");
    }
}

fn main() {
    emit_compile_time_env("MLT_ENV");
    emit_compile_time_env("MLT_SUPABASE_URL");
    emit_compile_time_env("MLT_SUPABASE_ANON_KEY");
    tauri_build::build();
}
