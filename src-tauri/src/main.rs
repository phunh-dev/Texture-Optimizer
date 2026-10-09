// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // 3D worker mode (child process started by the mesh commands).
    if let Some(code) = texture_optimizer_lib::mesh_worker::run_if_requested() {
        std::process::exit(code);
    }
    texture_optimizer_lib::run();
}
