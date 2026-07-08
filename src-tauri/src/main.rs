#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use resona_lib::audio::player::Player;
use resona_lib::audio::lufs::LufsCache;
use resona_lib::commands;
use resona_lib::discord_rpc;
use resona_lib::library::artist_artwork::ArtistArtworkCache;
use resona_lib::library::artist_artwork_worker::ArtistArtworkWorker;
use resona_lib::library::track_artwork::TrackArtworkCache;
use resona_lib::library::db;
use resona_lib::remote;
use resona_lib::search::tantivy_index::TantivyIndex;
use resona_lib::library::scanner;
use resona_lib::library::eq;
use resona_lib::state::AppState;
#[cfg(windows)]
use resona_lib::gpu::artwork_engine::GpuArtworkEngine;
#[cfg(windows)]
use resona_lib::windows::taskbar::TaskbarManager;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::Emitter;
use tauri::Manager;
use tauri::tray::{TrayIconBuilder, MouseButton, MouseButtonState, TrayIconEvent};
use tauri::menu::{Menu, MenuItemBuilder, PredefinedMenuItem};
use tauri_plugin_global_shortcut::{Code, Modifiers, Shortcut, ShortcutEvent, ShortcutState, GlobalShortcutExt};

fn app_data_dir(app: &tauri::App) -> PathBuf {
    app.path()
        .app_data_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
}

fn main() {
    env_logger::init();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, None))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .setup(|app| {
            let data_dir = app_data_dir(app);
            std::fs::create_dir_all(&data_dir).ok();
            let db_path = data_dir.join("library.db");
            let pool = tauri::async_runtime::block_on(db::init_pool(&db_path))
                .expect("Failed to init database");
            
            // Initialize Tantivy full-text search index (Phase 2)
            let search_index_dir = data_dir.join("search_index");
            std::fs::create_dir_all(&search_index_dir).ok();
            let search = TantivyIndex::new(&search_index_dir)
                .expect("Failed to init tantivy search index");

            let (player, volume) = Player::new();

            let artwork_cache_dir = data_dir.join("artist_artwork");
            let artist_cache = Arc::new(ArtistArtworkCache::new(artwork_cache_dir));
            let artist_worker = Arc::new(ArtistArtworkWorker::new());
            let track_artwork_cache = Arc::new(TrackArtworkCache::new(data_dir.join("track_artwork")));
            
            // Initialize LUFS cache (Phase 2)
            let lufs_cache = Arc::new(std::sync::Mutex::new(LufsCache::new()));

            #[cfg(windows)]
            let gpu_engine = tauri::async_runtime::block_on(GpuArtworkEngine::initialize())
                .unwrap_or_else(|e| {
                    log::warn!("Failed to initialize GPU engine: {}", e);
                    Arc::new(GpuArtworkEngine::default_for_fallback())
                });

            let state = AppState {
                player: Arc::new(player),
                db: pool.clone(),
                search: Arc::new(std::sync::Mutex::new(search)),
                playlist_queue: Arc::new(std::sync::Mutex::new(Vec::new())),
                volume: volume.clone(),
                discord_rpc: discord_rpc::DiscordRPC::new(),
                remote: remote::RemoteControl::new(),
                artwork_cache: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
                artist_artwork_cache: artist_cache.clone(),
                artist_artwork_worker: artist_worker.clone(),
                track_artwork_cache: track_artwork_cache.clone(),
                is_scanning: Arc::new(std::sync::atomic::AtomicBool::new(false)),
                is_fetching: Arc::new(std::sync::atomic::AtomicBool::new(false)),
                current_track_id: Arc::new(std::sync::Mutex::new(None)),
                lufs_cache: Some(lufs_cache), // Phase 2
                #[cfg(windows)]
                gpu_engine: Some(gpu_engine),
            };
            app.manage(state);

            // Seed default EQ profiles if table is empty
            let db_pool = app.state::<AppState>().db.clone();
            let _ = tauri::async_runtime::block_on(eq::seed_defaults(&db_pool));

            // Global shortcuts (media keys)
            {
                let state = app.state::<AppState>();
                let player_shortcut = state.player.clone();
                let toggle_shortcut = Shortcut::new(Some(Modifiers::empty()), Code::MediaPlayPause);
                let next_shortcut = Shortcut::new(Some(Modifiers::empty()), Code::MediaTrackNext);
                let prev_shortcut = Shortcut::new(Some(Modifiers::empty()), Code::MediaTrackPrevious);
                let stop_shortcut = Shortcut::new(Some(Modifiers::empty()), Code::MediaStop);

                let _ = app.global_shortcut().on_shortcut(toggle_shortcut, move |_app, _shortcut, event: ShortcutEvent| {
                    if event.state == ShortcutState::Pressed {
                        player_shortcut.toggle();
                    }
                });
                let _ = app.global_shortcut().on_shortcut(next_shortcut, move |_a, _s, event: ShortcutEvent| {
                    if event.state == ShortcutState::Pressed {
                        let _ = _a.emit("media-key", "NextTrack");
                    }
                });
                let _ = app.global_shortcut().on_shortcut(prev_shortcut, move |_a, _s, event: ShortcutEvent| {
                    if event.state == ShortcutState::Pressed {
                        let _ = _a.emit("media-key", "PreviousTrack");
                    }
                });
                let _ = app.global_shortcut().on_shortcut(stop_shortcut, move |_a, _s, event: ShortcutEvent| {
                    if event.state == ShortcutState::Pressed {
                        let _ = _a.emit("media-key", "Stop");
                    }
                });
            }

            // Tray icon
            let show = MenuItemBuilder::with_id("show", "Show")
                .accelerator("CmdOrCtrl+Shift+M")
                .build(app)?;
            let quit = MenuItemBuilder::with_id("quit", "Quit")
                .accelerator("CmdOrCtrl+Q")
                .build(app)?;
            let separator = PredefinedMenuItem::separator(app)?;
            let menu = Menu::with_items(app, &[&show, &separator, &quit])?;
            TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .tooltip("Resoná")
                .on_menu_event(|app, event| {
                    match event.id.0.as_str() {
                        "show" => {
                            if let Some(window) = app.get_webview_window("main") {
                                let _ = window.show();
                                let _ = window.set_focus();
                            }
                        }
                        "quit" => {
                            app.exit(0);
                        }
                        _ => {}
                    }
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        if let Some(window) = tray.app_handle().get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                })
                .build(app)?;

            // Start background artist artwork worker
            let worker_state = app.state::<AppState>();
            artist_worker.start(worker_state.db.clone(), artist_cache, app.handle().clone());

            // Start file watchers for all library folders
            let state = app.state::<AppState>();
            let folders = tauri::async_runtime::block_on(
                resona_lib::library::db::get_library_folders(&state.db)
            ).unwrap_or_default();
            for folder_path in &folders {
                let path = PathBuf::from(folder_path);
                if path.exists() {
                    if let Ok(watcher) = scanner::start_watcher(state.db.clone(), path) {
                        // Leak the watcher so it lives for the app's lifetime
                        Box::leak(Box::new(watcher));
                    }
                }
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_tracks,
            commands::get_track,
            commands::search_tracks,
            commands::get_all_genres,
            commands::get_tracks_by_genre_id,
            commands::scan_folder,
            commands::get_artwork,
            commands::seek,
            commands::play_track,
            commands::toggle_play,
            commands::set_volume,
            commands::set_eq_band,
            commands::set_eq_enabled,
            commands::get_position,
            commands::get_playlists,
            commands::create_playlist,
            commands::delete_playlist,
            commands::rename_playlist,
            commands::export_playlist_m3u,
            commands::import_playlist_m3u,
            commands::get_playlist_track_count,
            commands::reorder_playlist_tracks,
            commands::add_to_playlist,
            commands::add_tracks_to_playlist,
            commands::remove_from_playlist,
            commands::get_playlist_tracks,
            commands::get_favorite_ids,
            commands::toggle_favorite,
            commands::get_library_folders,
            commands::add_library_folder,
            commands::add_library_folders,
            commands::remove_library_folder,
            commands::delete_tracks,
            commands::get_lyrics,
            commands::record_play,
            commands::get_recently_played_tracks,
            commands::get_most_listened_tracks,
            commands::get_least_listened_tracks,
            commands::show_in_explorer,
            commands::search_all,
            commands::find_lyrics,
            commands::update_track_metadata,
            commands::get_playlists_for_track,
            commands::get_artist_artwork,
            commands::get_track_artwork,
            commands::get_track_palette,
            commands::set_artwork,
            commands::prevent_sleep,
            commands::allow_sleep,
            commands::now_playing,
            commands::scrobble,
            commands::discord_connect,
            commands::discord_disconnect,
            commands::update_discord_presence,
            commands::remote_start,
            commands::remote_stop,
            commands::remote_status,
            commands::get_radio_stations,
            commands::add_radio_station,
            commands::delete_radio_station,
            commands::update_radio_station,
            commands::play_radio,
            commands::ping_audio,
            commands::stop_radio,
            commands::radio_status,
            commands::get_eq_profiles,
            commands::get_active_eq_profile,
            commands::create_eq_profile,
            commands::rename_eq_profile,
            commands::delete_eq_profile,
            commands::apply_eq_profile,
            commands::update_eq_band,
            commands::save_lyrics,
            commands::get_track_lyrics,
            commands::batch_fetch_lyrics,
            commands::get_track_palette,
            commands::get_player_state,
        ])
        .run(tauri::generate_context!())
        .expect("Failed to run application");
}
