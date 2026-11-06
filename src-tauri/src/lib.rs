mod commands;

use commands::{
    AppState, add_entries, archive_info, cancel_job, create_archive, extract_archive,
    format_capabilities, join_volumes_command, list_archive, remove_entries, split_archive_volumes,
    test_archive,
};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            format_capabilities,
            list_archive,
            archive_info,
            create_archive,
            extract_archive,
            test_archive,
            add_entries,
            remove_entries,
            split_archive_volumes,
            join_volumes_command,
            cancel_job
        ])
        .run(tauri::generate_context!())
        .expect("error while running Archon");
}
