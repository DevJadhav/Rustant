//! Inter-annotator agreement metrics.

use serde::{Deserialize, Serialize};

/// Inter-annotator agreement result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgreementResult {
    pub method: String,
    pub score: f64,
    pub annotator_count: usize,
    pub sample_count: usize,
    pub interpretation: String,
}

/// Calculate Cohen's Kappa for two annotators.
pub fn cohens_kappa(annotations_a: &[i32], annotations_b: &[i32]) -> f64 {
    if annotations_a.len() != annotations_b.len() || annotations_a.is_empty() {
        return 0.0;
    }
    let n = annotations_a.len() as f64;
    let agree = annotations_a
        .iter()
        .zip(annotations_b.iter())
        .filter(|(a, b)| a == b)
        .count() as f64;
    let po = agree / n;

    // Expected agreement
    let categories: std::collections::HashSet<_> =
        annotations_a.iter().chain(annotations_b.iter()).collect();
    let mut pe = 0.0;
    for &cat in &categories {
        let count_a = annotations_a.iter().filter(|&&a| a == *cat).count() as f64;
        let count_b = annotations_b.iter().filter(|&&b| b == *cat).count() as f64;
        pe += (count_a / n) * (count_b / n);
    }

    if (1.0 - pe).abs() < f64::EPSILON {
        return 1.0;
    }
    (po - pe) / (1.0 - pe)
}

/// Interpret a Kappa score.
pub fn interpret_kappa(kappa: f64) -> &'static str {
    if kappa < 0.0 {
        "Poor"
    } else if kappa < 0.20 {
        "Slight"
    } else if kappa < 0.40 {
        "Fair"
    } else if kappa < 0.60 {
        "Moderate"
    } else if kappa < 0.80 {
        "Substantial"
    } else {
        "Almost Perfect"
    }
}

/// Calculate the Intraclass Correlation Coefficient (one-way random model).
///
/// `ratings` is a slice of rows, one per subject. Each row contains the ratings
/// from every rater for that subject. All rows must have the same length (k raters).
///
/// ICC = (MSB - MSW) / (MSB + (k-1)*MSW)
///
/// Returns 0.0 for degenerate inputs (empty, single rater, or zero variance).
pub fn icc(ratings: &[Vec<f64>]) -> f64 {
    if ratings.is_empty() {
        return 0.0;
    }
    let k = ratings[0].len();
    if k < 2 || ratings.iter().any(|r| r.len() != k) {
        return 0.0;
    }
    let n = ratings.len() as f64;
    let kf = k as f64;

    // Grand mean.
    let total: f64 = ratings.iter().flat_map(|r| r.iter()).sum();
    let grand_mean = total / (n * kf);

    // Row means.
    let row_means: Vec<f64> = ratings.iter().map(|r| r.iter().sum::<f64>() / kf).collect();

    // Between-subjects sum of squares.
    let ssb: f64 = row_means
        .iter()
        .map(|m| kf * (m - grand_mean).powi(2))
        .sum();

    // Within-subjects sum of squares.
    let ssw: f64 = ratings
        .iter()
        .zip(row_means.iter())
        .map(|(row, rm)| row.iter().map(|x| (x - rm).powi(2)).sum::<f64>())
        .sum();

    let df_b = n - 1.0;
    let df_w = n * (kf - 1.0);

    if df_b <= 0.0 || df_w <= 0.0 {
        return 0.0;
    }

    let msb = ssb / df_b;
    let msw = ssw / df_w;

    let denom = msb + (kf - 1.0) * msw;
    if denom.abs() < f64::EPSILON {
        return 0.0;
    }

    (msb - msw) / denom
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cohens_kappa_perfect() {
        let a = vec![1, 2, 3, 1, 2];
        let b = vec![1, 2, 3, 1, 2];
        assert!((cohens_kappa(&a, &b) - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_interpret_kappa() {
        assert_eq!(interpret_kappa(0.85), "Almost Perfect");
        assert_eq!(interpret_kappa(0.45), "Moderate");
    }
}
