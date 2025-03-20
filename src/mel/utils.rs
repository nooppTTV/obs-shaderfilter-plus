use std::f32;

/// Convert frequency (Hz) to Mel scale
pub fn hz_to_mel(f: f32) -> f32 {
    2595.0 * (1.0 + f / 700.0).log10()
}

/// Convert Mel scale value to frequency (Hz)
pub fn mel_to_hz(m: f32) -> f32 {
    700.0 * (10_f32.powf(m / 2595.0) - 1.0)
}

/// Generate linearly spaced values between start and stop with num total points
pub fn linspace(start: f32, stop: f32, num: usize) -> Vec<f32> {
    if num == 0 {
        return vec![];
    }
    if num == 1 {
        return vec![start];
    }
    let step = (stop - start) / (num - 1) as f32;
    (0..num).map(|i| start + i as f32 * step).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hz_to_mel() {
        let mel = hz_to_mel(1000.0);
        assert!(mel > 0.0);
        // Specific value test
        assert!((mel - 1000.0).abs() < 1.0);
    }

    #[test]
    fn test_mel_to_hz() {
        let hz = mel_to_hz(1000.0);
        assert!(hz > 0.0);
    }

    #[test]
    fn test_linspace() {
        let values = linspace(0.0, 10.0, 11);
        assert_eq!(values.len(), 11);
        assert_eq!(values[0], 0.0);
        assert_eq!(values[10], 10.0);
    }
} 