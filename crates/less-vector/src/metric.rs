//! Distance metrics ("spaces") for vector search.

use serde::{Deserialize, Serialize};

use less_common::{LessError, Result};

/// Vector metric space. `distance(a, b)` returns a score where **smaller
/// is closer**, so all spaces share the same ordering convention.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Metric {
    /// Squared Euclidean distance.
    L2,
    /// 1 - cosine similarity (vectors are unit-normalized on insert).
    Cosine,
    /// Negative inner product.
    Dot,
}

impl Metric {
    pub fn parse(s: &str) -> Result<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "l2" | "euclidean" | "euclid" => Ok(Self::L2),
            "cosine" | "cos" => Ok(Self::Cosine),
            "dot" | "ip" | "inner_product" | "dotproduct" => Ok(Self::Dot),
            _ => Err(LessError::Config(format!(
                "unknown metric '{s}' (expected l2 | cosine | dot)"
            ))),
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::L2 => "l2",
            Self::Cosine => "cosine",
            Self::Dot => "dot",
        }
    }

    /// Normalize in place where the metric requires it (cosine).
    pub fn prepare(&self, v: &mut [f32]) {
        if matches!(self, Self::Cosine) {
            let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
            if norm > 0.0 {
                for x in v.iter_mut() {
                    *x /= norm;
                }
            }
        }
    }

    /// Distance between two prepared vectors. Smaller = closer.
    pub fn distance(&self, a: &[f32], b: &[f32]) -> f32 {
        debug_assert_eq!(a.len(), b.len());
        match self {
            Self::L2 => a
                .iter()
                .zip(b.iter())
                .map(|(x, y)| {
                    let d = x - y;
                    d * d
                })
                .sum(),
            Self::Cosine => {
                // Vectors are unit-normalized, so cosine = dot product.
                1.0 - a.iter().zip(b.iter()).map(|(x, y)| x * y).sum::<f32>()
            }
            Self::Dot => -a.iter().zip(b.iter()).map(|(x, y)| x * y).sum::<f32>(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn l2_distance() {
        let a = [0.0f32, 3.0, 4.0];
        let b = [0.0f32, 0.0, 0.0];
        assert_eq!(Metric::L2.distance(&a, &b), 25.0);
        assert_eq!(Metric::L2.distance(&a, &a), 0.0);
    }

    #[test]
    fn cosine_normalizes_and_orders() {
        let mut a = vec![1.0f32, 0.0];
        let mut b = vec![0.0f32, 1.0];
        Metric::Cosine.prepare(&mut a);
        Metric::Cosine.prepare(&mut b);
        // Orthogonal unit vectors: distance 1.
        assert!((Metric::Cosine.distance(&a, &b) - 1.0).abs() < 1e-6);
        assert!(Metric::Cosine.distance(&a, &a) < 1e-6);
    }

    #[test]
    fn dot_ordering() {
        let a = [1.0f32, 0.0, 0.0];
        let b = [0.9f32, 0.0, 0.0];
        let c = [0.0f32, 1.0, 0.0];
        // a~b (dot .9) is closer than a~c (dot 0).
        assert!(Metric::Dot.distance(&a, &b) < Metric::Dot.distance(&a, &c));
    }

    #[test]
    fn parse_names() {
        assert_eq!(Metric::parse("L2").unwrap(), Metric::L2);
        assert_eq!(Metric::parse("cos").unwrap(), Metric::Cosine);
        assert_eq!(Metric::parse("ip").unwrap(), Metric::Dot);
        assert!(Metric::parse("hamming").is_err());
    }
}
