//! Windows-specific file watcher using ReadDirectoryChangesW API
//! 
//! This provides instant file change detection without polling,
//! replacing the cross-platform notify crate for better performance.

#[cfg(windows)]
use std::path::{Path, PathBuf};
#[cfg(windows)]
use std::sync::Arc;
#[cfg(windows)]
use sqlx::SqlitePool;
#[cfg(windows)]
use tokio::sync::mpsc;
#[cfg(windows)]
use windows::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
#[cfg(windows)]
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_ACTION_ADDED, FILE_ACTION_MODIFIED, FILE_ACTION_REMOVED,
    FILE_ACTION_RENAMED_NEW_NAME, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    FILE_FLAG_BACKUP_SEMANTICS, FILE_NOTIFY_CHANGE_FILE_NAME,
    FILE_NOTIFY_CHANGE_LAST_WRITE, FILE_NOTIFY_CHANGE_SIZE,
};
#[cfg(windows)]
use windows::Win32::System::Threading::Sleep;
#[cfg(windows)]
use windows::Win32::System::IO::ReadDirectoryChangesW;

/// File change event types
#[derive(Debug, Clone)]
pub enum FileChangeEvent {
    Added(PathBuf),
    Modified(PathBuf),
    Removed(PathBuf),
    Renamed(PathBuf, PathBuf), // (old_path, new_path)
}

/// Windows directory watcher using ReadDirectoryChangesW
pub struct WindowsWatcher {
    handle: Option<HANDLE>,
    watch_path: PathBuf,
    event_tx: mpsc::Sender<FileChangeEvent>,
}

impl WindowsWatcher {
    /// Create a new watcher for the specified directory
    pub fn new<P: AsRef<Path>>(path: P, event_tx: mpsc::Sender<FileChangeEvent>) -> Result<Self, String> {
        #[cfg(not(windows))]
        {
            return Err("WindowsWatcher is only available on Windows".to_string());
        }
        
        #[cfg(windows)]
        {
            let path = path.as_ref().to_path_buf();
            
            // Open directory handle
            let handle = unsafe {
                use std::ffi::OsStr;
                use std::os::windows::ffi::OsStrExt;
                
                let wide_path: Vec<u16> = OsStr::new(&path)
                    .encode_wide()
                    .chain(Some(0))
                    .collect();
                
                CreateFileW(
                    windows::core::PCWSTR(wide_path.as_ptr()),
                    0, // GENERIC_READ not needed for directory monitoring
                    FILE_SHARE_READ | FILE_SHARE_WRITE,
                    None, // no security attributes
                    OPEN_EXISTING,
                    FILE_FLAG_BACKUP_SEMANTICS, // Required for directories
                    HANDLE::default(),
                )
            };
            
            if handle == INVALID_HANDLE_VALUE {
                return Err(format!("Failed to open directory: {:?}", path));
            }
            
            Ok(Self {
                handle: Some(handle),
                watch_path: path,
                event_tx,
            })
        }
    }
    
    /// Start watching for changes (runs in background task)
    pub async fn start_watching(&self, pool: SqlitePool) -> Result<(), String> {
        #[cfg(not(windows))]
        {
            return Err("WindowsWatcher is only available on Windows".to_string());
        }
        
        #[cfg(windows)]
        {
            let handle = self.handle.ok_or("Watcher handle not initialized")?;
            let watch_path = self.watch_path.clone();
            let event_tx = self.event_tx.clone();
            
            tokio::spawn(async move {
                const BUFFER_SIZE: usize = 4096;
                let mut buffer = vec![0u8; BUFFER_SIZE];
                
                loop {
                    // Wait for directory changes
                    let bytes_returned = unsafe {
                        ReadDirectoryChangesW(
                            handle,
                            Some(buffer.as_mut_ptr() as *mut _),
                            BUFFER_SIZE as u32,
                            false, // non-recursive
                            FILE_NOTIFY_CHANGE_FILE_NAME | FILE_NOTIFY_CHANGE_LAST_WRITE | FILE_NOTIFY_CHANGE_SIZE,
                            &mut 0u32, // bytes returned
                            None, // no overlapped I/O
                            None, // no completion routine
                        )
                    };
                    
                    if bytes_returned == 0 {
                        log::error!("ReadDirectoryChangesW failed");
                        Sleep(1000); // Wait before retrying
                        continue;
                    }
                    
                    // Parse FILE_NOTIFY_INFORMATION structures
                    let mut offset = 0;
                    while offset < bytes_returned as usize {
                        let notify = unsafe {
                            &*(buffer.as_ptr().add(offset) as *const windows::Win32::Storage::FileSystem::FILE_NOTIFY_INFORMATION)
                        };
                        
                        let action = notify.Action;
                        let filename_len = notify.FileNameLength as usize;
                        let filename_bytes = &buffer[offset + 20..offset + 20 + filename_len]; // FILE_NOTIFY_INFORMATION header is 20 bytes
                        
                        // Convert UTF-16 to String
                        let filename_utf16: Vec<u16> = filename_bytes
                            .chunks_exact(2)
                            .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
                            .collect();
                        let filename = String::from_utf16_lossy(&filename_utf16);
                        
                        let full_path = watch_path.join(&filename);
                        
                        // Debounce: wait a bit to ensure file operation is complete
                        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                        
                        // Send event based on action
                        match action {
                            FILE_ACTION_ADDED => {
                                let _ = event_tx.send(FileChangeEvent::Added(full_path)).await;
                            }
                            FILE_ACTION_MODIFIED => {
                                let _ = event_tx.send(FileChangeEvent::Modified(full_path)).await;
                            }
                            FILE_ACTION_REMOVED => {
                                let _ = event_tx.send(FileChangeEvent::Removed(full_path)).await;
                            }
                            FILE_ACTION_RENAMED_NEW_NAME => {
                                // For renames, we'd need to track the old name from previous event
                                // Simplified: treat as added
                                let _ = event_tx.send(FileChangeEvent::Added(full_path)).await;
                            }
                            _ => {}
                        }
                        
                        // Move to next record
                        offset = if notify.NextEntryOffset == 0 {
                            break;
                        } else {
                            offset + notify.NextEntryOffset as usize
                        };
                    }
                }
            });
            
            Ok(())
        }
    }
}

impl Drop for WindowsWatcher {
    fn drop(&mut self) {
        #[cfg(windows)]
        {
            if let Some(handle) = self.handle.take() {
                unsafe {
                    let _ = CloseHandle(handle);
                }
            }
        }
    }
}

/// Start a Windows-specific file watcher that triggers library rescans
#[cfg(windows)]
pub fn start_watcher(pool: SqlitePool, folder: PathBuf) -> Result<(), String> {
    use crate::library::scanner::scan_folder;
    use crate::library::track_artwork::TrackArtworkCache;
    
    let (tx, mut rx) = mpsc::channel::<FileChangeEvent>(100);
    
    // Create watcher
    let watcher = WindowsWatcher::new(&folder, tx)?;
    
    // Start watching in background
    tokio::spawn(async move {
        // Simple debouncing: collect events within a time window
        let mut pending_changes = std::collections::HashSet::new();
        let debounce_duration = tokio::time::Duration::from_secs(2);
        
        loop {
            tokio::select! {
                Some(event) = rx.recv() => {
                    match event {
                        FileChangeEvent::Added(path) | FileChangeEvent::Modified(path) => {
                            pending_changes.insert(path);
                        }
                        FileChangeEvent::Removed(path) => {
                            pending_changes.insert(path);
                        }
                        FileChangeEvent::Renamed(old, new) => {
                            pending_changes.remove(&old);
                            pending_changes.insert(new);
                        }
                    }
                }
                _ = tokio::time::sleep(debounce_duration) => {
                    if !pending_changes.is_empty() {
                        log::info!("Detected {} file changes, rescanning...", pending_changes.len());
                        
                        // Rescan the folder
                        let artwork_cache = TrackArtworkCache::new();
                        match scan_folder(&pool, &folder, &artwork_cache).await {
                            Ok(result) => {
                                log::info!("Rescan complete: added={}, updated={}, removed={}", 
                                    result.added, result.updated, result.removed);
                            }
                            Err(e) => {
                                log::error!("Rescan failed: {}", e);
                            }
                        }
                        
                        pending_changes.clear();
                    }
                }
            }
        }
    });
    
    // Start the actual watching
    tokio::spawn(async move {
        let _ = watcher.start_watching(pool).await;
    });
    
    Ok(())
}

#[cfg(not(windows))]
pub fn start_watcher(_pool: SqlitePool, _folder: PathBuf) -> Result<(), String> {
    Err("Windows file watcher is only available on Windows".to_string())
}
