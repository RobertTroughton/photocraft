//! Photoshop Color Table files (`.act`).
//!
//! Layout from Adobe's public "Photoshop File Formats Specification" (Color Table): 256 RGB
//! triplets (768 bytes, unused entries zero). Files written for palettes with fewer entries or a
//! transparent entry carry a 4-byte trailer: a big-endian `u16` colour count and a big-endian
//! `u16` transparent index (`0xFFFF` for none).

use crate::error::{PsdError, Result};

/// Most entries in a colour table.
pub const MAX_COLORS: usize = 256;
const BODY: usize = MAX_COLORS * 3;
const NO_TRANSPARENCY: u16 = 0xFFFF;

/// A parsed `.act` file.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ActFile {
    /// Entries in file order (1..=256).
    pub colors: Vec<[u8; 3]>,
    /// Index of the transparent entry, if the trailer names one.
    pub transparent: Option<u8>,
}

/// Parse an `.act` file. Anything but 768 or 772 bytes, a zero count, or a count or transparent
/// index past the table is an error; never panics.
pub fn parse(data: &[u8]) -> Result<ActFile> {
    if data.len() != BODY && data.len() != BODY + 4 {
        return Err(PsdError::Invalid(format!("an .act file is {BODY} or {} bytes, not {}", BODY + 4, data.len())));
    }
    let (body, trailer) = data.split_at(BODY);
    let be = |i: usize| trailer.get(i..i + 2).map_or(NO_TRANSPARENCY, |b| u16::from_be_bytes([b[0], b[1]]));
    let count = if trailer.is_empty() { MAX_COLORS } else { usize::from(be(0)) };
    if count == 0 || count > MAX_COLORS {
        return Err(PsdError::Invalid(format!("an .act file holds 1 to {MAX_COLORS} colours, not {count}")));
    }
    let transparent = match be(2) {
        NO_TRANSPARENCY => None,
        i if usize::from(i) < count => Some(i as u8),
        // A stale index past the table (some writers leave one behind) means no transparency.
        _ => None,
    };
    let colors = body.as_chunks::<3>().0.iter().take(count).copied().collect();
    Ok(ActFile { colors, transparent })
}

/// Write an `.act` file. A full 256-entry table without transparency is the bare 768 bytes;
/// anything else gets the trailer, as Photoshop writes it.
pub fn write(colors: &[[u8; 3]], transparent: Option<u8>) -> Result<Vec<u8>> {
    if colors.is_empty() || colors.len() > MAX_COLORS {
        return Err(PsdError::LimitExceeded("an .act file holds 1 to 256 colours"));
    }
    let mut out = vec![0u8; BODY];
    for (dst, c) in out.as_chunks_mut::<3>().0.iter_mut().zip(colors) {
        *dst = *c;
    }
    let transparent = transparent.filter(|&i| usize::from(i) < colors.len());
    if colors.len() < MAX_COLORS || transparent.is_some() {
        out.extend_from_slice(&(colors.len() as u16).to_be_bytes());
        out.extend_from_slice(&transparent.map_or(NO_TRANSPARENCY, u16::from).to_be_bytes());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_with_and_without_the_trailer() {
        let full: Vec<[u8; 3]> = (0..=255u8).map(|i| [i, 255 - i, i / 2]).collect();
        let bytes = write(&full, None).unwrap();
        assert_eq!(bytes.len(), 768);
        assert_eq!(parse(&bytes).unwrap(), ActFile { colors: full.clone(), transparent: None });

        let short = &full[..17];
        let bytes = write(short, Some(3)).unwrap();
        assert_eq!(bytes.len(), 772);
        assert_eq!(&bytes[768..], &[0, 17, 0, 3]);
        assert_eq!(parse(&bytes).unwrap(), ActFile { colors: short.to_vec(), transparent: Some(3) });
        assert_eq!(write(&parse(&bytes).unwrap().colors, Some(3)).unwrap(), bytes, "byte-stable");

        assert_eq!(write(&full, Some(255)).unwrap().len(), 772);
    }

    #[test]
    fn out_of_range_transparency_is_dropped() {
        assert_eq!(write(&[[1, 2, 3]], Some(5)).unwrap().len(), 772);
        let mut b = write(&[[1, 2, 3]], None).unwrap();
        b[770..].copy_from_slice(&[0, 9]);
        assert_eq!(parse(&b).unwrap().transparent, None);
    }

    #[test]
    fn hostile_input_is_an_error_not_a_panic() {
        assert!(parse(&[]).is_err());
        assert!(parse(&[0; 767]).is_err());
        assert!(parse(&[0; 769]).is_err());
        let mut b = vec![0u8; 772];
        assert!(parse(&b).is_err(), "zero count");
        b[768..770].copy_from_slice(&[1, 1]);
        assert!(parse(&b).is_err(), "count past 256");
        assert!(write(&[], None).is_err());
        assert!(write(&[[0; 3]; 257], None).is_err());
    }
}
