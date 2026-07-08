//! Windows-Specific Module for Resona Player
//! 
//! Provides native Windows integration including:
//! - Taskbar features (progress, jump lists, overlay icons)
//! - Media key handling
//! - High DPI awareness
//! - Direct2D/DirectWrite GPU acceleration

pub mod taskbar;

pub use taskbar::{
    TaskbarManager,
    TaskbarProgressState,
    ThumbnailButton,
    JumpListEntry,
    update_jump_list,
    register_media_keys,
    enable_high_dpi,
};
