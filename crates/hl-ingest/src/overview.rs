//! Map overview images under the kill map.
//!
//! The app ships more.tf's renders of the Highlander pool (`overviews/` in
//! the repository, used with more.tf's permission), named by map base
//! (`upward.png` for `pl_upward_f12`). An image in `<app data>/overviews/`
//! of the same name wins, so a player can put a better one in. A map with
//! no image keeps the outline drawn from kills.
//!
//! The Maps section in Settings (Q31) lets a player put their own image in
//! for any map, and line it up over the kills where the table below has no
//! placement for it; both are kept beside the built-in ones and win.
//!
//! **Placement.** Each built-in image is square and covers `1024 × scale` game units,
//! centred on `(x + 910·scale, y − 512·scale)`: the transform more.tf uses for
//! the same images, per map. Checked on this account by projecting stored
//! kill positions onto the images: 94-99% land on the drawn map, and firing
//! positions sit on balconies and cliff edges where Snipers stand.

use anyhow::{Context, Result};
use base64::Engine;
use hl_demos::map_base;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// `(map base, scale, x, y)`, from more.tf's map table.
const PLACEMENT: &[(&str, f64, f64, f64)] = &[
    ("gullywash", 9.3, -8464.0, 4761.0),
    ("snakewater", 11.0, -9484.0, 5840.0),
    ("sultry", 10.5, -9589.0, 5360.0),
    ("process", 10.0, -9102.0, 5120.0),
    ("sunshine", 9.0, -13824.0, 9855.0),
    ("metalworks", 11.0, -9852.0, 4760.0),
    ("villa", 10.5, -9557.0, 5376.0),
    ("proworks", 11.0, -9852.0, 4760.0),
    ("bagel", 9.0, -8192.0, 4608.0),
    ("ashville", 8.0, -7322.0, 4101.0),
    ("product", 7.0, -7907.0, 3584.0),
    ("proplant", 9.0, -8192.0, 4608.0),
    ("proot", 7.75, -7054.0, 3968.0),
    ("cascade", 8.25, -7512.0, 4226.0),
    ("swiftwater", 8.0, -4381.0, 2726.0),
    ("vigil", 7.5, -5802.0, 4940.0),
    ("upward", 5.5, -4956.0, 2216.0),
    ("steel", 8.0, -6740.0, 3196.0),
];

/// The built-in images, by map base: more.tf's renders, with permission.
const BUILT_IN: &[(&str, &[u8])] = &[
    ("ashville", include_bytes!("../../../overviews/ashville.png")),
    ("bagel", include_bytes!("../../../overviews/bagel.png")),
    ("cascade", include_bytes!("../../../overviews/cascade.png")),
    ("gullywash", include_bytes!("../../../overviews/gullywash.png")),
    ("process", include_bytes!("../../../overviews/process.png")),
    ("product", include_bytes!("../../../overviews/product.png")),
    ("proot", include_bytes!("../../../overviews/proot.png")),
    ("proplant", include_bytes!("../../../overviews/proplant.png")),
    ("steel", include_bytes!("../../../overviews/steel.png")),
    ("swiftwater", include_bytes!("../../../overviews/swiftwater.png")),
    ("upward", include_bytes!("../../../overviews/upward.png")),
    ("vigil", include_bytes!("../../../overviews/vigil.png")),
];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Overview {
    pub map_base: String,
    /// Game units at the image's left edge and top edge (game y points up).
    pub min_x: f64,
    pub max_y: f64,
    /// Game units the image spans across.
    pub size: f64,
    /// Height over width: 1 for the built-in renders, whatever an imported
    /// image is otherwise. The image spans `size × aspect` units down.
    pub aspect: f64,
    /// The image itself, as a data URL the page can draw.
    pub image: String,
}

/// Where an image sits, in game units, as the player lines it up.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Placement {
    pub min_x: f64,
    pub max_y: f64,
    pub size: f64,
}

/// Image files a player may put in: the types a webview draws.
const EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "webp"];
/// Larger than any render seen, small enough that a wrong file is refused.
const MAX_BYTES: usize = 25 * 1024 * 1024;

/// Where a map's image sits in game units, from more.tf's table:
/// `(min_x, max_y, size)`.
pub fn placement(map: &str) -> Option<(f64, f64, f64)> {
    let base = map_base(map);
    let &(_, scale, x, y) = PLACEMENT.iter().find(|p| p.0 == base)?;
    let size = 1024.0 * scale;
    let (cx, cy) = (x + 910.0 * scale, y - 512.0 * scale);
    Some((cx - size / 2.0, cy + size / 2.0, size))
}

fn placement_path(dir: &Path, base: &str) -> PathBuf {
    dir.join(format!("{base}.placement.json"))
}

/// The player's own image file for a base, if there is one.
fn user_image(dir: &Path, base: &str) -> Option<PathBuf> {
    EXTENSIONS.iter().map(|e| dir.join(format!("{base}.{e}"))).find(|p| p.exists())
}

/// A placement the player saved, else the built-in one.
pub fn placement_for(dir: &Path, base: &str) -> Option<Placement> {
    if let Some(p) = std::fs::read_to_string(placement_path(dir, base)).ok().and_then(|t| serde_json::from_str(&t).ok()) {
        return Some(p);
    }
    placement(base).map(|(min_x, max_y, size)| Placement { min_x, max_y, size })
}

/// `(mime, width, height)` from an image's first bytes: PNG, JPEG or WebP.
/// None for anything else, which is how a wrong file is refused.
pub fn image_info(b: &[u8]) -> Option<(&'static str, u32, u32)> {
    let be32 = |i: usize| b.get(i..i + 4).map(|s| u32::from_be_bytes([s[0], s[1], s[2], s[3]]));
    let be16 = |i: usize| b.get(i..i + 2).map(|s| u32::from(u16::from_be_bytes([s[0], s[1]])));
    let le16 = |i: usize| b.get(i..i + 2).map(|s| u32::from(u16::from_le_bytes([s[0], s[1]])));
    let le24 = |i: usize| b.get(i..i + 3).map(|s| u32::from(s[0]) | u32::from(s[1]) << 8 | u32::from(s[2]) << 16);
    if b.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some(("image/png", be32(16)?, be32(20)?));
    }
    if b.starts_with(&[0xFF, 0xD8]) {
        // Walk the segments to a start-of-frame marker, which holds the size.
        let mut i = 2;
        while i + 9 < b.len() {
            if b[i] != 0xFF {
                return None;
            }
            let marker = b[i + 1];
            let len = be16(i + 2)? as usize;
            if (0xC0..=0xCF).contains(&marker) && ![0xC4, 0xC8, 0xCC].contains(&marker) {
                return Some(("image/jpeg", be16(i + 7)?, be16(i + 5)?));
            }
            i += 2 + len;
        }
        return None;
    }
    if b.get(0..4) == Some(b"RIFF") && b.get(8..12) == Some(b"WEBP") {
        return match b.get(12..16)? {
            b"VP8 " => Some(("image/webp", le16(26)? & 0x3FFF, le16(28)? & 0x3FFF)),
            b"VP8L" => {
                let v = u32::from_le_bytes([*b.get(21)?, *b.get(22)?, *b.get(23)?, *b.get(24)?]);
                Some(("image/webp", (v & 0x3FFF) + 1, ((v >> 14) & 0x3FFF) + 1))
            }
            b"VP8X" => Some(("image/webp", le24(24)? + 1, le24(27)? + 1)),
            _ => None,
        };
    }
    None
}

/// A map's image as a data URL with its aspect (height over width): the
/// player's own first, then the built-in one. Whether or not it has a
/// placement, so the aligner can show it.
pub fn image(dir: &Path, map: &str) -> Result<Option<(String, f64)>> {
    let base = map_base(map);
    let bytes = match user_image(dir, &base) {
        Some(path) => std::fs::read(&path).with_context(|| format!("reading {}", path.display()))?,
        None => match BUILT_IN.iter().find(|i| i.0 == base) {
            Some(&(_, b)) => b.to_vec(),
            None => return Ok(None),
        },
    };
    let Some((mime, w, h)) = image_info(&bytes) else { return Ok(None) };
    let aspect = if w == 0 { 1.0 } else { f64::from(h) / f64::from(w) };
    Ok(Some((format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)), aspect)))
}

/// The overview for a map, when both its placement and its image are known:
/// the player's own image and placement in `dir` first, then the built-in.
pub fn load(dir: &Path, map: &str) -> Result<Option<Overview>> {
    let base = map_base(map);
    let Some(Placement { min_x, max_y, size }) = placement_for(dir, &base) else { return Ok(None) };
    let Some((image, aspect)) = image(dir, &base)? else { return Ok(None) };
    Ok(Some(Overview { map_base: base, min_x, max_y, size, aspect, image }))
}

/// Where a map's image and placement come from: "yours", "built in" or
/// "none". What the Maps section in Settings lists.
pub fn origins(dir: &Path, base: &str) -> (&'static str, &'static str) {
    let built_in = BUILT_IN.iter().find(|i| i.0 == base).map(|i| i.1);
    // A copy of the built-in image, byte for byte, is the built-in image:
    // installs from before the images shipped kept them in this folder.
    let own = user_image(dir, base).filter(|p| built_in.is_none_or(|b| std::fs::read(p).map_or(true, |mine| mine != b)));
    let image = if own.is_some() {
        "yours"
    } else if built_in.is_some() {
        "built in"
    } else {
        "none"
    };
    let placed = if placement_path(dir, base).exists() {
        "yours"
    } else if placement(base).is_some() {
        "built in"
    } else {
        "none"
    };
    (image, placed)
}

/// Every base with a built-in image or placement.
pub fn built_in_bases() -> impl Iterator<Item = &'static str> {
    BUILT_IN.iter().map(|i| i.0).chain(PLACEMENT.iter().map(|p| p.0))
}

/// Bases the player has put an image or a placement in for.
pub fn user_bases(dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    entries
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            let base = name.strip_suffix(".placement.json").map(str::to_string).or_else(|| {
                let (stem, ext) = name.rsplit_once('.')?;
                EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str()).then(|| stem.to_string())
            })?;
            Some(base)
        })
        .collect()
}

/// Put a player's image in for a map, replacing any image of theirs there
/// was. Refuses anything that is not a PNG, JPEG or WebP, or is too large.
pub fn import(dir: &Path, map: &str, from: &Path) -> Result<()> {
    let base = map_base(map);
    anyhow::ensure!(!base.is_empty() && base.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'), "`{map}` is not a map");
    let bytes = std::fs::read(from).with_context(|| format!("reading {}", from.display()))?;
    anyhow::ensure!(bytes.len() <= MAX_BYTES, "{} is larger than 25 MB", from.display());
    let (mime, w, h) = image_info(&bytes).with_context(|| format!("{} is not a PNG, JPEG or WebP image", from.display()))?;
    anyhow::ensure!(w >= 64 && h >= 64, "{} is only {w}×{h}", from.display());
    let ext = match mime {
        "image/png" => "png",
        "image/jpeg" => "jpg",
        _ => "webp",
    };
    std::fs::create_dir_all(dir)?;
    // One image a map: a new one replaces the old whatever its type.
    for e in EXTENSIONS {
        let old = dir.join(format!("{base}.{e}"));
        if old.exists() {
            std::fs::remove_file(&old)?;
        }
    }
    let path = dir.join(format!("{base}.{ext}"));
    let tmp = dir.join(format!("{base}.{ext}.tmp"));
    std::fs::write(&tmp, &bytes)?;
    std::fs::rename(&tmp, &path)?;
    Ok(())
}

/// Save where the player lined the image up.
pub fn save_placement(dir: &Path, map: &str, p: Placement) -> Result<()> {
    anyhow::ensure!(p.min_x.is_finite() && p.max_y.is_finite() && p.size.is_finite() && p.size > 0.0, "not a placement");
    std::fs::create_dir_all(dir)?;
    std::fs::write(placement_path(dir, &map_base(map)), serde_json::to_string_pretty(&p)?)?;
    Ok(())
}

/// Back to the built-in image and placement: the player's own files go.
pub fn remove(dir: &Path, map: &str) -> Result<()> {
    let base = map_base(map);
    for path in EXTENSIONS.iter().map(|e| dir.join(format!("{base}.{e}"))).chain([placement_path(dir, &base)]) {
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A position more.tf would draw at (x%, y%) of the image lands at the
    /// same fraction of our placement.
    #[test]
    fn placement_matches_moretf_projection() {
        let (s, x, y) = (5.5, -4956.0, 2216.0);
        let moretf = |gx: f64, gy: f64| {
            (
                (gx - (x + 910.0 * s)) / s * 0.097656 + 50.0,
                (gy - (y - 512.0 * s)) / s * -0.097656 + 50.0,
            )
        };
        let (min_x, max_y, size) = placement("pl_upward_f12").unwrap();
        for (gx, gy) in [(0.0, 0.0), (1234.0, -2345.0), (-3000.0, 1500.0)] {
            let (mx, my) = moretf(gx, gy);
            let (ox, oy) = ((gx - min_x) / size * 100.0, (max_y - gy) / size * 100.0);
            assert!((mx - ox).abs() < 0.01 && (my - oy).abs() < 0.01, "{gx},{gy}: {mx},{my} vs {ox},{oy}");
        }
    }

    #[test]
    fn built_in_images_are_used_and_every_one_has_a_placement() {
        let empty = std::env::temp_dir().join(format!("hl-overviews-none-{}", std::process::id()));
        let o = load(&empty, "pl_vigil_rc10").unwrap().expect("vigil ships an image");
        assert!(o.image.starts_with("data:image/png;base64,iVBORw0KGgo"), "a PNG");
        for (base, bytes) in BUILT_IN {
            assert!(placement(base).is_some(), "{base} has an image but no placement");
            assert_eq!(&bytes[..4], [0x89, b'P', b'N', b'G'], "{base}");
        }
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("hl-overview-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A PNG header of the given size: enough for `image_info`.
    fn png(w: u32, h: u32) -> Vec<u8> {
        let mut b = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        b.extend(w.to_be_bytes());
        b.extend(h.to_be_bytes());
        b.extend([8, 6, 0, 0, 0]);
        b
    }

    #[test]
    fn image_sizes_are_read_from_the_header() {
        assert_eq!(image_info(&png(1024, 768)), Some(("image/png", 1024, 768)));
        let (mime, w, h) = image_info(BUILT_IN[0].1).unwrap();
        assert_eq!((mime, w, h > 0), ("image/png", w, true));
        // A minimal JPEG: SOI, an APP0 segment, then SOF0 with 600×400.
        let jpeg = [
            0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x04, 0x00, 0x00, 0xFF, 0xC0, 0x00, 0x11, 0x08, 0x01, 0x90, 0x02, 0x58, 0x03, 0, 0, 0, 0, 0, 0,
        ];
        assert_eq!(image_info(&jpeg), Some(("image/jpeg", 600, 400)));
        assert_eq!(image_info(b"not an image at all"), None);
    }

    #[test]
    fn a_players_image_and_placement_win_and_remove_goes_back() {
        let dir = scratch("import");
        let file = dir.join("mine.png");
        std::fs::write(&file, png(2048, 1024)).unwrap();
        import(&dir.join("overviews"), "pl_vigil_rc10", &file).unwrap();
        let o = dir.join("overviews");
        assert_eq!(origins(&o, "vigil"), ("yours", "built in"));
        let v = load(&o, "pl_vigil_rc10").unwrap().unwrap();
        assert!((v.aspect - 0.5).abs() < 1e-9, "a wide image keeps its shape");

        let p = Placement { min_x: -100.0, max_y: 200.0, size: 5000.0 };
        save_placement(&o, "vigil", p).unwrap();
        assert_eq!(placement_for(&o, "vigil"), Some(p));
        let mut bases = user_bases(&o);
        bases.sort();
        bases.dedup();
        assert_eq!(bases, ["vigil"]);

        remove(&o, "vigil").unwrap();
        assert_eq!(origins(&o, "vigil"), ("built in", "built in"));
        assert!(load(&o, "pl_vigil_rc10").unwrap().unwrap().image.starts_with("data:image/png"));
    }

    #[test]
    fn a_copy_of_the_built_in_image_counts_as_built_in() {
        let dir = scratch("copy");
        std::fs::write(dir.join("vigil.png"), BUILT_IN.iter().find(|i| i.0 == "vigil").unwrap().1).unwrap();
        assert_eq!(origins(&dir, "vigil").0, "built in");
        std::fs::write(dir.join("vigil.png"), png(512, 512)).unwrap();
        assert_eq!(origins(&dir, "vigil").0, "yours");
    }

    #[test]
    fn a_file_that_is_not_an_image_is_refused() {
        let dir = scratch("refuse");
        let file = dir.join("notes.png");
        std::fs::write(&file, b"these are my callouts").unwrap();
        assert!(import(&dir, "vigil", &file).is_err());
        assert_eq!(origins(&dir, "vigil").0, "built in");
    }

    #[test]
    fn a_map_with_an_image_but_no_placement_has_no_overview_until_lined_up() {
        let dir = scratch("unplaced");
        let file = dir.join("lake.png");
        std::fs::write(&file, png(1024, 1024)).unwrap();
        import(&dir, "koth_lakeside_final", &file).unwrap();
        assert_eq!(origins(&dir, "lakeside"), ("yours", "none"));
        assert!(load(&dir, "koth_lakeside_final").unwrap().is_none());
        assert!(image(&dir, "koth_lakeside_final").unwrap().is_some(), "the aligner can still show it");
        save_placement(&dir, "lakeside", Placement { min_x: 0.0, max_y: 0.0, size: 4000.0 }).unwrap();
        assert!(load(&dir, "koth_lakeside_final").unwrap().is_some());
    }

    #[test]
    fn versions_share_an_image_and_unknown_maps_have_none() {
        assert_eq!(placement("koth_proot_b5b"), placement("koth_proot_final"));
        assert!(placement("koth_lakeside_final").is_none());
    }
}
