//! GPU Module for Windows-Only Resona Player
//! 
//! Provides hardware-accelerated image processing and effects using
//! Direct2D, DirectWrite, and wgpu for Intel Iris Xe GPUs.

pub mod artwork_engine;

pub use artwork_engine::{GpuArtworkEngine, extract_dominant_colors_gpu};
