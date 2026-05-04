pub mod atomic_write;
pub mod commands;
pub mod indexer;
pub mod search;
pub mod vault;
pub mod watcher;

use commands::AppState;
use tauri::Manager;
use tokio::sync::Mutex;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(Mutex::new(AppState::default()))
        .invoke_handler(tauri::generate_handler![
            // Vault
            commands::open_vault,
            commands::close_vault,
            commands::get_tree,
            commands::read_note,
            commands::create_note,
            commands::update_note,
            commands::delete_note,
            commands::rename_note,
            commands::create_folder,
            // Indexer
            commands::rebuild_index,
            commands::get_note,
            commands::get_backlinks,
            commands::get_graph_data,
            commands::get_tags,
            commands::get_notes_by_tag,
            commands::resolve_wikilink,
            // Search
            commands::search,
            commands::search_by_tag,
            commands::suggest,
            // Watcher
            commands::start_watcher,
            commands::stop_watcher,
        ])
        .setup(|app| {
            #[cfg(debug_assertions)]
            {
                let window = app.get_webview_window("main").unwrap();
                window.open_devtools();
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
