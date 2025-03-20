mod filter_bank;
mod utils;

// Re-export the public API
pub use filter_bank::MelFilterBank;
pub use utils::{hz_to_mel, mel_to_hz, linspace};

/// Parameters for configuring Mel spectrogram generation
pub struct MelParameters {
    pub n_mels: usize,
    pub f_min: f32,
    pub f_max: f32,
} 