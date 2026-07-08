//! LUFS (Loudness Units Full Scale) normalization using EBU R128 standard
//! 
//! This module provides loudness measurement and normalization for consistent
//! playback volume across tracks with different mastering levels.

use loudness_rs::{EbuR128, Measurement};
use std::path::Path;
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

/// LUFS measurement result
#[derive(Debug, Clone)]
pub struct LufsResult {
    /// Integrated loudness in LUFS
    pub integrated_loudness: f32,
    /// Loudness range in LU
    pub loudness_range: f32,
    /// True peak in dBTP
    pub true_peak: f32,
    /// Recommended gain adjustment in dB
    pub recommended_gain: f32,
}

impl Default for LufsResult {
    fn default() -> Self {
        Self {
            integrated_loudness: -23.0, // EBU R128 target
            loudness_range: 0.0,
            true_peak: 0.0,
            recommended_gain: 0.0,
        }
    }
}

/// Calculate LUFS for an audio file
/// 
/// This analyzes the entire track to determine its perceived loudness
/// according to the EBU R128 standard.
pub fn calculate_lufs(file_path: &Path) -> Result<LufsResult, String> {
    // Open the media source
    let file = std::fs::File::open(file_path)
        .map_err(|e| format!("Failed to open file: {}", e))?;
    
    let mss = MediaSourceStream::new(Box::new(file));
    
    // Create a hint to help the format detector
    let mut hint = Hint::new();
    if let Some(ext) = file_path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    
    // Use default options
    let format_opts = FormatOptions::default();
    let metadata_opts = MetadataOptions::default();
    
    // Probe the media source
    let probe = symphonia::default::get_probe();
    let format = probe
        .format(&mut mss.clone(), &hint, &format_opts, &metadata_opts)
        .map_err(|e| format!("Failed to probe format: {}", e))?;
    
    // Find the first audio track
    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or("No audio track found")?;
    
    let codec_params = &track.codec_params;
    
    // Get decoder
    let mut decoder = symphonia::default::get_codecs()
        .make(codec_params, &DecoderOptions::default())
        .map_err(|e| format!("Failed to create decoder: {}", e))?;
    
    // Initialize EBU R128 meter
    let mut ebu = EbuR128::new(2); // Stereo
    
    // Process audio frames
    while let Ok(packet) = format.next_packet() {
        if packet.track_id() != track.id {
            continue;
        }
        
        while let Ok(decoded) = decoder.decode(&packet) {
            // Convert to f32 samples
            let spec = decoded.spec();
            let duration = spec.capacity() as u64;
            
            for ch in 0..spec.channels().count() {
                let channel_data = spec.channel(ch);
                
                // Feed samples to EBU R128 meter
                for sample in channel_data.iter() {
                    ebu.add_sample(*sample as f32);
                }
            }
            
            // Advance by the number of samples processed
            ebu.advance(duration);
        }
    }
    
    // Get measurements
    let integrated = ebu.integrated_loudness();
    let range = ebu.loudness_range();
    let true_peak = ebu.true_peak();
    
    // Calculate recommended gain adjustment
    // Target is typically -23 LUFS (EBU R128) or -14 LUFS (Spotify/YouTube normalization)
    let target_lufs = -23.0;
    let recommended_gain = target_lufs - integrated;
    
    Ok(LufsResult {
        integrated_loudness: integrated,
        loudness_range: range,
        true_peak,
        recommended_gain: recommended_gain.clamp(-15.0, 15.0), // Limit gain adjustment
    })
}

/// Apply LUFS-based gain adjustment to audio samples
/// 
/// This function applies the calculated gain adjustment to normalize
/// the perceived loudness of audio samples.
pub fn apply_lufs_gain(samples: &mut [f32], gain_db: f32) {
    if gain_db.abs() < 0.01 {
        return; // No adjustment needed
    }
    
    // Convert dB to linear gain
    let linear_gain = 10.0_f32.powf(gain_db / 20.0);
    
    // Apply gain with clipping prevention
    for sample in samples.iter_mut() {
        *sample = (*sample * linear_gain).clamp(-1.0, 1.0);
    }
}

/// Volume normalization modes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NormalizationMode {
    /// Normalize each track individually (consistent per-track volume)
    Track,
    /// Normalize based on album loudness (preserves dynamic range within albums)
    Album,
    /// No normalization
    Off,
}

impl Default for NormalizationMode {
    fn default() -> Self {
        NormalizationMode::Track
    }
}

/// Cache for LUFS measurements to avoid re-analysis
#[derive(Debug, Clone)]
pub struct LufsCache {
    /// Map of file path hash to LUFS result
    measurements: std::collections::HashMap<u64, LufsResult>,
}

impl LufsCache {
    pub fn new() -> Self {
        Self {
            measurements: std::collections::HashMap::new(),
        }
    }
    
    pub fn get(&self, file_path: &Path) -> Option<&LufsResult> {
        let hash = self.hash_path(file_path);
        self.measurements.get(&hash)
    }
    
    pub fn insert(&mut self, file_path: &Path, result: LufsResult) {
        let hash = self.hash_path(file_path);
        self.measurements.insert(hash, result);
    }
    
    fn hash_path(&self, path: &Path) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        path.hash(&mut hasher);
        hasher.finish()
    }
}

impl Default for LufsCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_gain_application() {
        let mut samples = vec![0.5f32; 100];
        apply_lufs_gain(&mut samples, 6.0); // +6dB
        
        // Samples should be amplified but clipped at 1.0
        for sample in &samples {
            assert!(*sample <= 1.0);
            assert!(*sample >= 0.5);
        }
    }
    
    #[test]
    fn test_gain_noop() {
        let mut samples = vec![0.5f32; 100];
        let original = samples.clone();
        apply_lufs_gain(&mut samples, 0.0);
        
        assert_eq!(samples, original);
    }
}
