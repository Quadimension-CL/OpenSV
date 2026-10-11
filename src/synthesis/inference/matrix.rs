//! Prepared dense weights for CPU float execution.

use std::{fmt, sync::Arc};

/// Structural errors are reported before an operator writes its output.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MatrixError {
    InvalidShape,
    InvalidLength {
        buffer: &'static str,
        expected: usize,
        actual: usize,
    },
    UnsupportedBits(u32),
    InvalidScale,
    Parallel(String),
    Cancelled,
}

impl fmt::Display for MatrixError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidShape => f.write_str("Dense matrix dimensions are empty or overflow"),
            Self::InvalidLength {
                buffer,
                expected,
                actual,
            } => write!(
                f,
                "Invalid {buffer} length: expected {expected}, got {actual}"
            ),
            Self::UnsupportedBits(bits) => {
                write!(f, "Unsupported integer matrix precision: {bits}")
            }
            Self::InvalidScale => {
                f.write_str("Integer matrix scale must be finite and nonnegative")
            }
            Self::Parallel(message) => f.write_str(message),
            Self::Cancelled => f.write_str("合成任务已由更新的编辑替代。"),
        }
    }
}

impl std::error::Error for MatrixError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Quantization {
    pub bits: u32,
    /// The signed fixed-point input exponent used by integer execution paths.
    /// It does not enter the dense float weight conversion.
    pub input_exponent: i32,
}

/// The prepared representation is row-major: output channel, then input channel.
/// Clones share immutable weights; execution performs no decoding or allocation.
#[derive(Clone)]
pub struct DenseMatrix {
    rows: usize,
    columns: usize,
    values: Arc<[f32]>,
    quantization: Option<Quantization>,
}

impl DenseMatrix {
    /// Float matrix resources are already row-major.
    pub fn from_float(
        rows: usize,
        columns: usize,
        values: Arc<[f32]>,
    ) -> Result<Self, MatrixError> {
        check_length("float weights", elements(rows, columns)?, values.len())?;
        Ok(Self {
            rows,
            columns,
            values,
            quantization: None,
        })
    }

    /// Integer resources are column-major, unlike float resources.
    ///
    /// The dense float path first computes scale / 2^(bits-1),
    /// then multiplies the signed stored integer. This conversion is cached
    /// once; it is not an emulation of an integer dot-product backend.
    pub fn from_quantized(
        rows: usize,
        columns: usize,
        bits: u32,
        input_exponent: i32,
        scales: &[f32],
        values: &[u8],
    ) -> Result<Self, MatrixError> {
        let count = elements(rows, columns)?;
        let (bytes, divisor) = match bits {
            8 => (1, 1.0 / 128.0),
            16 => (2, 1.0 / 32768.0),
            _ => return Err(MatrixError::UnsupportedBits(bits)),
        };
        let expected = count.checked_mul(bytes).ok_or(MatrixError::InvalidShape)?;
        check_length("integer weights", expected, values.len())?;
        check_length("row scales", rows, scales.len())?;
        if scales
            .iter()
            .any(|scale| !scale.is_finite() || *scale < 0.0)
        {
            return Err(MatrixError::InvalidScale);
        }
        let mut decoded = vec![0.0; count];
        for (output, &scale) in scales.iter().enumerate() {
            let factor = scale * divisor;
            for input in 0..columns {
                let index = (input * rows + output) * bytes;
                let signed = if bits == 8 {
                    values[index] as i8 as f32
                } else {
                    i16::from_le_bytes([values[index], values[index + 1]]) as f32
                };
                decoded[output * columns + input] = signed * factor;
            }
        }
        Ok(Self {
            rows,
            columns,
            values: decoded.into(),
            quantization: Some(Quantization {
                bits,
                input_exponent,
            }),
        })
    }

    pub fn rows(&self) -> usize {
        self.rows
    }

    pub fn columns(&self) -> usize {
        self.columns
    }

    pub fn quantization(&self) -> Option<Quantization> {
        self.quantization
    }

    pub(super) fn row(&self, output: usize) -> &[f32] {
        &self.values[output * self.columns..(output + 1) * self.columns]
    }
}

pub(super) fn check_length(
    buffer: &'static str,
    expected: usize,
    actual: usize,
) -> Result<(), MatrixError> {
    if actual != expected {
        return Err(MatrixError::InvalidLength {
            buffer,
            expected,
            actual,
        });
    }
    Ok(())
}

fn elements(rows: usize, columns: usize) -> Result<usize, MatrixError> {
    if rows == 0 || columns == 0 {
        return Err(MatrixError::InvalidShape);
    }
    rows.checked_mul(columns).ok_or(MatrixError::InvalidShape)
}
