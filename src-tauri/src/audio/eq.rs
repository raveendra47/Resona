use std::f32::consts::PI;

pub const EQ_FREQUENCIES: [f32; 10] = [
    32.0, 64.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0,
];
const EQ_Q: f32 = 1.0;
const EQ_SAMPLE_RATE: f32 = 44100.0;

pub struct Biquad {
    b0: f32, b1: f32, b2: f32, a1: f32, a2: f32,
    x1: f32, x2: f32, y1: f32, y2: f32,
}

impl Biquad {
    pub fn new(b0: f32, b1: f32, b2: f32, a1: f32, a2: f32) -> Self {
        Self { b0, b1, b2, a1, a2, x1: 0.0, x2: 0.0, y1: 0.0, y2: 0.0 }
    }

    pub fn reset(&mut self) {
        self.x1 = 0.0; self.x2 = 0.0; self.y1 = 0.0; self.y2 = 0.0;
    }

    pub fn process(&mut self, sample: f32) -> f32 {
        let y = self.b0 * sample + self.b1 * self.x1 + self.b2 * self.x2
            - self.a1 * self.y1 - self.a2 * self.y2;
        self.x2 = self.x1; self.x1 = sample;
        self.y2 = self.y1; self.y1 = y;
        y
    }
    
    /// Process multiple samples in sequence (SIMD-friendly)
    pub fn process_batch(&mut self, samples: &mut [f32]) {
        if !self.enabled() {
            return;
        }
        
        for sample in samples.iter_mut() {
            let y = self.b0 * *sample + self.b1 * self.x1 + self.b2 * self.x2
                - self.a1 * self.y1 - self.a2 * self.y2;
            self.x2 = self.x1; self.x1 = *sample;
            self.y2 = self.y1; self.y1 = y;
            *sample = y;
        }
    }
    
    fn enabled(&self) -> bool {
        // Check if any coefficient is non-zero (filter is active)
        self.b0 != 0.0 || self.b1 != 0.0 || self.b2 != 0.0 || self.a1 != 0.0 || self.a2 != 0.0
    }
}

/// RBJ peaking filter coefficients.
fn peaking_coeffs(sample_rate: f32, freq: f32, gain_db: f32, q: f32) -> (f32, f32, f32, f32, f32) {
    let a = 10.0_f32.powf(gain_db / 40.0);
    let omega = 2.0 * PI * freq / sample_rate;
    let sin_w = omega.sin();
    let cos_w = omega.cos();
    let alpha = sin_w / (2.0 * q);
    let a_plus_alpha = a * alpha;
    let a_inv_alpha = alpha / a;

    let norm = 1.0 / (1.0 + alpha);
    let b0 = (1.0 + a_inv_alpha) * norm;
    let b1 = (-2.0 * cos_w) * norm;
    let b2 = (1.0 - a_inv_alpha) * norm;
    let a1 = (-2.0 * cos_w) * norm;
    let a2 = (1.0 - a_plus_alpha) * norm;
    (b0, b1, b2, a1, a2)
}

pub struct GraphicEq {
    bands: [Biquad; 10],
    gains: [f32; 10],
    enabled: bool,
}

impl GraphicEq {
    pub fn new() -> Self {
        let bands = [(); 10].map(|_| {
            let (b0, b1, b2, a1, a2) = peaking_coeffs(EQ_SAMPLE_RATE, 1000.0, 0.0, EQ_Q);
            Biquad::new(b0, b1, b2, a1, a2)
        });
        Self { bands, gains: [0.0; 10], enabled: true }
    }

    pub fn set_band(&mut self, index: usize, gain_db: f32) {
        if index >= 10 { return; }
        self.gains[index] = gain_db;
        let (b0, b1, b2, a1, a2) = peaking_coeffs(
            EQ_SAMPLE_RATE, EQ_FREQUENCIES[index], gain_db, EQ_Q,
        );
        self.bands[index] = Biquad::new(b0, b1, b2, a1, a2);
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn reset_all(&mut self) {
        for b in &mut self.bands {
            b.reset();
        }
    }

    /// Process stereo interleaved PCM in-place. No-op when disabled.
    /// Optimized for SIMD with batch processing per band.
    pub fn process(&mut self, samples: &mut [f32]) {
        if !self.enabled {
            return;
        }
        
        // Process left and right channels separately for better SIMD optimization
        let len = samples.len();
        
        // Extract left and right channels
        let mut left = Vec::with_capacity(len / 2);
        let mut right = Vec::with_capacity(len / 2);
        
        for (i, chunk) in samples.chunks_exact(2).enumerate() {
            left.push(chunk[0]);
            right.push(chunk[1]);
        }
        
        // Process each band on all samples (SIMD-friendly pattern)
        for band in &mut self.bands {
            band.process_batch(&mut left);
            band.process_batch(&mut right);
        }
        
        // Interleave back
        for (i, frame) in samples.chunks_exact_mut(2).enumerate() {
            if i < left.len() {
                frame[0] = left[i];
            }
            if i < right.len() {
                frame[1] = right[i];
            }
        }
    }
    
    /// Alternative: Process sample-by-sample (original method, less SIMD-friendly)
    pub fn process_scalar(&mut self, samples: &mut [f32]) {
        if !self.enabled {
            return;
        }
        for frame in samples.chunks_exact_mut(2) {
            frame[0] = self.bands.iter_mut().fold(frame[0], |s, b| b.process(s));
            frame[1] = self.bands.iter_mut().fold(frame[1], |s, b| b.process(s));
        }
    }
}

/// Write a 32-bit float WAV to a Vec<u8> (stereo, 44100Hz).
pub fn write_wav_f32(samples: &[f32], sample_rate: u32) -> Vec<u8> {
    let channels: u16 = 2;
    let bits_per_sample: u16 = 32;
    let byte_rate = sample_rate * channels as u32 * (bits_per_sample as u32 / 8);
    let block_align = channels * (bits_per_sample / 8);
    let data_len = samples.len() as u32 * 4;
    let file_len = 36 + data_len;

    let mut wav = Vec::with_capacity(44 + samples.len() * 4);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&file_len.to_le_bytes());
    wav.extend_from_slice(b"WAVE");
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&3u16.to_le_bytes()); // IEEE float
    wav.extend_from_slice(&channels.to_le_bytes());
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&byte_rate.to_le_bytes());
    wav.extend_from_slice(&block_align.to_le_bytes());
    wav.extend_from_slice(&bits_per_sample.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_len.to_le_bytes());
    for &s in samples {
        wav.extend_from_slice(&s.to_le_bytes());
    }
    wav
}
