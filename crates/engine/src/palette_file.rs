//! Load and Save for Image › Mode › Color Table and the Custom palette of Indexed Color.
//!
//! Photoshop colour tables are `.act` files; `.aco` and `.ase` swatch files load too (their
//! colours go through the swatch conversions to sRGB, the first 256 are kept) and a palette
//! saves to any of the three by file extension.

use serde_json::json;

use crate::presets::Group;
use crate::presets::swatches::{Swatch, SwatchColor, SwatchConv};
use crate::swatch_cmds::{self, Format};
use crate::{EngineError, Result, Session};

/// A palette read from a file.
pub struct Table {
    pub colors: Vec<[u8; 3]>,
    pub transparent: Option<u8>,
    /// Colours past the 256th that were left out.
    pub ignored: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TableFormat {
    Act,
    Swatches(Format),
}

fn extension_format(path: &str) -> Option<TableFormat> {
    let ext = std::path::Path::new(path).extension()?.to_str()?.to_ascii_lowercase();
    match ext.as_str() {
        "act" => Some(TableFormat::Act),
        "aco" => Some(TableFormat::Swatches(Format::Aco)),
        "ase" => Some(TableFormat::Swatches(Format::Ase)),
        _ => None,
    }
}

fn detect(bytes: &[u8], path: &str) -> Option<TableFormat> {
    if bytes.starts_with(photocraft_psd::ase::SIGNATURE) {
        return Some(TableFormat::Swatches(Format::Ase));
    }
    extension_format(path).or(match bytes.len() {
        768 | 772 => Some(TableFormat::Act),
        _ if matches!(bytes.get(..2), Some([0, 1] | [0, 2])) => Some(TableFormat::Swatches(Format::Aco)),
        _ => None,
    })
}

fn to8(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// Read the colour table in `path` (`.act`, `.aco` or `.ase`).
pub fn load(s: &Session, cmd: &str, path: &str) -> Result<Table> {
    let (bytes, _) = crate::preset_import_cmds::file_bytes(&json!({ "path": path }), cmd, ".act, .aco or .ase file")?;
    parse(s, cmd, path, &bytes)
}

/// Decode a colour table file; `name` (its path or file name) is the format hint.
pub fn parse(s: &Session, cmd: &str, name: &str, bytes: &[u8]) -> Result<Table> {
    let bad = |msg: String| EngineError::BadParams { cmd: cmd.into(), msg };
    let format = detect(bytes, name).ok_or_else(|| bad(format!("{name}: not an .act, .aco or .ase colour table")))?;
    match format {
        TableFormat::Act => {
            let f = photocraft_psd::act::parse(bytes).map_err(|e| bad(format!("{name}: not a readable .act file: {e}")))?;
            Ok(Table { colors: f.colors, transparent: f.transparent, ignored: 0 })
        }
        TableFormat::Swatches(fmt) => {
            let (groups, _) = swatch_cmds::read_file(bytes, fmt, "Palette").map_err(|e| bad(format!("{name}: {e}")))?;
            let conv = SwatchConv::new(s);
            let all: Vec<[u8; 3]> = groups.iter().flat_map(|g| g.items.iter()).map(|sw| conv.rgb(&sw.color).map(to8)).collect();
            let ignored = all.len().saturating_sub(photocraft_psd::act::MAX_COLORS);
            let colors = all.into_iter().take(photocraft_psd::act::MAX_COLORS).collect();
            Ok(Table { colors, transparent: None, ignored })
        }
    }
}

/// Encode a colour table for the file name `path`: the extension picks the format (`.act` when
/// it has none). Only `.act` keeps the transparent entry.
pub fn encode(cmd: &str, path: &str, colors: &[[u8; 3]], transparent: Option<u8>) -> Result<Vec<u8>> {
    let bad = |msg: String| EngineError::BadParams { cmd: cmd.into(), msg };
    let format = if std::path::Path::new(path).extension().is_none() {
        TableFormat::Act
    } else {
        extension_format(path).ok_or_else(|| bad(format!("{path}: palettes save as .act, .aco or .ase")))?
    };
    match format {
        TableFormat::Act => photocraft_psd::act::write(colors, transparent).map_err(|e| bad(e.to_string())),
        TableFormat::Swatches(fmt) => {
            let items =
                colors.iter().enumerate().map(|(i, c)| Swatch::new(&format!("Color {}", i + 1), SwatchColor::Rgb(c.map(|v| f32::from(v) / 255.0)))).collect();
            let (bytes, ..) = swatch_cmds::write_file(&[Group::new("Palette", items)], fmt).map_err(bad)?;
            Ok(bytes)
        }
    }
}

/// Write a colour table to `path` (see [`encode`]).
pub fn save(cmd: &str, path: &str, colors: &[[u8; 3]], transparent: Option<u8>) -> Result<()> {
    crate::file_cmds::write_file(path, &encode(cmd, path, colors, transparent)?)
}
