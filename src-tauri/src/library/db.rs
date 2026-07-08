use crate::library::track::Track;
use lofty::file::TaggedFileExt;
use serde::{Deserialize, Serialize};
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
use sqlx::Executor;
use std::path::Path;

pub async fn init_pool(path: &Path) -> Result<SqlitePool, String> {
    let pool = SqlitePoolOptions::new()
        .max_connections(15) // Increased from 5 for better parallelism
        .connect_with(
            sqlx::sqlite::SqliteConnectOptions::new()
                .filename(path)
                .create_if_missing(true),
        )
        .await
        .map_err(|e| format!("Failed to create pool: {e}"))?;
    
    // Apply performance PRAGMAs before migrations
    sqlx::query("PRAGMA journal_mode=WAL")
        .execute(&pool)
        .await
        .map_err(|e| format!("Failed to set WAL mode: {e}"))?;
    
    sqlx::query("PRAGMA synchronous=NORMAL")
        .execute(&pool)
        .await
        .map_err(|e| format!("Failed to set synchronous mode: {e}"))?;
    
    sqlx::query("PRAGMA cache_size=-64000") // 64MB cache
        .execute(&pool)
        .await
        .map_err(|e| format!("Failed to set cache size: {e}"))?;
    
    sqlx::query("PRAGMA temp_store=MEMORY")
        .execute(&pool)
        .await
        .map_err(|e| format!("Failed to set temp store: {e}"))?;
    
    sqlx::query("PRAGMA mmap_size=268435456") // 256MB memory-mapped I/O
        .execute(&pool)
        .await
        .map_err(|e| format!("Failed to set mmap size: {e}"))?;
    
    run_migrations(&pool).await?;

    let pool_clone = pool.clone();
    tokio::spawn(async move {
        let _ = repair_artwork_status(&pool_clone).await;
    });

    Ok(pool)
}

async fn run_migrations(pool: &SqlitePool) -> Result<(), String> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS tracks (
            id          TEXT PRIMARY KEY,
            file_path   TEXT NOT NULL UNIQUE,
            title       TEXT NOT NULL DEFAULT '',
            artist      TEXT NOT NULL DEFAULT '',
            album       TEXT NOT NULL DEFAULT '',
            album_artist TEXT NOT NULL DEFAULT '',
            composer    TEXT NOT NULL DEFAULT '',
            track_number INTEGER DEFAULT 0,
            disc_number  INTEGER DEFAULT 0,
            year        INTEGER DEFAULT 0,
            genre       TEXT NOT NULL DEFAULT '',
            duration    REAL NOT NULL DEFAULT 0,
            sample_rate INTEGER NOT NULL DEFAULT 0,
            bit_depth   INTEGER NOT NULL DEFAULT 0,
            file_size   INTEGER NOT NULL DEFAULT 0,
            file_format TEXT NOT NULL DEFAULT '',
            has_artwork INTEGER NOT NULL DEFAULT 0,
            replaygain_track_gain REAL NOT NULL DEFAULT 0.0,
            replaygain_album_gain REAL NOT NULL DEFAULT 0.0,
            cue_parent_id TEXT,
            cue_offset REAL NOT NULL DEFAULT 0.0,
            created_at  TEXT NOT NULL DEFAULT (datetime('now')),
            modified_at TEXT NOT NULL DEFAULT (datetime('now'))
        )",
    )
    .execute(pool)
    .await
    .map_err(|e| format!("Migration failed: {e}"))?;

    // Migration to add has_artwork to existing tables
    let _ = sqlx::query(
        "ALTER TABLE tracks ADD COLUMN has_artwork INTEGER NOT NULL DEFAULT 0",
    )
    .execute(pool)
    .await;

    // Migration to add composer to existing tables
    let _ = sqlx::query(
        "ALTER TABLE tracks ADD COLUMN composer TEXT NOT NULL DEFAULT ''",
    )
    .execute(pool)
    .await;

    // Migration to add ReplayGain columns
    let _ = sqlx::query(
        "ALTER TABLE tracks ADD COLUMN replaygain_track_gain REAL NOT NULL DEFAULT 0.0",
    )
    .execute(pool)
    .await;
    let _ = sqlx::query(
        "ALTER TABLE tracks ADD COLUMN replaygain_album_gain REAL NOT NULL DEFAULT 0.0",
    )
    .execute(pool)
    .await;

    // Migration to add CUE columns
    let _ = sqlx::query(
        "ALTER TABLE tracks ADD COLUMN cue_parent_id TEXT",
    )
    .execute(pool)
    .await;
    let _ = sqlx::query(
        "ALTER TABLE tracks ADD COLUMN cue_offset REAL NOT NULL DEFAULT 0.0",
    )
    .execute(pool)
    .await;

    // Migration to add artwork_key to tracks
    let _ = sqlx::query(
        "ALTER TABLE tracks ADD COLUMN artwork_key TEXT",
    )
    .execute(pool)
    .await;

    // Migration to add play_count to tracks
    let _ = sqlx::query(
        "ALTER TABLE tracks ADD COLUMN play_count INTEGER NOT NULL DEFAULT 0",
    )
    .execute(pool)
    .await;

    // Migration to add last_played_at to tracks
    let _ = sqlx::query(
        "ALTER TABLE tracks ADD COLUMN last_played_at TEXT",
    )
    .execute(pool)
    .await;

    // Back-fill play_count and last_played_at from existing play_history
    let _ = sqlx::query(
        "UPDATE tracks SET play_count = (
            SELECT COUNT(*) FROM play_history WHERE play_history.track_id = tracks.id
        ), last_played_at = (
            SELECT MAX(played_at) FROM play_history WHERE play_history.track_id = tracks.id
        )
        WHERE id IN (SELECT track_id FROM play_history)"
    )
    .execute(pool)
    .await;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS playlists (
            id          TEXT PRIMARY KEY,
            name        TEXT NOT NULL,
            description TEXT NOT NULL DEFAULT '',
            created_at  TEXT NOT NULL DEFAULT (datetime('now'))
        )",
    )
    .execute(pool)
    .await
    .map_err(|e| format!("Migration failed: {e}"))?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS playlist_tracks (
            playlist_id TEXT NOT NULL REFERENCES playlists(id) ON DELETE CASCADE,
            track_id    TEXT NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
            position    INTEGER NOT NULL,
            added_at    TEXT NOT NULL DEFAULT (datetime('now')),
            PRIMARY KEY (playlist_id, track_id)
        )",
    )
    .execute(pool)
    .await
    .map_err(|e| format!("Migration failed: {e}"))?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS favorites (
            track_id TEXT PRIMARY KEY REFERENCES tracks(id) ON DELETE CASCADE,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        )",
    )
    .execute(pool)
    .await
    .map_err(|e| format!("Migration failed: {e}"))?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS library_folders (
            path TEXT PRIMARY KEY,
            added_at TEXT NOT NULL DEFAULT (datetime('now'))
        )",
    )
    .execute(pool)
    .await
    .map_err(|e| format!("Migration failed: {e}"))?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS play_history (
            id       INTEGER PRIMARY KEY AUTOINCREMENT,
            track_id TEXT NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
            played_at TEXT NOT NULL DEFAULT (datetime('now'))
        )",
    )
    .execute(pool)
    .await
    .map_err(|e| format!("Migration failed: {e}"))?;
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_play_history_track_id ON play_history(track_id)",
    )
    .execute(pool)
    .await
    .map_err(|e| format!("Migration failed: {e}"))?;
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_play_history_played_at ON play_history(played_at)",
    )
    .execute(pool)
    .await
    .map_err(|e| format!("Migration failed: {e}"))?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS eq_profiles (
            id          TEXT PRIMARY KEY,
            name        TEXT NOT NULL,
            is_default  INTEGER NOT NULL DEFAULT 0,
            is_active   INTEGER NOT NULL DEFAULT 0,
            created_at  TEXT NOT NULL DEFAULT (datetime('now'))
        )",
    )
    .execute(pool)
    .await
    .map_err(|e| format!("Migration failed: {e}"))?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS eq_bands (
            profile_id  TEXT NOT NULL REFERENCES eq_profiles(id) ON DELETE CASCADE,
            band_index  INTEGER NOT NULL,
            gain        REAL NOT NULL DEFAULT 0.0,
            PRIMARY KEY (profile_id, band_index)
        )",
    )
    .execute(pool)
    .await
    .map_err(|e| format!("Migration failed: {e}"))?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS radio_stations (
            id          TEXT PRIMARY KEY,
            name        TEXT NOT NULL,
            url         TEXT NOT NULL,
            genre       TEXT NOT NULL DEFAULT '',
            logo_url    TEXT NOT NULL DEFAULT '',
            country     TEXT NOT NULL DEFAULT '',
            language    TEXT NOT NULL DEFAULT '',
            bitrate     INTEGER NOT NULL DEFAULT 0,
            created_at  TEXT NOT NULL DEFAULT (datetime('now'))
        )",
    )
    .execute(pool)
    .await
    .map_err(|e| format!("Migration failed: {e}"))?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS lyrics (
            track_id    TEXT PRIMARY KEY REFERENCES tracks(id) ON DELETE CASCADE,
            content     TEXT NOT NULL DEFAULT '',
            source      TEXT NOT NULL DEFAULT '',
            meta_content TEXT NOT NULL DEFAULT '',
            meta_source TEXT NOT NULL DEFAULT '',
            updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
        )",
    )
    .execute(pool)
    .await
    .map_err(|e| format!("Migration failed: {e}"))?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS artists (
            name        TEXT PRIMARY KEY,
            artwork_key TEXT

        )",
    )
    .execute(pool)
    .await
    .map_err(|e| format!("Migration failed: {e}"))?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS genres (
            id                TEXT PRIMARY KEY,
            name              TEXT NOT NULL UNIQUE,
            normalization_key TEXT NOT NULL
        )",
    )
    .execute(pool)
    .await
    .map_err(|e| format!("Migration failed: {e}"))?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_genres_normalization_key ON genres(normalization_key)",
    )
    .execute(pool)
    .await
    .map_err(|e| format!("Migration failed: {e}"))?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS track_genres (
            track_id TEXT NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
            genre_id TEXT NOT NULL REFERENCES genres(id) ON DELETE CASCADE,
            position INTEGER NOT NULL,
            PRIMARY KEY (track_id, genre_id)
        )",
    )
    .execute(pool)
    .await
    .map_err(|e| format!("Migration failed: {e}"))?;

    // Migration: add raw_genre_names to tracks
    let _ = sqlx::query(
        "ALTER TABLE tracks ADD COLUMN raw_genre_names TEXT NOT NULL DEFAULT ''",
    )
    .execute(pool)
    .await;

    Ok(())
}

pub async fn get_library_folders(pool: &SqlitePool) -> Result<Vec<String>, String> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT path FROM library_folders ORDER BY added_at DESC"
    )
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Failed to fetch library folders: {e}"))?;
    Ok(rows.into_iter().map(|r| r.0).collect())
}

pub async fn add_library_folder(pool: &SqlitePool, path: &str) -> Result<(), String> {
    sqlx::query("INSERT OR IGNORE INTO library_folders (path) VALUES (?)")
        .bind(path)
        .execute(pool)
        .await
        .map_err(|e| format!("Failed to add library folder: {e}"))?;
    Ok(())
}

pub async fn remove_library_folder(pool: &SqlitePool, path: &str) -> Result<(), String> {
    let normalized = path.trim_end_matches(&['/', '\\'][..]);
    let sep = std::path::MAIN_SEPARATOR;
    let with_sep = format!("{}{}", normalized, sep);

    // Match tracks whose file_path starts with the folder path + separator,
    // or is exactly the folder path itself.
    // Use ^ as the LIKE escape character to avoid conflicts with path backslashes.
    let escaped = with_sep.replace('%', "^%").replace('_', "^_");
    let like_pattern = format!("{}%", escaped);

    let ids: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM tracks WHERE file_path = ? OR file_path LIKE ? ESCAPE '^'"
    )
    .bind(normalized)
    .bind(&like_pattern)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Failed to fetch tracks for removal: {e}"))?;

    if !ids.is_empty() {
        // Also clean up any CUE child tracks whose parent is among the removed tracks
        delete_tracks(pool, &ids).await?;
    }

    sqlx::query("DELETE FROM library_folders WHERE path = ?")
        .bind(path)
        .execute(pool)
        .await
        .map_err(|e| format!("Failed to remove library folder: {e}"))?;
    Ok(())
}

pub async fn get_favorite_ids(pool: &SqlitePool) -> Result<Vec<String>, String> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT track_id FROM favorites ORDER BY created_at DESC"
    )
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Failed to fetch favorites: {e}"))?;
    Ok(rows.into_iter().map(|r| r.0).collect())
}

pub async fn toggle_favorite(pool: &SqlitePool, id: &str) -> Result<bool, String> {
    let exists: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM favorites WHERE track_id = ?"
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .map_err(|e| format!("Failed to check favorite: {e}"))?;
    if exists.0 > 0 {
        sqlx::query("DELETE FROM favorites WHERE track_id = ?")
            .bind(id)
            .execute(pool)
            .await
            .map_err(|e| format!("Failed to remove favorite: {e}"))?;
        Ok(false)
    } else {
        sqlx::query("INSERT INTO favorites (track_id) VALUES (?)")
            .bind(id)
            .execute(pool)
            .await
            .map_err(|e| format!("Failed to add favorite: {e}"))?;
        Ok(true)
    }
}

pub async fn upsert_track<'e>(executor: impl Executor<'e, Database = sqlx::Sqlite>, track: &Track) -> Result<(), String> {
    sqlx::query(
         "INSERT INTO tracks (id, file_path, title, artist, album, album_artist,
          composer, track_number, disc_number, year, genre, raw_genre_names, duration, sample_rate, bit_depth,
          file_size, file_format, has_artwork, replaygain_track_gain, replaygain_album_gain,
          cue_parent_id, cue_offset, artwork_key, created_at, modified_at)
          VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?,
          datetime('now'), datetime('now'))
          ON CONFLICT(file_path) DO UPDATE SET
          title=excluded.title, artist=excluded.artist, album=excluded.album,
          album_artist=excluded.album_artist, composer=excluded.composer,
          track_number=excluded.track_number, disc_number=excluded.disc_number,
          year=excluded.year, genre=excluded.genre, raw_genre_names=excluded.raw_genre_names,
          duration=excluded.duration,
          sample_rate=excluded.sample_rate, bit_depth=excluded.bit_depth,
          file_size=excluded.file_size, file_format=excluded.file_format,
          has_artwork=excluded.has_artwork, modified_at=datetime('now'),
          replaygain_track_gain=excluded.replaygain_track_gain,
          replaygain_album_gain=excluded.replaygain_album_gain,
          cue_parent_id=excluded.cue_parent_id,
          cue_offset=excluded.cue_offset,
          artwork_key=excluded.artwork_key",
    )
    .bind(&track.id)
    .bind(&track.file_path)
    .bind(&track.title)
    .bind(&track.artist)
    .bind(&track.album)
    .bind(&track.album_artist)
    .bind(&track.composer)
    .bind(track.track_number as i64)
    .bind(track.disc_number as i64)
    .bind(track.year as i64)
    .bind(&track.genre)
    .bind(&track.raw_genre_names)
    .bind(track.duration)
    .bind(track.sample_rate as i64)
    .bind(track.bit_depth as i64)
    .bind(track.file_size as i64)
    .bind(&track.file_format)
    .bind(if track.has_artwork { 1 } else { 0 })
    .bind(track.replaygain_track_gain)
    .bind(track.replaygain_album_gain)
    .bind(track.cue_parent_id.as_deref())
    .bind(track.cue_offset)
    .bind(track.artwork_key.as_deref())
    .execute(executor)
    .await
    .map_err(|e| format!("Failed to upsert track: {e}"))?;
    Ok(())
}

pub async fn delete_track(pool: &SqlitePool, id: &str) -> Result<(), String> {
    delete_tracks(pool, &[id.to_string()]).await?;
    Ok(())
}

pub async fn delete_tracks(
    pool: &SqlitePool,
    ids: &[String],
) -> Result<Vec<String>, String> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }

    // Collect file paths before deletion
    let mut file_paths = Vec::new();
    let mut all_ids = ids.to_vec();

    // Find and include CUE child tracks whose parent is among the deleted tracks
    let cue_child_ids: Vec<String> = {
        let params: Vec<String> = ids.iter().enumerate().map(|(i, _)| format!("?{}", i + 1)).collect();
        let placeholders = params.join(",");
        let sql = format!("SELECT id FROM tracks WHERE cue_parent_id IN ({})", placeholders);
        let mut q = sqlx::query_scalar::<_, String>(&sql);
        for id in ids {
            q = q.bind(id);
        }
        q.fetch_all(pool).await.unwrap_or_default()
    };
    all_ids.extend(cue_child_ids);
    all_ids.dedup();

    for id in &all_ids {
        if let Ok(Some(track)) = get_track(pool, id).await {
            file_paths.push(track.file_path);
        }
    }

    // Build parameterized IN clause
    let params: Vec<String> = all_ids.iter().enumerate().map(|(i, _)| format!("?{}", i + 1)).collect();
    let placeholders = params.join(",");

    let play_history_sql = format!("DELETE FROM play_history WHERE track_id IN ({})", placeholders);
    let favorites_sql = format!("DELETE FROM favorites WHERE track_id IN ({})", placeholders);
    let playlist_tracks_sql = format!("DELETE FROM playlist_tracks WHERE track_id IN ({})", placeholders);
    let tracks_sql = format!("DELETE FROM tracks WHERE id IN ({})", placeholders);

    let mut query = sqlx::query(&play_history_sql);
    for id in &all_ids {
        query = query.bind(id);
    }
    query.execute(pool).await.map_err(|e| format!("Failed to delete play history: {e}"))?;

    let mut query = sqlx::query(&favorites_sql);
    for id in &all_ids {
        query = query.bind(id);
    }
    query.execute(pool).await.map_err(|e| format!("Failed to delete favorites: {e}"))?;

    let mut query = sqlx::query(&playlist_tracks_sql);
    for id in &all_ids {
        query = query.bind(id);
    }
    query.execute(pool).await.map_err(|e| format!("Failed to delete playlist_tracks: {e}"))?;

    let mut query = sqlx::query(&tracks_sql);
    for id in &all_ids {
        query = query.bind(id);
    }
    query.execute(pool).await.map_err(|e| format!("Failed to delete tracks: {e}"))?;

    Ok(file_paths)
}

pub async fn get_all_tracks(pool: &SqlitePool) -> Result<Vec<Track>, String> {
    let rows = sqlx::query_as::<_, TrackRow>(
         "SELECT id, file_path, title, artist, album, album_artist, composer,
           track_number, disc_number, year, genre, raw_genre_names, duration, sample_rate,
           bit_depth, file_size, file_format, has_artwork, replaygain_track_gain, replaygain_album_gain, cue_parent_id, cue_offset, artwork_key FROM tracks ORDER BY title"
    )
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Failed to fetch tracks: {e}"))?;
    Ok(rows.into_iter().map(|r| r.into()).collect())
}

pub async fn get_track(pool: &SqlitePool, id: &str) -> Result<Option<Track>, String> {
    let row = sqlx::query_as::<_, TrackRow>(
        "SELECT id, file_path, title, artist, album, album_artist, composer,
          track_number, disc_number, year, genre, raw_genre_names, duration, sample_rate,
          bit_depth, file_size, file_format, has_artwork, replaygain_track_gain, replaygain_album_gain, cue_parent_id, cue_offset, artwork_key FROM tracks WHERE id = ?"
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("Failed to fetch track: {e}"))?;
    Ok(row.map(|r| r.into()))
}

pub async fn record_play(pool: &SqlitePool, track_id: &str) -> Result<(), String> {
    sqlx::query("INSERT INTO play_history (track_id) VALUES (?)")
        .bind(track_id)
        .execute(pool)
        .await
        .map_err(|e| format!("Failed to record play: {e}"))?;
    sqlx::query("UPDATE tracks SET play_count = play_count + 1, last_played_at = datetime('now') WHERE id = ?")
        .bind(track_id)
        .execute(pool)
        .await
        .map_err(|e| format!("Failed to update play count: {e}"))?;
    Ok(())
}

pub async fn get_recently_played_tracks(
    pool: &SqlitePool,
    limit: i64,
) -> Result<Vec<Track>, String> {
    let rows = sqlx::query_as::<_, TrackRow>(
        "SELECT id, file_path, title, artist, album, album_artist, composer,
          track_number, disc_number, year, genre, raw_genre_names, duration, sample_rate,
          bit_depth, file_size, file_format, has_artwork, replaygain_track_gain, replaygain_album_gain,
          cue_parent_id, cue_offset, artwork_key FROM tracks
          WHERE play_count > 0
          ORDER BY last_played_at DESC
          LIMIT ?",
    )
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Failed to fetch recently played: {e}"))?;
    Ok(rows.into_iter().map(|r| r.into()).collect())
}

pub async fn get_most_listened_tracks(
    pool: &SqlitePool,
    limit: i64,
) -> Result<Vec<Track>, String> {
    let rows = sqlx::query_as::<_, TrackRow>(
        "SELECT id, file_path, title, artist, album, album_artist, composer,
          track_number, disc_number, year, genre, raw_genre_names, duration, sample_rate,
          bit_depth, file_size, file_format, has_artwork, replaygain_track_gain, replaygain_album_gain,
          cue_parent_id, cue_offset, artwork_key FROM tracks
          WHERE play_count > 0
          ORDER BY play_count DESC
          LIMIT ?",
    )
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Failed to fetch most listened: {e}"))?;
    Ok(rows.into_iter().map(|r| r.into()).collect())
}

pub async fn get_least_listened_tracks(
    pool: &SqlitePool,
    limit: i64,
) -> Result<Vec<Track>, String> {
    let rows = sqlx::query_as::<_, TrackRow>(
        "SELECT id, file_path, title, artist, album, album_artist, composer,
          track_number, disc_number, year, genre, raw_genre_names, duration, sample_rate,
          bit_depth, file_size, file_format, has_artwork, replaygain_track_gain, replaygain_album_gain,
          cue_parent_id, cue_offset, artwork_key FROM tracks
          ORDER BY play_count ASC, COALESCE(last_played_at, '1970-01-01') ASC
          LIMIT ?",
    )
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Failed to fetch least listened: {e}"))?;
    Ok(rows.into_iter().map(|r| r.into()).collect())
}

pub async fn get_track_by_path(pool: &SqlitePool, path: &str) -> Result<Option<Track>, String> {
    let row = sqlx::query_as::<_, TrackRow>(
        "SELECT id, file_path, title, artist, album, album_artist, composer,
          track_number, disc_number, year, genre, raw_genre_names, duration, sample_rate,
          bit_depth, file_size, file_format, has_artwork, replaygain_track_gain, replaygain_album_gain, cue_parent_id, cue_offset, artwork_key FROM tracks WHERE file_path = ?"
    )
    .bind(path)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("Failed to fetch track by path: {e}"))?;
    Ok(row.map(|r| r.into()))
}

#[derive(sqlx::FromRow)]
pub(crate) struct TrackRow {
    id: String,
    file_path: String,
    title: String,
    artist: String,
    album: String,
    album_artist: String,
    composer: String,
    track_number: i32,
    disc_number: i32,
    year: i32,
    genre: String,
    raw_genre_names: String,
    duration: f64,
    sample_rate: i32,
    bit_depth: i32,
    file_size: i64,
    file_format: String,
    has_artwork: bool,
    replaygain_track_gain: f64,
    replaygain_album_gain: f64,
    cue_parent_id: Option<String>,
    cue_offset: f64,
    artwork_key: Option<String>,
}

impl From<TrackRow> for Track {
    fn from(r: TrackRow) -> Self {
        Track {
            id: r.id,
            file_path: r.file_path,
            title: r.title,
            artist: r.artist,
            album: r.album,
            album_artist: r.album_artist,
            composer: r.composer,
            track_number: r.track_number as u32,
            disc_number: r.disc_number as u32,
            year: r.year,
            genre: r.genre,
            raw_genre_names: r.raw_genre_names,
            duration: r.duration,
            sample_rate: r.sample_rate as u32,
            bit_depth: r.bit_depth as u32,
            file_size: r.file_size as u64,
            file_format: r.file_format,
            has_artwork: r.has_artwork,
            replaygain_track_gain: r.replaygain_track_gain,
            replaygain_album_gain: r.replaygain_album_gain,
            cue_parent_id: r.cue_parent_id,
            cue_offset: r.cue_offset,
            artwork_key: r.artwork_key,
        }
    }
}

pub async fn get_radio_stations(pool: &SqlitePool) -> Result<Vec<crate::radio::RadioStation>, String> {
    #[derive(sqlx::FromRow)]
    struct RadioRow {
        id: String,
        name: String,
        url: String,
        genre: String,
        logo_url: String,
        country: String,
        language: String,
        bitrate: i64,
    }
    let rows: Vec<RadioRow> = sqlx::query_as(
        "SELECT id, name, url, genre, logo_url, country, language, bitrate FROM radio_stations ORDER BY name"
    )
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Failed to fetch radio stations: {e}"))?;
    Ok(rows.into_iter().map(|r| crate::radio::RadioStation {
        id: r.id,
        name: r.name,
        url: r.url,
        genre: r.genre,
        logo_url: r.logo_url,
        country: r.country,
        language: r.language,
        bitrate: r.bitrate,
    }).collect())
}

pub async fn add_radio_station(pool: &SqlitePool, station: &crate::radio::RadioStation) -> Result<(), String> {
    sqlx::query(
        "INSERT INTO radio_stations (id, name, url, genre, logo_url, country, language, bitrate) VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(&station.id)
    .bind(&station.name)
    .bind(&station.url)
    .bind(&station.genre)
    .bind(&station.logo_url)
    .bind(&station.country)
    .bind(&station.language)
    .bind(station.bitrate)
    .execute(pool)
    .await
    .map_err(|e| format!("Failed to add radio station: {e}"))?;
    Ok(())
}

pub async fn delete_radio_station(pool: &SqlitePool, id: &str) -> Result<(), String> {
    sqlx::query("DELETE FROM radio_stations WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await
        .map_err(|e| format!("Failed to delete radio station: {e}"))?;
    Ok(())
}

pub async fn update_radio_station(pool: &SqlitePool, station: &crate::radio::RadioStation) -> Result<(), String> {
    sqlx::query(
        "UPDATE radio_stations SET name=?, url=?, genre=?, logo_url=?, country=?, language=?, bitrate=? WHERE id=?"
    )
    .bind(&station.name)
    .bind(&station.url)
    .bind(&station.genre)
    .bind(&station.logo_url)
    .bind(&station.country)
    .bind(&station.language)
    .bind(station.bitrate)
    .bind(&station.id)
    .execute(pool)
    .await
    .map_err(|e| format!("Failed to update radio station: {e}"))?;
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LyricRow {
    pub track_id: String,
    pub content: String,
    pub source: String,
    pub meta_content: String,
    pub meta_source: String,
}

pub async fn get_lyrics(pool: &SqlitePool, track_id: &str) -> Result<Option<LyricRow>, String> {
    let row = sqlx::query_as::<_, (String, String, String, String, String)>(
        "SELECT track_id, content, source, meta_content, meta_source FROM lyrics WHERE track_id = ?"
    )
    .bind(track_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("Failed to fetch lyrics: {e}"))?;
    Ok(row.map(|(track_id, content, source, meta_content, meta_source)| LyricRow {
        track_id, content, source, meta_content, meta_source,
    }))
}

pub async fn upsert_lyrics<'e>(executor: impl Executor<'e, Database = sqlx::Sqlite>, track_id: &str, content: &str, source: &str, meta_content: &str, meta_source: &str) -> Result<(), String>
{
    sqlx::query(
        "INSERT INTO lyrics (track_id, content, source, meta_content, meta_source, updated_at)
         VALUES (?, ?, ?, ?, ?, datetime('now'))
         ON CONFLICT(track_id) DO UPDATE SET
         content=excluded.content, source=excluded.source,
         meta_content=excluded.meta_content, meta_source=excluded.meta_source,
         updated_at=datetime('now')"
    )
    .bind(track_id)
    .bind(content)
    .bind(source)
    .bind(meta_content)
    .bind(meta_source)
    .execute(executor)
    .await
    .map_err(|e| format!("Failed to upsert lyrics: {e}"))?;
    Ok(())
}

pub async fn delete_lyrics(pool: &SqlitePool, track_id: &str) -> Result<(), String> {
    sqlx::query("DELETE FROM lyrics WHERE track_id = ?")
        .bind(track_id)
        .execute(pool)
        .await
        .map_err(|e| format!("Failed to delete lyrics: {e}"))?;
    Ok(())
}

pub async fn get_artist_artwork_key(pool: &SqlitePool, name: &str) -> Result<Option<String>, String> {
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT artwork_key FROM artists WHERE name = ? AND artwork_key IS NOT NULL AND artwork_key != ''"
    )
    .bind(name)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("Failed to fetch artist artwork key: {e}"))?;
    Ok(row.map(|r| r.0))
}

pub async fn set_artist_artwork_key(pool: &SqlitePool, name: &str, key: &str) -> Result<(), String> {
    sqlx::query(
        "INSERT INTO artists (name, artwork_key) VALUES (?, ?) ON CONFLICT(name) DO UPDATE SET artwork_key=excluded.artwork_key"
    )
    .bind(name)
    .bind(key)
    .execute(pool)
    .await
    .map_err(|e| format!("Failed to set artist artwork key: {e}"))?;
    Ok(())
}

pub async fn get_all_genres(pool: &SqlitePool) -> Result<Vec<crate::library::track::Genre>, String> {
    let rows = sqlx::query_as::<_, (String, String, String, i64)>(
        "SELECT g.id, g.name, g.normalization_key, COUNT(tg.track_id) as track_count
         FROM genres g
         LEFT JOIN track_genres tg ON tg.genre_id = g.id
         GROUP BY g.id
         ORDER BY g.name"
    )
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Failed to fetch genres: {e}"))?;
    Ok(rows.into_iter().map(|(id, name, normalization_key, track_count)| {
        crate::library::track::Genre { id, name, normalization_key, track_count }
    }).collect())
}

pub async fn get_tracks_by_genre_id(pool: &SqlitePool, genre_id: &str) -> Result<Vec<Track>, String> {
    let rows = sqlx::query_as::<_, TrackRow>(
        "SELECT t.id, t.file_path, t.title, t.artist, t.album, t.album_artist, t.composer,
         t.track_number, t.disc_number, t.year, t.genre, t.raw_genre_names, t.duration, t.sample_rate,
         t.bit_depth, t.file_size, t.file_format, t.has_artwork, t.replaygain_track_gain,
         t.replaygain_album_gain, t.cue_parent_id, t.cue_offset, t.artwork_key
         FROM tracks t
         JOIN track_genres tg ON tg.track_id = t.id
         WHERE tg.genre_id = ?
         GROUP BY t.id
         ORDER BY t.year DESC, t.album, t.disc_number, t.track_number"
    )
    .bind(genre_id)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Failed to fetch tracks by genre: {e}"))?;
    Ok(rows.into_iter().map(|r| r.into()).collect())
}

pub async fn upsert_genre(conn: &mut sqlx::SqliteConnection, genre: &crate::library::track::Genre) -> Result<(), String> {
    sqlx::query(
        "INSERT INTO genres (id, name, normalization_key) VALUES (?, ?, ?)
         ON CONFLICT(id) DO UPDATE SET normalization_key = excluded.normalization_key"
    )
    .bind(&genre.id)
    .bind(&genre.name)
    .bind(&genre.normalization_key)
    .execute(&mut *conn)
    .await
    .map_err(|e| format!("Failed to upsert genre: {e}"))?;
    Ok(())
}

pub async fn set_track_genres(conn: &mut sqlx::SqliteConnection, track_id: &str, genre_ids: &[String]) -> Result<(), String> {
    sqlx::query("DELETE FROM track_genres WHERE track_id = ?")
        .bind(track_id)
        .execute(&mut *conn)
        .await
        .map_err(|e| format!("Failed to delete track genres: {e}"))?;
    for (i, gid) in genre_ids.iter().enumerate() {
        sqlx::query("INSERT INTO track_genres (track_id, genre_id, position) VALUES (?, ?, ?)")
            .bind(track_id)
            .bind(gid)
            .bind(i as i64)
            .execute(&mut *conn)
            .await
            .map_err(|e| format!("Failed to insert track genre: {e}"))?;
    }
    Ok(())
}

pub async fn repair_artwork_status(pool: &SqlitePool) -> Result<(), String> {
    let rows = sqlx::query_as::<_, TrackRow>(
        "SELECT id, file_path, title, artist, album, album_artist, composer,
          track_number, disc_number, year, genre, raw_genre_names, duration, sample_rate,
          bit_depth, file_size, file_format, has_artwork, replaygain_track_gain, replaygain_album_gain, cue_parent_id, cue_offset, artwork_key FROM tracks WHERE has_artwork = 0"
    )
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;

    for r in rows {
        let path = Path::new(&r.file_path);
        if path.exists() {
            if let Ok(tagged_file) = lofty::read_from_path(path) {
                let has_art = tagged_file
                    .tags()
                    .iter()
                    .any(|tag| !tag.pictures().is_empty());
                if has_art {
                    let _ = sqlx::query("UPDATE tracks SET has_artwork = 1 WHERE id = ?")
                        .bind(&r.id)
                        .execute(pool)
                        .await;
                }
            }
        }
    }

    // Back-fill artwork_key from other tracks in same album (handles pre-migration data)
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
    .execute(pool)
    .await;

    // Back-fill raw_genre_names from the old genre column for pre-migration tracks
    let _ = sqlx::query(
        "UPDATE tracks SET raw_genre_names = genre WHERE raw_genre_names = '' AND genre != ''"
    )
    .execute(pool)
    .await;

    // Back-fill track_genres for tracks that have raw_genre_names but no genre links
    let unlinked: Vec<(String, String)> = sqlx::query_as(
        "SELECT t.id, t.raw_genre_names FROM tracks t
         WHERE t.raw_genre_names != ''
         AND NOT EXISTS (SELECT 1 FROM track_genres tg WHERE tg.track_id = t.id)"
    )
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    for (track_id, raw) in &unlinked {
        let names = crate::library::track::split_genre_string(raw);
        let mut gids: Vec<String> = Vec::new();
        for name in &names {
            let nk = crate::library::track::normalize_genre_name(name);
            let gid = format!("genre-{}", nk);
            let genre = crate::library::track::Genre {
                id: gid.clone(),
                name: name.clone(),
                normalization_key: nk,
                track_count: 0,
            };
            let _ = upsert_genre(&mut *pool.acquire().await.unwrap_or_else(|_| panic!("DB error")), &genre).await;
            gids.push(gid);
        }
        let _ = set_track_genres(&mut *pool.acquire().await.unwrap_or_else(|_| panic!("DB error")), track_id, &gids).await;
    }

    // Performance indexes for faster queries (Phase 1 optimization)
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_tracks_album ON tracks(album)"
    )
    .execute(pool)
    .await
    .map_err(|e| format!("Migration failed: {e}"))?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_tracks_artist ON tracks(artist)"
    )
    .execute(pool)
    .await
    .map_err(|e| format!("Migration failed: {e}"))?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_tracks_album_artist ON tracks(album_artist)"
    )
    .execute(pool)
    .await
    .map_err(|e| format!("Migration failed: {e}"))?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_tracks_genre ON tracks(genre)"
    )
    .execute(pool)
    .await
    .map_err(|e| format!("Migration failed: {e}"))?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_tracks_year ON tracks(year)"
    )
    .execute(pool)
    .await
    .map_err(|e| format!("Migration failed: {e}"))?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_tracks_duration ON tracks(duration)"
    )
    .execute(pool)
    .await
    .map_err(|e| format!("Migration failed: {e}"))?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_tracks_play_count ON tracks(play_count DESC)"
    )
    .execute(pool)
    .await
    .map_err(|e| format!("Migration failed: {e}"))?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_tracks_artwork_key ON tracks(artwork_key) WHERE artwork_key IS NOT NULL"
    )
    .execute(pool)
    .await
    .map_err(|e| format!("Migration failed: {e}"))?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_playlist_tracks_position ON playlist_tracks(playlist_id, position)"
    )
    .execute(pool)
    .await
    .map_err(|e| format!("Migration failed: {e}"))?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_tracks_file_path ON tracks(file_path)"
    )
    .execute(pool)
    .await
    .map_err(|e| format!("Migration failed: {e}"))?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_tracks_cue_parent ON tracks(cue_parent_id) WHERE cue_parent_id IS NOT NULL"
    )
    .execute(pool)
    .await
    .map_err(|e| format!("Migration failed: {e}"))?;

    Ok(())
}
