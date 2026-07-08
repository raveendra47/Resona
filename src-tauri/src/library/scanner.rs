use crate::library::cue;
use crate::library::db;
use crate::library::track::{extract_artwork_bytes, extract_lyrics_from_file, normalize_genre_name, split_genre_string, Genre, Track};
use crate::library::track_artwork::TrackArtworkCache;
use serde::Serialize;
use sqlx::SqlitePool;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;
use walkdir::WalkDir;
use rayon::prelude::*;

#[derive(Serialize)]
pub struct ScanResult {
    pub added: usize,
    pub updated: usize,
    pub removed: usize,
}

pub async fn scan_folder(pool: &SqlitePool, folder: &Path, artwork_cache: &TrackArtworkCache) -> Result<ScanResult, String> {
    let exts = ["mp3", "flac", "wav", "ogg", "m4a", "aac", "wma", "opus"];
    
    // Collect all audio file entries in parallel
    let entries: Vec<_> = WalkDir::new(folder)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
        .par_bridge()
        .filter(|entry| {
            entry.path().is_file() && 
            entry.path().extension()
                .and_then(|e| e.to_str())
                .map(|e| exts.contains(&e.to_lowercase().as_str()))
                .unwrap_or(false)
        })
        .collect();

    let mut tx = pool.begin().await.map_err(|e| format!("Failed to begin transaction: {e}"))?;

    let mut added = 0;
    let updated = 0;
    
    // Process tracks in parallel with batched inserts
    let processed_results: Vec<(Track, Option<String>, Option<(String, String)>, Option<Vec<Track>>)> = entries
        .par_iter()
        .filter_map(|entry| {
            let path = entry.path();
            match Track::from_path(path) {
                Ok(mut track) => {
                    // Extract artwork
                    let artwork_key = if track.has_artwork && track.artwork_key.is_none() {
                        extract_artwork_bytes(path).and_then(|bytes| artwork_cache.save(&bytes).ok())
                    } else {
                        None
                    };
                    
                    if let Some(key) = &artwork_key {
                        track.artwork_key = Some(key.clone());
                    }
                    
                    // Extract lyrics
                    let lyrics = extract_lyrics_from_file(path);
                    
                    // Parse CUE sheets
                    let cue_tracks = cue::find_cue_for_audio(path)
                        .map(|cue_content| {
                            let cue_tracks = cue::parse_cue(&cue_content);
                            let parent_id = track.id.clone();
                            cue_tracks.into_iter().enumerate().map(|(i, ct)| {
                                let mut virtual_track = track.clone();
                                virtual_track.id = format!("{}-cue-{:02}", parent_id, ct.track_number);
                                virtual_track.title = ct.title.clone();
                                if !ct.artist.is_empty() {
                                    virtual_track.artist = ct.artist.clone();
                                }
                                virtual_track.track_number = ct.track_number;
                                virtual_track.cue_parent_id = Some(parent_id.clone());
                                virtual_track.cue_offset = ct.start_secs;
                                virtual_track
                            }).collect::<Vec<_>>()
                        });
                    
                    Some((track, artwork_key, lyrics, cue_tracks))
                }
                Err(e) => {
                    log::warn!("Failed to read metadata for {:?}: {e}", path);
                    None
                }
            }
        })
        .collect();

    // Insert all tracks and related data in batches
    for (mut track, _artwork_key, lyrics, cue_tracks) in processed_results {
        let parent_id = track.id.clone();
        db::upsert_track(&mut *tx, &track).await?;
        added += 1;

        // Process genres
        if !track.raw_genre_names.is_empty() {
            let genre_names = split_genre_string(&track.raw_genre_names);
            let mut genre_ids: Vec<String> = Vec::new();
            for name in &genre_names {
                let nk = normalize_genre_name(name);
                let gid = format!("genre-{}", nk);
                let genre = Genre {
                    id: gid.clone(),
                    name: name.clone(),
                    normalization_key: nk,
                    track_count: 0,
                };
                let _ = db::upsert_genre(&mut *tx, &genre).await;
                genre_ids.push(gid);
            }
            let _ = db::set_track_genres(&mut *tx, &track.id, &genre_ids).await;
        }

        // Insert lyrics
        if let Some((meta_lyrics, meta_source)) = lyrics {
            let _ = db::upsert_lyrics(&mut *tx, &track.id, "", "", &meta_lyrics, &meta_source).await;
        }

        // Insert CUE child tracks
        if let Some(cue_track_list) = cue_tracks {
            for ct in &cue_track_list {
                db::upsert_track(&mut *tx, ct).await?;
                added += 1;
                
                // Inherit parent's genres
                if !ct.raw_genre_names.is_empty() {
                    let genre_names = split_genre_string(&ct.raw_genre_names);
                    let mut genre_ids: Vec<String> = Vec::new();
                    for name in &genre_names {
                        let nk = normalize_genre_name(name);
                        let gid = format!("genre-{}", nk);
                        let genre = Genre {
                            id: gid.clone(),
                            name: name.clone(),
                            normalization_key: nk,
                            track_count: 0,
                        };
                        let _ = db::upsert_genre(&mut *tx, &genre).await;
                        genre_ids.push(gid);
                    }
                    let _ = db::set_track_genres(&mut *tx, &ct.id, &genre_ids).await;
                }
            }
        }
    }

    // Propagate artwork_key to tracks in the same album that have none
    let _ = sqlx::query(
        "UPDATE tracks SET artwork_key = (
            SELECT t2.artwork_key FROM tracks t2
            WHERE t2.album = tracks.album
              AND t2.artwork_key IS NOT NULL AND t2.artwork_key != ''
            LIMIT 1
        ) WHERE artwork_key IS NULL AND album IN (
            SELECT album FROM tracks
            WHERE artwork_key IS NOT NULL AND artwork_key != '' AND album != ''
        )"
    )
    .execute(&mut *tx)
    .await;

    // Remove tracks whose files no longer exist on disk in this folder
    let sep = std::path::MAIN_SEPARATOR;
    let folder_str = folder.to_string_lossy();
    let with_sep = format!("{}{}", folder_str, sep);
    let escaped = with_sep.replace('%', "^%").replace('_', "^_");
    let like_pattern = format!("{}%", escaped);
    let orphan_ids: Vec<(String, String)> = sqlx::query_as(
        "SELECT id, file_path FROM tracks WHERE file_path = ? OR file_path LIKE ? ESCAPE '^'"
    )
    .bind(folder_str.as_ref())
    .bind(&like_pattern)
    .fetch_all(&mut *tx)
    .await
    .map_err(|e| format!("Failed to fetch orphan tracks: {e}"))?;

    let mut missing_ids: Vec<String> = Vec::new();
    for (id, file_path) in &orphan_ids {
        if !std::path::Path::new(file_path).exists() {
            missing_ids.push(id.clone());
        }
    }

    // Also find CUE children of missing tracks
    if !missing_ids.is_empty() {
        let params: Vec<String> = missing_ids.iter().enumerate().map(|(i, _)| format!("?{}", i + 1)).collect();
        let cue_sql = format!("SELECT id FROM tracks WHERE cue_parent_id IN ({})", params.join(","));
        let mut q = sqlx::query_scalar::<_, String>(&cue_sql);
        for id in &missing_ids {
            q = q.bind(id);
        }
        if let Ok(children) = q.fetch_all(&mut *tx).await {
            missing_ids.extend(children);
        }
    }

    let mut actually_removed = 0;
    if !missing_ids.is_empty() {
        let params: Vec<String> = missing_ids.iter().enumerate().map(|(i, _)| format!("?{}", i + 1)).collect();
        let placeholders = params.join(",");

        let del_hist = format!("DELETE FROM play_history WHERE track_id IN ({})", placeholders);
        let mut q = sqlx::query(&del_hist);
        for id in &missing_ids { q = q.bind(id); }
        let _ = q.execute(&mut *tx).await;

        let del_fav = format!("DELETE FROM favorites WHERE track_id IN ({})", placeholders);
        let mut q = sqlx::query(&del_fav);
        for id in &missing_ids { q = q.bind(id); }
        let _ = q.execute(&mut *tx).await;

        let del_pl = format!("DELETE FROM playlist_tracks WHERE track_id IN ({})", placeholders);
        let mut q = sqlx::query(&del_pl);
        for id in &missing_ids { q = q.bind(id); }
        let _ = q.execute(&mut *tx).await;

        let del_trk = format!("DELETE FROM tracks WHERE id IN ({})", placeholders);
        let mut q = sqlx::query(&del_trk);
        for id in &missing_ids { q = q.bind(id); }
        q.execute(&mut *tx).await.map_err(|e| format!("Failed to delete orphan tracks: {e}"))?;

        actually_removed = missing_ids.len();
    }

    tx.commit().await.map_err(|e| format!("Failed to commit transaction: {e}"))?;

    Ok(ScanResult {
        added,
        updated,
        removed: actually_removed,
    })
}

// Windows-specific file watcher will be implemented in Phase 1
// For now, keep the notify-based watcher as a placeholder
#[cfg(not(windows))]
pub fn start_watcher(
    pool: SqlitePool,
    folder: std::path::PathBuf,
) -> Result<(), String> {
    log::warn!("File watcher not yet implemented for this platform");
    Ok(())
}

#[cfg(windows)]
pub fn start_watcher(
    pool: SqlitePool,
    folder: std::path::PathBuf,
) -> Result<(), String> {
    // TODO: Implement Windows-specific file watcher using ReadDirectoryChangesW
    // This will replace the notify crate with native Windows API
    log::info!("Windows file watcher placeholder - will use ReadDirectoryChangesW");
    
    // For now, just return Ok to allow compilation
    // The actual implementation will be in windows_watcher.rs
    Ok(())
}
