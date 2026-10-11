//! Frame-boundary seed events for acoustic noise streams.
//!
//! Equal adjacent seeds are distinct reset events. A zero-length segment also
//! remains an event: the source advances at most one segment per output frame.

use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SeedError {
    LengthMismatch { seeds: usize, lengths: usize },
    MaskLengthMismatch { seeds: usize, mask: usize },
    FrameCountOverflow,
    RadiusOverflow,
}

impl fmt::Display for SeedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LengthMismatch { seeds, lengths } => {
                write!(
                    f,
                    "Seed event counts differ: {seeds} seeds, {lengths} lengths"
                )
            }
            Self::MaskLengthMismatch { seeds, mask } => {
                write!(
                    f,
                    "Seed spread counts differ: {seeds} seeds, {mask} mask entries"
                )
            }
            Self::FrameCountOverflow => {
                f.write_str("Seed timeline exceeds signed 32-bit frame range")
            }
            Self::RadiusOverflow => f.write_str("Seed spread radius exceeds signed 32-bit range"),
        }
    }
}

impl std::error::Error for SeedError {}

/// State can be cloned with a stage checkpoint; restarting only the RNG is not
/// enough to resume the same noise sequence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SeedSource {
    seeds: Vec<u32>,
    boundaries: Vec<i32>,
    next: usize,
    frame: i32,
}

impl SeedSource {
    /// Boundaries contain segment starts, not ends.
    pub fn new(seeds: &[u32], lengths: &[usize]) -> Result<Self, SeedError> {
        validate_lengths(seeds.len(), lengths)?;
        let mut accumulated = 0;
        let boundaries = lengths
            .iter()
            .map(|&length| {
                let start = accumulated;
                accumulated += length as i32;
                start
            })
            .collect();
        Ok(Self {
            seeds: seeds.to_vec(),
            boundaries,
            next: 0,
            frame: -1,
        })
    }

    /// Hashes the concatenation
    /// `prefix + previous + "_" + current + "_" + next + "_" + parameter`.
    /// The parameter uses signed decimal.
    pub fn context<S: AsRef<str>>(
        strings: &[S],
        lengths: &[usize],
        prefix: &str,
        parameter: i32,
    ) -> Result<Self, SeedError> {
        validate_lengths(strings.len(), lengths)?;
        let suffix = parameter.to_string();
        let seeds = strings
            .iter()
            .enumerate()
            .map(|(index, current)| {
                let previous = index.checked_sub(1).map_or("", |i| strings[i].as_ref());
                let next = strings.get(index + 1).map_or("", AsRef::as_ref);
                context_hash(
                    format!("{prefix}{previous}_{}_{next}_{suffix}", current.as_ref()).as_bytes(),
                )
            })
            .collect::<Vec<_>>();
        Self::new(&seeds, lengths)
    }

    /// Call exactly once for every output frame, including frames without a
    /// reset. `Some(0)` is an event, and must not be treated as `None`.
    pub fn next_event(&mut self) -> Option<u32> {
        // The signed counter starts at -1 so the first call visits frame zero.
        self.frame = self.frame.wrapping_add(1);
        if self.next >= self.boundaries.len() || self.frame < self.boundaries[self.next] {
            return None;
        }
        let seed = self.seeds[self.next];
        self.next += 1;
        Some(seed)
    }
}

/// FNV-1a with signed byte extension. A zero hash is replaced by the hash of " ".
pub fn context_hash(bytes: &[u8]) -> u32 {
    fn hash(bytes: &[u8]) -> u32 {
        bytes.iter().fold(0x811c_9dc5_u32, |state, &byte| {
            (state ^ (byte as i8 as i32 as u32)).wrapping_mul(0x0100_0193)
        })
    }
    let value = hash(bytes);
    if value == 0 { hash(b" ") } else { value }
}

/// Redistribute masked segment frames to neighbouring unmasked seed events.
///
/// Keeps a separate reset at the unmasked segment's initial start after its
/// newly borrowed prefix. It does not merge equal seeds or change the total
/// number of frames.
pub fn spread(
    seeds: &[u32],
    lengths: &[usize],
    mask: &[bool],
    radius: usize,
) -> Result<(Vec<u32>, Vec<usize>), SeedError> {
    validate_lengths(seeds.len(), lengths)?;
    if seeds.len() != mask.len() {
        return Err(SeedError::MaskLengthMismatch {
            seeds: seeds.len(),
            mask: mask.len(),
        });
    }
    if radius > i32::MAX as usize {
        return Err(SeedError::RadiusOverflow);
    }
    let mut remaining = lengths.to_vec();
    let mut prefixes = vec![0; seeds.len()];
    let mut suffixes = vec![0; seeds.len()];
    let mut start = 0;
    while start < mask.len() {
        if !mask[start] {
            start += 1;
            continue;
        }
        let mut end = start + 1;
        while end < mask.len() && mask[end] {
            end += 1;
        }
        let total: usize = lengths[start..end].iter().sum();
        let (left, right) = match (start > 0, end < mask.len()) {
            // Give an odd leftover frame to the left neighbour.
            (true, true) if total < radius * 2 => (total.div_ceil(2), total / 2),
            (true, true) => (radius, radius),
            (true, false) => (total.min(radius), 0),
            (false, true) => (0, total.min(radius)),
            (false, false) => (0, 0),
        };
        if start > 0 {
            suffixes[start - 1] = left;
        }
        if end < mask.len() {
            prefixes[end] = right;
        }
        remove_frames(remaining[start..end].iter_mut(), left);
        remove_frames(remaining[start..end].iter_mut().rev(), right);
        start = end;
    }
    let mut output_seeds = Vec::with_capacity(seeds.len());
    let mut output_lengths = Vec::with_capacity(seeds.len());
    let mut append = |seed, length| {
        output_seeds.push(seed);
        output_lengths.push(length);
    };
    for index in 0..seeds.len() {
        if mask[index] {
            if remaining[index] > 0 {
                append(seeds[index], remaining[index]);
            }
        } else {
            if prefixes[index] > 0 {
                append(seeds[index], prefixes[index]);
            }
            append(seeds[index], lengths[index] + suffixes[index]);
        }
    }
    Ok((output_seeds, output_lengths))
}

fn remove_frames<'a>(lengths: impl Iterator<Item = &'a mut usize>, mut count: usize) {
    for length in lengths {
        let taken = count.min(*length);
        *length -= taken;
        count -= taken;
        if count == 0 {
            break;
        }
    }
}

fn validate_lengths(count: usize, lengths: &[usize]) -> Result<(), SeedError> {
    if count != lengths.len() {
        return Err(SeedError::LengthMismatch {
            seeds: count,
            lengths: lengths.len(),
        });
    }
    if count > i32::MAX as usize
        || lengths
            .iter()
            .try_fold(0_usize, |total, &length| total.checked_add(length))
            .is_none_or(|total| total > i32::MAX as usize)
    {
        return Err(SeedError::FrameCountOverflow);
    }
    Ok(())
}
