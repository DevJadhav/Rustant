//! Statistical anomaly detection for SRE operations.
//!
//! Provides lightweight, dependency-free anomaly detection using standard
//! statistical methods: Z-score, IQR, and moving average with standard deviation.

use serde::{Deserialize, Serialize};

/// Detection method for anomaly analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DetectionMethod {
    /// Z-score deviation from mean.
    ZScore {
        /// Minimum data points for reliable detection.
        window_size: usize,
    },
    /// Interquartile range (robust to outliers).
    Iqr {
        /// IQR multiplier (default: 1.5, strict: 1.0, lenient: 3.0).
        multiplier: f64,
    },
    /// Moving average with standard deviation bands.
    MovingAverage {
        /// Window size for moving average.
        window_size: usize,
        /// Standard deviation multiplier for anomaly threshold.
        std_multiplier: f64,
    },
}

impl Default for DetectionMethod {
    fn default() -> Self {
        DetectionMethod::ZScore { window_size: 30 }
    }
}

/// Result of anomaly detection on a single data point.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnomalyResult {
    /// Whether this value is classified as anomalous.
    pub is_anomaly: bool,
    /// Anomaly score from 0.0 (normal) to 1.0 (extreme).
    pub score: f64,
    /// Expected value based on the model.
    pub expected: f64,
    /// Actual observed value.
    pub actual: f64,
    /// Expected range (lower, upper).
    pub expected_range: (f64, f64),
    /// Detection method used.
    pub method: String,
}

/// Anomaly detector using configurable statistical methods.
pub struct AnomalyDetector {
    method: DetectionMethod,
    /// Sensitivity multiplier (0.5 = more sensitive, 2.0 = less sensitive).
    sensitivity: f64,
}

impl AnomalyDetector {
    /// Create a new detector with default Z-score method.
    pub fn new(method: DetectionMethod, sensitivity: f64) -> Self {
        Self {
            method,
            sensitivity: sensitivity.max(0.1),
        }
    }

    /// Create a default Z-score detector.
    pub fn default_zscore() -> Self {
        Self::new(DetectionMethod::ZScore { window_size: 30 }, 1.0)
    }

    /// Create an IQR detector (robust to outliers).
    pub fn default_iqr() -> Self {
        Self::new(DetectionMethod::Iqr { multiplier: 1.5 }, 1.0)
    }

    /// Create a moving average detector.
    pub fn default_moving_average() -> Self {
        Self::new(
            DetectionMethod::MovingAverage {
                window_size: 10,
                std_multiplier: 2.0,
            },
            1.0,
        )
    }

    /// Detect if a new value is anomalous given historical data.
    pub fn detect(&self, data: &[f64], new_value: f64) -> AnomalyResult {
        match &self.method {
            DetectionMethod::ZScore { window_size } => {
                self.detect_zscore(data, new_value, *window_size)
            }
            DetectionMethod::Iqr { multiplier } => self.detect_iqr(data, new_value, *multiplier),
            DetectionMethod::MovingAverage {
                window_size,
                std_multiplier,
            } => self.detect_moving_average(data, new_value, *window_size, *std_multiplier),
        }
    }

    /// Find all anomalies in a data series.
    pub fn detect_batch(&self, data: &[f64]) -> Vec<(usize, AnomalyResult)> {
        if data.len() < 3 {
            return Vec::new();
        }

        let mut results = Vec::new();
        let min_window = match &self.method {
            DetectionMethod::ZScore { window_size } => *window_size,
            DetectionMethod::Iqr { .. } => 4,
            DetectionMethod::MovingAverage { window_size, .. } => *window_size,
        };

        for i in min_window..data.len() {
            let history = &data[..i];
            let result = self.detect(history, data[i]);
            if result.is_anomaly {
                results.push((i, result));
            }
        }

        results
    }

    fn detect_zscore(&self, data: &[f64], new_value: f64, window_size: usize) -> AnomalyResult {
        let window = if data.len() > window_size {
            &data[data.len() - window_size..]
        } else {
            data
        };

        if window.len() < 2 {
            return AnomalyResult {
                is_anomaly: false,
                score: 0.0,
                expected: new_value,
                actual: new_value,
                expected_range: (new_value, new_value),
                method: "z_score".to_string(),
            };
        }

        let mean = mean(window);
        let std_dev = std_deviation(window, mean);

        if std_dev < f64::EPSILON {
            let is_anomaly = (new_value - mean).abs() > f64::EPSILON;
            return AnomalyResult {
                is_anomaly,
                score: if is_anomaly { 1.0 } else { 0.0 },
                expected: mean,
                actual: new_value,
                expected_range: (mean, mean),
                method: "z_score".to_string(),
            };
        }

        let z_score = (new_value - mean).abs() / std_dev;
        let threshold = 2.0 * self.sensitivity;
        let score = (z_score / (threshold * 2.0)).min(1.0);

        AnomalyResult {
            is_anomaly: z_score > threshold,
            score,
            expected: mean,
            actual: new_value,
            expected_range: (mean - threshold * std_dev, mean + threshold * std_dev),
            method: "z_score".to_string(),
        }
    }

    fn detect_iqr(&self, data: &[f64], new_value: f64, multiplier: f64) -> AnomalyResult {
        if data.len() < 4 {
            return AnomalyResult {
                is_anomaly: false,
                score: 0.0,
                expected: new_value,
                actual: new_value,
                expected_range: (new_value, new_value),
                method: "iqr".to_string(),
            };
        }

        let mut sorted = data.to_vec();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        let q1 = percentile(&sorted, 25.0);
        let q3 = percentile(&sorted, 75.0);
        let iqr = q3 - q1;
        let adjusted_multiplier = multiplier * self.sensitivity;

        let lower = q1 - adjusted_multiplier * iqr;
        let upper = q3 + adjusted_multiplier * iqr;
        let median = percentile(&sorted, 50.0);

        let is_anomaly = new_value < lower || new_value > upper;
        let distance = if new_value < lower {
            lower - new_value
        } else if new_value > upper {
            new_value - upper
        } else {
            0.0
        };
        let score = if iqr > f64::EPSILON {
            (distance / iqr).min(1.0)
        } else if is_anomaly {
            1.0
        } else {
            0.0
        };

        AnomalyResult {
            is_anomaly,
            score,
            expected: median,
            actual: new_value,
            expected_range: (lower, upper),
            method: "iqr".to_string(),
        }
    }

    fn detect_moving_average(
        &self,
        data: &[f64],
        new_value: f64,
        window_size: usize,
        std_multiplier: f64,
    ) -> AnomalyResult {
        let window = if data.len() > window_size {
            &data[data.len() - window_size..]
        } else {
            data
        };

        if window.len() < 2 {
            return AnomalyResult {
                is_anomaly: false,
                score: 0.0,
                expected: new_value,
                actual: new_value,
                expected_range: (new_value, new_value),
                method: "moving_average".to_string(),
            };
        }

        let ma = mean(window);
        let std_dev = std_deviation(window, ma);
        let adjusted_multiplier = std_multiplier * self.sensitivity;

        let lower = ma - adjusted_multiplier * std_dev;
        let upper = ma + adjusted_multiplier * std_dev;

        let is_anomaly = new_value < lower || new_value > upper;
        let deviation = (new_value - ma).abs();
        let score = if std_dev > f64::EPSILON {
            (deviation / (adjusted_multiplier * std_dev * 2.0)).min(1.0)
        } else if is_anomaly {
            1.0
        } else {
            0.0
        };

        AnomalyResult {
            is_anomaly,
            score,
            expected: ma,
            actual: new_value,
            expected_range: (lower, upper),
            method: "moving_average".to_string(),
        }
    }
}

/// Compute the mean of a slice.
fn mean(data: &[f64]) -> f64 {
    if data.is_empty() {
        return 0.0;
    }
    data.iter().sum::<f64>() / data.len() as f64
}

/// Compute standard deviation given a precomputed mean.
fn std_deviation(data: &[f64], mean_val: f64) -> f64 {
    if data.len() < 2 {
        return 0.0;
    }
    let variance =
        data.iter().map(|x| (x - mean_val).powi(2)).sum::<f64>() / (data.len() - 1) as f64;
    variance.sqrt()
}

/// Compute a percentile from sorted data using linear interpolation.
fn percentile(sorted: &[f64], pct: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    if sorted.len() == 1 {
        return sorted[0];
    }
    let rank = (pct / 100.0) * (sorted.len() - 1) as f64;
    let lower = rank.floor() as usize;
    let upper = rank.ceil() as usize;
    if lower == upper {
        sorted[lower]
    } else {
        let frac = rank - lower as f64;
        sorted[lower] * (1.0 - frac) + sorted[upper] * frac
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zscore_normal_value() {
        let detector = AnomalyDetector::default_zscore();
        let data: Vec<f64> = (0..30).map(|i| 100.0 + (i as f64 % 5.0)).collect();
        let result = detector.detect(&data, 102.0);
        assert!(!result.is_anomaly);
    }

    #[test]
    fn test_zscore_anomalous_value() {
        let detector = AnomalyDetector::default_zscore();
        let data: Vec<f64> = vec![100.0; 30];
        let result = detector.detect(&data, 200.0);
        assert!(result.is_anomaly);
        assert!(result.score > 0.5);
    }

    #[test]
    fn test_iqr_normal_value() {
        let detector = AnomalyDetector::default_iqr();
        let data: Vec<f64> = (0..20).map(|i| 50.0 + i as f64).collect();
        let result = detector.detect(&data, 55.0);
        assert!(!result.is_anomaly);
    }

    #[test]
    fn test_iqr_anomalous_value() {
        let detector = AnomalyDetector::default_iqr();
        let data: Vec<f64> = vec![10.0, 11.0, 12.0, 10.5, 11.5, 12.5, 10.0, 11.0];
        let result = detector.detect(&data, 50.0);
        assert!(result.is_anomaly);
    }

    #[test]
    fn test_moving_average_normal() {
        let detector = AnomalyDetector::default_moving_average();
        let data: Vec<f64> = (0..15).map(|i| 100.0 + (i as f64 * 0.5)).collect();
        let result = detector.detect(&data, 105.0);
        assert!(!result.is_anomaly);
    }

    #[test]
    fn test_moving_average_anomaly() {
        let detector = AnomalyDetector::default_moving_average();
        let data: Vec<f64> = vec![100.0; 15];
        let result = detector.detect(&data, 150.0);
        assert!(result.is_anomaly);
    }

    #[test]
    fn test_detect_batch() {
        let detector = AnomalyDetector::new(DetectionMethod::ZScore { window_size: 5 }, 1.0);
        let mut data: Vec<f64> = vec![10.0; 10];
        data.push(100.0); // anomaly at index 10
        data.push(10.0);
        let anomalies = detector.detect_batch(&data);
        assert!(!anomalies.is_empty());
        assert_eq!(anomalies[0].0, 10);
    }

    #[test]
    fn test_empty_data() {
        let detector = AnomalyDetector::default_zscore();
        let result = detector.detect(&[], 42.0);
        assert!(!result.is_anomaly);
    }

    #[test]
    fn test_single_data_point() {
        let detector = AnomalyDetector::default_zscore();
        let result = detector.detect(&[42.0], 42.0);
        assert!(!result.is_anomaly);
    }

    #[test]
    fn test_sensitivity_affects_detection() {
        let data: Vec<f64> = vec![100.0; 30];
        let sensitive = AnomalyDetector::new(DetectionMethod::ZScore { window_size: 30 }, 0.5);
        let lenient = AnomalyDetector::new(DetectionMethod::ZScore { window_size: 30 }, 2.0);
        // A moderately unusual value
        let result_sensitive = sensitive.detect(&data, 105.0);
        let result_lenient = lenient.detect(&data, 105.0);
        // More sensitive detector should have higher score
        assert!(result_sensitive.score >= result_lenient.score);
    }
}
