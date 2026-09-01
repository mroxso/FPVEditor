//! `.cube` 3D LUT parsing and trilinear sampling — the CPU-side reference
//! implementation; the GPU path uploads the same grid as a 3D texture and
//! samples it in a shader, but this is what the renderer is verified
//! against.

use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub struct Lut3D {
    pub size: usize,
    /// RGB triples, indexed as `data[b * size * size + g * size + r]`
    /// (matches the standard `.cube` file ordering: red fastest).
    pub data: Vec<[f32; 3]>,
    pub domain_min: [f32; 3],
    pub domain_max: [f32; 3],
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum LutError {
    #[error("missing LUT_3D_SIZE directive")]
    MissingSize,
    #[error("expected {expected} data rows, found {found}")]
    WrongRowCount { expected: usize, found: usize },
    #[error("malformed data row: {0}")]
    BadRow(String),
}

impl Lut3D {
    /// An identity LUT at a given grid resolution (useful as a default / for tests).
    pub fn identity(size: usize) -> Self {
        let mut data = Vec::with_capacity(size * size * size);
        for b in 0..size {
            for g in 0..size {
                for r in 0..size {
                    let denom = (size - 1).max(1) as f32;
                    data.push([r as f32 / denom, g as f32 / denom, b as f32 / denom]);
                }
            }
        }
        Self {
            size,
            data,
            domain_min: [0.0, 0.0, 0.0],
            domain_max: [1.0, 1.0, 1.0],
        }
    }

    fn at(&self, r: usize, g: usize, b: usize) -> [f32; 3] {
        self.data[b * self.size * self.size + g * self.size + r]
    }

    /// Sample the LUT at a color in `[domain_min, domain_max]` using
    /// trilinear interpolation between the 8 surrounding grid points.
    pub fn sample(&self, color: [f32; 3]) -> [f32; 3] {
        let n = self.size - 1;
        let mut grid = [0.0f32; 3];
        for i in 0..3 {
            let range = (self.domain_max[i] - self.domain_min[i]).max(1e-9);
            let normalized = ((color[i] - self.domain_min[i]) / range).clamp(0.0, 1.0);
            grid[i] = normalized * n as f32;
        }

        let r0 = (grid[0].floor() as usize).min(n);
        let g0 = (grid[1].floor() as usize).min(n);
        let b0 = (grid[2].floor() as usize).min(n);
        let r1 = (r0 + 1).min(n);
        let g1 = (g0 + 1).min(n);
        let b1 = (b0 + 1).min(n);
        let fr = grid[0] - r0 as f32;
        let fg = grid[1] - g0 as f32;
        let fb = grid[2] - b0 as f32;

        let c000 = self.at(r0, g0, b0);
        let c100 = self.at(r1, g0, b0);
        let c010 = self.at(r0, g1, b0);
        let c110 = self.at(r1, g1, b0);
        let c001 = self.at(r0, g0, b1);
        let c101 = self.at(r1, g0, b1);
        let c011 = self.at(r0, g1, b1);
        let c111 = self.at(r1, g1, b1);

        let mut out = [0.0f32; 3];
        for i in 0..3 {
            let c00 = lerp(c000[i], c100[i], fr);
            let c10 = lerp(c010[i], c110[i], fr);
            let c01 = lerp(c001[i], c101[i], fr);
            let c11 = lerp(c011[i], c111[i], fr);
            let c0 = lerp(c00, c10, fg);
            let c1 = lerp(c01, c11, fg);
            out[i] = lerp(c0, c1, fb);
        }
        out
    }

    /// Parse Adobe/Iridas `.cube` LUT text.
    pub fn parse(text: &str) -> Result<Self, LutError> {
        let mut size: Option<usize> = None;
        let mut domain_min = [0.0f32, 0.0, 0.0];
        let mut domain_max = [1.0f32, 1.0, 1.0];
        let mut rows: Vec<[f32; 3]> = Vec::new();

        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some(rest) = line.strip_prefix("LUT_3D_SIZE") {
                size = rest.trim().parse::<usize>().ok();
                continue;
            }
            if let Some(rest) = line.strip_prefix("DOMAIN_MIN") {
                domain_min = parse_triple(rest).ok_or_else(|| LutError::BadRow(line.to_string()))?;
                continue;
            }
            if let Some(rest) = line.strip_prefix("DOMAIN_MAX") {
                domain_max = parse_triple(rest).ok_or_else(|| LutError::BadRow(line.to_string()))?;
                continue;
            }
            if line.starts_with("TITLE") || line.starts_with("LUT_1D_SIZE") {
                continue;
            }
            let triple = parse_triple(line).ok_or_else(|| LutError::BadRow(line.to_string()))?;
            rows.push(triple);
        }

        let size = size.ok_or(LutError::MissingSize)?;
        let expected = size * size * size;
        if rows.len() != expected {
            return Err(LutError::WrongRowCount {
                expected,
                found: rows.len(),
            });
        }

        Ok(Self {
            size,
            data: rows,
            domain_min,
            domain_max,
        })
    }
}

fn parse_triple(s: &str) -> Option<[f32; 3]> {
    let parts: Vec<&str> = s.split_whitespace().collect();
    if parts.len() != 3 {
        return None;
    }
    Some([
        parts[0].parse().ok()?,
        parts[1].parse().ok()?,
        parts[2].parse().ok()?,
    ])
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// A tiny registry of named LUTs, for lookup by the `lut_path` string a
/// command carries (real filesystem loading lives at a higher layer; this
/// keeps `fpv-gpu` testable without touching disk).
#[derive(Default)]
pub struct LutCache {
    entries: HashMap<String, Lut3D>,
}

impl LutCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, key: impl Into<String>, lut: Lut3D) {
        self.entries.insert(key.into(), lut);
    }

    pub fn get(&self, key: &str) -> Option<&Lut3D> {
        self.entries.get(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_lut_leaves_colors_unchanged() {
        let lut = Lut3D::identity(9);
        let color = [0.37, 0.62, 0.11];
        let out = lut.sample(color);
        assert!((out[0] - color[0]).abs() < 1e-3);
        assert!((out[1] - color[1]).abs() < 1e-3);
        assert!((out[2] - color[2]).abs() < 1e-3);
    }

    #[test]
    fn parses_a_minimal_cube_file() {
        let text = "\
TITLE \"test\"
LUT_3D_SIZE 2
0.0 0.0 0.0
1.0 0.0 0.0
0.0 1.0 0.0
1.0 1.0 0.0
0.0 0.0 1.0
1.0 0.0 1.0
0.0 1.0 1.0
1.0 1.0 1.0
";
        let lut = Lut3D::parse(text).unwrap();
        assert_eq!(lut.size, 2);
        assert_eq!(lut.data.len(), 8);
        // Corner (r=1,g=1,b=1) should be pure white.
        assert_eq!(lut.at(1, 1, 1), [1.0, 1.0, 1.0]);
    }

    #[test]
    fn missing_size_directive_is_an_error() {
        let err = Lut3D::parse("0.0 0.0 0.0\n").unwrap_err();
        assert_eq!(err, LutError::MissingSize);
    }

    #[test]
    fn wrong_row_count_is_an_error() {
        let text = "LUT_3D_SIZE 2\n0.0 0.0 0.0\n1.0 1.0 1.0\n";
        let err = Lut3D::parse(text).unwrap_err();
        assert_eq!(
            err,
            LutError::WrongRowCount {
                expected: 8,
                found: 2
            }
        );
    }

    #[test]
    fn a_lut_that_maps_everything_to_red_saturates_output_to_red() {
        let mut data = Vec::new();
        for _ in 0..(4 * 4 * 4) {
            data.push([1.0, 0.0, 0.0]);
        }
        let lut = Lut3D {
            size: 4,
            data,
            domain_min: [0.0, 0.0, 0.0],
            domain_max: [1.0, 1.0, 1.0],
        };
        let out = lut.sample([0.5, 0.5, 0.5]);
        assert_eq!(out, [1.0, 0.0, 0.0]);
    }

    #[test]
    fn cache_stores_and_retrieves_by_key() {
        let mut cache = LutCache::new();
        cache.insert("warm", Lut3D::identity(4));
        assert!(cache.get("warm").is_some());
        assert!(cache.get("missing").is_none());
    }
}
