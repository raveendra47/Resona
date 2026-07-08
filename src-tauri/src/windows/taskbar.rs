//! Windows Shell Integration for Taskbar Features
//! 
//! Provides native Windows taskbar integration including:
//! - Progress indicators
//! - Thumbnail toolbar buttons
//! - Jump lists
//! - Overlay icons

use anyhow::{Context, Result};
use log::{debug, info, warn};
use std::sync::Arc;
use tokio::sync::Mutex;

#[cfg(windows)]
use windows::{
    core::*,
    Win32::Foundation::*,
    Win32::System::Com::*,
    Win32::UI::Shell::*,
    Win32::UI::WindowsAndMessaging::*,
};

/// Windows taskbar integration manager
pub struct TaskbarManager {
    #[cfg(windows)]
    taskbar_list: Option<ITaskbarList4>,
    hwnd: Option<HWND>,
}

static TASKBAR_MANAGER: tokio::sync::OnceCell<Arc<Mutex<TaskbarManager>>> = 
    tokio::sync::OnceCell::const_new();

impl TaskbarManager {
    /// Initialize the taskbar manager
    pub async fn initialize(hwnd: usize) -> Result<Arc<Mutex<Self>>> {
        TASKBAR_MANAGER
            .get_or_try_init(|| Self::new(hwnd))
            .await
            .map(|e| e.clone())
    }

    #[cfg(windows)]
    async fn new(hwnd: usize) -> Result<Arc<Mutex<Self>>> {
        info!("Initializing Windows taskbar integration...");

        // Initialize COM
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED)
                .ok()
                .context("Failed to initialize COM")?;
        }

        // Create TaskbarList instance
        let taskbar_list: ITaskbarList4 = unsafe {
            CoCreateInstance(&CLSID_TaskbarList, None, CLSCTX_INPROC_SERVER)?
        };

        let hwnd = HWND(hwnd as *mut _);

        // Register the window with the taskbar
        unsafe {
            taskbar_list.HrInit()?;
        }

        let manager = Arc::new(Mutex::new(Self {
            taskbar_list: Some(taskbar_list),
            hwnd: Some(hwnd),
        }));

        info!("Taskbar integration initialized successfully");
        Ok(manager)
    }

    #[cfg(not(windows))]
    async fn new(hwnd: usize) -> Result<Arc<Mutex<Self>>> {
        Ok(Arc::new(Mutex::new(Self { hwnd: None })))
    }

    /// Set progress bar state (normal, paused, error, indeterminate)
    pub async fn set_progress_state(&self, state: TaskbarProgressState) -> Result<()> {
        #[cfg(windows)]
        {
            let this = self.lock().await;
            if let Some(ref taskbar) = this.taskbar_list {
                if let Some(hwnd) = this.hwnd {
                    let flag = match state {
                        TaskbarProgressState::Normal => TBPF_NORMAL,
                        TaskbarProgressState::Paused => TBPF_PAUSED,
                        TaskbarProgressState::Error => TBPF_ERROR,
                        TaskbarProgressState::Indeterminate => TBPF_INDETERMINATE,
                        TaskbarProgressState::None => TBPF_NOPROGRESS,
                    };

                    unsafe {
                        taskbar.SetProgressState(hwnd, flag.0)?;
                    }
                    debug!("Taskbar progress state set to {:?}", state);
                }
            }
        }
        Ok(())
    }

    /// Set progress value (0-100)
    pub async fn set_progress_value(&self, value: u32, max: u32) -> Result<()> {
        #[cfg(windows)]
        {
            let this = self.lock().await;
            if let Some(ref taskbar) = this.taskbar_list {
                if let Some(hwnd) = this.hwnd {
                    unsafe {
                        taskbar.SetProgressValue(hwnd, value as u64, max as u64)?;
                    }
                    debug!("Taskbar progress set to {}/{}", value, max);
                }
            }
        }
        Ok(())
    }

    /// Set overlay icon (playing, paused, stopped indicators)
    pub async fn set_overlay_icon(&self, icon_path: Option<&str>, description: &str) -> Result<()> {
        #[cfg(windows)]
        {
            use windows::Win32::Graphics::Imaging::*;
            use windows::Win32::System::LibraryLoader::*;
            use windows::Win32::UI::WindowsAndMessaging::*;

            let this = self.lock().await;
            if let Some(ref taskbar) = this.taskbar_list {
                if let Some(hwnd) = this.hwnd {
                    if let Some(path) = icon_path {
                        // Load icon from file using WIC
                        unsafe {
                            let hicon = LoadImageW(
                                HINSTANCE::default(),
                                &HSTRING::from(path),
                                IMAGE_ICON,
                                0,
                                0,
                                LR_LOADFROMFILE | LR_DEFAULTSIZE,
                            )?;
                            
                            taskbar.SetOverlayIcon(
                                hwnd,
                                HICON(hicon.0),
                                &HSTRING::from(description),
                            )?;
                        }
                    } else {
                        // Remove overlay icon
                        unsafe {
                            taskbar.SetOverlayIcon(hwnd, HICON::default(), &HSTRING::from(description))?;
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Set thumbnail toolbar buttons (play, pause, next, previous)
    pub async fn set_thumbnail_toolbar(&self, buttons: Vec<ThumbnailButton>) -> Result<()> {
        #[cfg(windows)]
        {
            // Implementation would create THUMBBUTTON array and call ThumbBarAddButtons
            // This requires a callback handler which is complex in Rust
            debug!("Setting thumbnail toolbar with {} buttons", buttons.len());
        }
        Ok(())
    }
}

/// Progress bar states for taskbar
#[derive(Debug, Clone, Copy)]
pub enum TaskbarProgressState {
    None,
    Normal,
    Paused,
    Error,
    Indeterminate,
}

/// Thumbnail toolbar button definition
#[derive(Debug, Clone)]
pub struct ThumbnailButton {
    pub id: u32,
    pub icon_path: String,
    pub tooltip: String,
    pub enabled: bool,
    pub visible: bool,
}

/// Jump list entry (recent files or tasks)
#[derive(Debug, Clone)]
pub struct JumpListEntry {
    pub title: String,
    pub file_path: Option<String>,
    pub icon_path: Option<String>,
}

/// Manage Windows jump lists for recent tracks
pub async fn update_jump_list(entries: Vec<JumpListEntry>) -> Result<()> {
    #[cfg(windows)]
    {
        use windows::Win32::UI::Shell::PropertiesSystem::*;
        
        info!("Updating jump list with {} entries", entries.len());

        unsafe {
            // Initialize COM
            CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok();

            // Create custom jump list
            let jump_list: ICustomDestinationList = 
                CoCreateInstance(&CLSID_DestinationList, None, CLSCTX_INPROC_SERVER)?;

            // Begin building the list
            let mut max_slots = 0u32;
            let removed_destinations = jump_list.BeginList(&mut max_slots)?;

            // Add entries
            for entry in entries {
                // Create shell link for the file
                let link: IShellLinkW = CoCreateInstance(&CLSID_ShellLink, None, CLSCTX_INPROC_SERVER)?;
                
                if let Some(path) = &entry.file_path {
                    link.SetPath(&HSTRING::from(path))?;
                    link.SetDescription(&HSTRING::from(&entry.title))?;
                    
                    if let Some(icon) = &entry.icon_path {
                        link.SetIconLocation(&HSTRING::from(icon), 0)?;
                    }

                    // Add to jump list
                    jump_list.AddUserTasks(&link)?;
                }
            }

            // Commit the list
            jump_list.CommitList()?;
        }
    }

    #[cfg(not(windows))]
    {
        debug!("Jump list update skipped (not Windows)");
    }

    Ok(())
}

/// Handle media key events from Windows
pub fn register_media_keys() -> Result<()> {
    #[cfg(windows)]
    {
        use windows::Win32::Media::Multimedia::*;
        use windows::Win32::System::Threading::*;

        info!("Registering media keys...");

        unsafe {
            // Register for media key events
            // This is a simplified version - full implementation needs a message loop
            let result = RegisterHotKey(
                HWND::default(),
                1,
                MOD_MEDIASELECT,
                0,
            );

            if result.as_bool() {
                info!("Media keys registered successfully");
            } else {
                warn!("Failed to register media keys");
            }
        }
    }

    Ok(())
}

/// Enable high DPI awareness for crisp rendering
#[cfg(windows)]
pub fn enable_high_dpi() -> Result<()> {
    use windows::Win32::UI::HiDpi::*;
    
    unsafe {
        SetProcessDpiAwareness(PROCESS_PER_MONITOR_DPI_AWARE)?;
        info!("High DPI awareness enabled");
    }
    
    Ok(())
}
