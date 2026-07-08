pub mod audio;
pub mod commands;
pub mod discord_rpc;
pub mod gpu;
pub mod lastfm;
pub mod library;
pub mod power;
pub mod radio;
pub mod remote;
pub mod search;
pub mod state;
pub mod windows;

#[cfg(windows)]
pub use windows::{
    TaskbarManager,
    TaskbarProgressState,
    ThumbnailButton,
    JumpListEntry,
    update_jump_list,
    register_media_keys,
    enable_high_dpi,
};

#[cfg(windows)]
pub use gpu::{GpuArtworkEngine, extract_dominant_colors_gpu};
