use super::utils::{hz_to_mel, mel_to_hz, linspace};

/// Struct representing the Mel Filter Bank
pub struct MelFilterBank {
    filters: Vec<Vec<f32>>, // Each filter is a vector of coefficients
    fft_size: usize,
    sample_rate: f32,
    n_mels: usize,
    f_min: f32,
    f_max: f32,
}

impl MelFilterBank {
    /// Create a new MelFilterBank and compute triangular filters
    pub fn new(fft_size: usize, sample_rate: f32, n_mels: usize, f_min: f32, f_max: f32) -> Self {
        // Number of FFT bins (only need non-redundant bins)
        let n_fft_bins = fft_size / 2 + 1;

        // Compute Mel boundaries for f_min and f_max
        let mel_min = hz_to_mel(f_min);
        let mel_max = hz_to_mel(f_max);
        // Compute n_mels + 2 points in Mel scale (including the boundaries)
        let mel_points = linspace(mel_min, mel_max, n_mels + 2);
        // Convert Mel points back to Hz
        let hz_points: Vec<f32> = mel_points.iter().map(|&m| mel_to_hz(m)).collect();
        // Map each Hz point to the corresponding FFT bin
        let bin: Vec<usize> = hz_points.iter()
            .map(|&hz| ((fft_size as f32 + 1.0) * hz / sample_rate).floor() as usize)
            .collect();

        let mut filters = Vec::with_capacity(n_mels);
        for i in 0..n_mels {
            let mut filter = vec![0.0; n_fft_bins];
            let bin_left = bin[i];
            let bin_center = bin[i + 1];
            let bin_right = bin[i + 2];

            // Rising slope of the triangular filter
            for j in bin_left..bin_center {
                if bin_center != bin_left {
                    filter[j] = (j - bin_left) as f32 / (bin_center - bin_left) as f32;
                }
            }
            // Falling slope of the triangular filter
            for j in bin_center..bin_right {
                if bin_right != bin_center {
                    filter[j] = (bin_right - j) as f32 / (bin_right - bin_center) as f32;
                }
            }
            filters.push(filter);
        }

        MelFilterBank {
            filters,
            fft_size,
            sample_rate,
            n_mels,
            f_min,
            f_max,
        }
    }

    /// Apply the filter bank to a slice of FFT magnitudes
    pub fn apply(&self, fft_magnitudes: &[f32]) -> Vec<f32> {
        let mut mel_output = vec![0.0; self.n_mels];
        let n_fft_bins = fft_magnitudes.len();
        for (i, filter) in self.filters.iter().enumerate() {
            let mut sum = 0.0;
            let bins = n_fft_bins.min(filter.len());
            for j in 0..bins {
                sum += filter[j] * fft_magnitudes[j];
            }
            mel_output[i] = sum;
        }
        mel_output
    }
    
    /// Apply log compression to mel values
    pub fn apply_log_compression(mel_values: &[f32], offset: f32) -> Vec<f32> {
        mel_values.iter()
            .map(|&x| 10.0 * (x + offset).log10())
            .collect()
    }
    
    /// Normalize MEL values to the range [0.0, 1.0]
    /// 
    /// This helps ensure consistent signal strength across different audio inputs
    /// by mapping the dynamic range of the MEL values to a standardized range.
    pub fn normalize(mel_values: &[f32]) -> Vec<f32> {
        if mel_values.is_empty() {
            return vec![];
        }
        
        // Find the min and max values
        let mut min_val = mel_values[0];
        let mut max_val = mel_values[0];
        
        for &val in mel_values.iter() {
            min_val = min_val.min(val);
            max_val = max_val.max(val);
        }
        
        // If range is zero, return values scaled to 0.5 to avoid division by zero
        if (max_val - min_val).abs() < 1e-6 {
            return mel_values.iter().map(|_| 0.5).collect();
        }
        
        // Normalize to [0, 1] range
        mel_values.iter()
            .map(|&x| (x - min_val) / (max_val - min_val))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mel_filter_bank() {
        let fft_size = 512;
        let sample_rate = 16000.0;
        let n_mels = 10;
        let f_min = 0.0;
        let f_max = 8000.0;
        let filterbank = MelFilterBank::new(fft_size, sample_rate, n_mels, f_min, f_max);

        // Create a dummy FFT magnitude vector with an impulse at a specific bin
        let mut fft_magnitudes = vec![0.0; fft_size / 2 + 1];
        fft_magnitudes[10] = 1.0;

        let mel_output = filterbank.apply(&fft_magnitudes);
        let sum: f32 = mel_output.iter().sum();
        // Expect some non-zero output if the impulse falls within one of the filter triangles
        assert!(sum > 0.0);
    }
    
    #[test]
    fn test_log_compression() {
        let values = vec![0.1, 1.0, 10.0];
        let log_values = MelFilterBank::apply_log_compression(&values, 1e-10);
        
        // Log values should be in a different range
        assert!(log_values[0] < 0.0); // log10 of 0.1 should be negative
        assert!(log_values[2] > log_values[1]); // ordering should be preserved
    }
} 