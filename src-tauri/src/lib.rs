//! Tauri shell: plugins, commands, the `thumb` protocol and backend state.
//! All real logic lives in `texopt-core` or in the Tauri-independent modules
//! below (`jobs`, `session`, `preview`, `thumb_protocol`), which are unit
//! tested without a running app.

pub mod commands;
pub mod commands_atlas;
pub mod commands_mesh;
pub mod commands_rename;
pub mod error;
pub mod jobs;
pub mod mesh_worker;
pub mod preview;
pub mod session;
pub mod thumb_protocol;

use tauri::Manager;

use crate::thumb_protocol::ThumbService;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .manage(commands::AppState::default())
        .manage(commands_mesh::MeshState::default())
        .manage(ThumbService::new())
        .register_asynchronous_uri_scheme_protocol(
            thumb_protocol::SCHEME,
            |ctx, request, responder| {
                let app = ctx.app_handle().clone();
                let uri = request.uri().to_string();
                let service = app.state::<ThumbService>();
                let cache = service.cache(|| {
                    app.path()
                        .app_cache_dir()
                        .unwrap_or_else(|_| std::env::temp_dir().join("texture-optimizer"))
                        .join("thumbs")
                });
                service
                    .spawn(move || responder.respond(thumb_protocol::thumb_response(&cache, &uri)));
            },
        )
        .invoke_handler(tauri::generate_handler![
            commands::scan_paths,
            commands::preview_op,
            commands::run_op,
            commands::cancel_job,
            commands::release_session,
            commands_atlas::atlas_preview,
            commands_atlas::atlas_export,
            commands_atlas::atlas_load_project,
            commands_mesh::mesh_scan,
            commands_mesh::mesh_preview_pack,
            commands_mesh::mesh_pack,
            commands_rename::rename_plan,
            commands_rename::rename_execute,
            commands_rename::rename_revert_last,
            commands_rename::rename_revert,
            commands_rename::rename_logs,
        ])
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while building tauri application");
}
