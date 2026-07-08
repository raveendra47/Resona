use crate::audio::player::Player;
use crate::audio::lufs::LufsCache;
use crate::discord_rpc::DiscordRPC;
use crate::library::artist_artwork::ArtistArtworkCache;
use crate::library::artist_artwork_worker::ArtistArtworkWorker;
use crate::library::track::Track;
use crate::library::track_artwork::TrackArtworkCache;
use crate::remote::RemoteControl;
use crate::search::tantivy_index::TantivyIndex;
#[cfg(windows)]
use crate::gpu::artwork_engine::GpuArtworkEngine;
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32};
use std::sync::{Arc, Mutex};

pub struct AppState {
    pub player: Arc<Player>,
    pub db: SqlitePool,
    pub search: Arc<Mutex<TantivyIndex>>, // Phase 2: Tantivy instead of HashMap
    pub playlist_queue: Arc<Mutex<Vec<Track>>>,
    pub volume: Arc<AtomicU32>,
    pub discord_rpc: DiscordRPC,
    pub remote: RemoteControl,
    pub artwork_cache: Arc<Mutex<HashMap<String, String>>>,
    pub artist_artwork_cache: Arc<ArtistArtworkCache>,
    pub artist_artwork_worker: Arc<ArtistArtworkWorker>,
    pub track_artwork_cache: Arc<TrackArtworkCache>,
    pub is_scanning: Arc<AtomicBool>,
    pub is_fetching: Arc<AtomicBool>,
    pub current_track_id: Arc<Mutex<Option<String>>>,
    pub lufs_cache: Option<Arc<Mutex<LufsCache>>>, // Phase 2: LUFS normalization
    #[cfg(windows)]
    pub gpu_engine: Option<Arc<GpuArtworkEngine>>, // Phase 3: GPU acceleration
}
