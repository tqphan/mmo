//! One region file (`.mmor`, docs/design/world.md): a square of whole cells with its heights and
//! land-use raster, each compressed with zlib. Heights are stored as differences from a planar
//! prediction, so smooth ground compresses to a few bits a sample.

use std::io::{Read, Write};

use flate2::Compression;
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use sim::cell::CELLS_PER_SIDE;
use sim::terrain::QUADS_PER_CHUNK;

use crate::TEXELS_PER_CELL;

const MAGIC: &[u8; 4] = b"MMOR";
const VERSION: u16 = 1;
/// Bit 7 of a land-use texel is reserved (docs/design/world.md).
const RESERVED_BIT: u8 = 0x80;

#[derive(Clone, Debug, PartialEq)]
pub struct Region {
    pub cell_x: u16,
    pub cell_z: u16,
    /// Cells along a side.
    pub cells: u16,
    /// `side()²` samples, rows of increasing z, edges included.
    pub heights: Vec<u16>,
    /// `texels()²` land-use texels, rows of increasing z.
    pub land: Vec<u8>,
}

impl Region {
    /// Height samples along a side.
    pub fn side(&self) -> usize {
        self.cells as usize * QUADS_PER_CHUNK + 1
    }

    /// Land-use texels along a side.
    pub fn texels(&self) -> usize {
        self.cells as usize * TEXELS_PER_CELL
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = MAGIC.to_vec();
        for v in [VERSION, self.cell_x, self.cell_z, self.cells] {
            out.extend(v.to_le_bytes());
        }
        for stream in [planes(&residuals(&self.heights, self.side())), self.land.clone()] {
            let mut z = ZlibEncoder::new(Vec::new(), Compression::best());
            z.write_all(&stream).expect("writing to a Vec");
            let z = z.finish().expect("writing to a Vec");
            out.extend((z.len() as u32).to_le_bytes());
            out.extend(z);
        }
        out
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() < 12 || &bytes[..4] != MAGIC {
            return Err("not a .mmor region file".into());
        }
        let word = |i: usize| u16::from_le_bytes([bytes[i], bytes[i + 1]]);
        if word(4) != VERSION {
            return Err(format!("region file version {}, expected {VERSION}", word(4)));
        }
        let mut r = Self { cell_x: word(6), cell_z: word(8), cells: word(10), heights: Vec::new(), land: Vec::new() };
        let off = |first: u16| first as u32 + r.cells as u32 > CELLS_PER_SIDE as u32;
        if r.cells == 0 || off(r.cell_x) || off(r.cell_z) {
            return Err("region is empty or off the map".into());
        }
        let mut at = 12;
        let mut stream = |want: usize, what: &str| -> Result<Vec<u8>, String> {
            let len = bytes.get(at..at + 4).ok_or(format!("{what}: truncated"))?;
            let len = u32::from_le_bytes(len.try_into().unwrap()) as usize;
            let z = bytes.get(at + 4..at + 4 + len).ok_or(format!("{what}: truncated"))?;
            at += 4 + len;
            let mut out = Vec::with_capacity(want);
            // One byte more than expected shows a stream that's too long without reading all of it.
            ZlibDecoder::new(z).take(want as u64 + 1).read_to_end(&mut out).map_err(|e| format!("{what}: {e}"))?;
            if out.len() != want {
                return Err(format!("{what}: {} bytes, expected {want}", out.len()));
            }
            Ok(out)
        };
        let (side, texels) = (r.side(), r.texels());
        let h = stream(2 * side * side, "heights")?;
        r.land = stream(texels * texels, "land use")?;
        if at != bytes.len() {
            return Err(format!("{} bytes after the land use", bytes.len() - at));
        }
        if r.land.iter().any(|&t| t & RESERVED_BIT != 0) {
            return Err("land use has the reserved bit set".into());
        }
        let n = side * side;
        let zigzag: Vec<u16> = (0..n).map(|i| u16::from_le_bytes([h[i], h[n + i]])).collect();
        r.heights = unpredict(&zigzag, side);
        Ok(r)
    }
}

/// The prediction for sample `(i, j)` from the ones before it: `left + above − above-left`, only
/// `left` on the first row, only `above` in the first column, 0 for the first sample. Modulo 2¹⁶.
fn predict(s: &[u16], side: usize, i: usize, j: usize) -> u16 {
    let at = |i: usize, j: usize| s[j * side + i];
    match (i, j) {
        (0, 0) => 0,
        (_, 0) => at(i - 1, 0),
        (0, _) => at(0, j - 1),
        _ => at(i - 1, j).wrapping_add(at(i, j - 1)).wrapping_sub(at(i - 1, j - 1)),
    }
}

/// Zig-zag coded differences from the prediction.
fn residuals(s: &[u16], side: usize) -> Vec<u16> {
    let mut out = Vec::with_capacity(s.len());
    for j in 0..side {
        for i in 0..side {
            let d = s[j * side + i].wrapping_sub(predict(s, side, i, j)) as i16;
            out.push(((d << 1) ^ (d >> 15)) as u16);
        }
    }
    out
}

fn unpredict(zigzag: &[u16], side: usize) -> Vec<u16> {
    let mut s = vec![0u16; zigzag.len()];
    for j in 0..side {
        for i in 0..side {
            let z = zigzag[j * side + i];
            let d = ((z >> 1) as i16) ^ -((z & 1) as i16);
            s[j * side + i] = predict(&s, side, i, j).wrapping_add(d as u16);
        }
    }
    s
}

/// All the low bytes, then all the high bytes: each plane compresses better on its own.
fn planes(v: &[u16]) -> Vec<u8> {
    v.iter().map(|x| *x as u8).chain(v.iter().map(|x| (x >> 8) as u8)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn region(height: impl Fn(usize, usize) -> u16) -> Region {
        let mut r = Region { cell_x: 20, cell_z: 30, cells: 2, heights: Vec::new(), land: Vec::new() };
        let (side, texels) = (r.side(), r.texels());
        r.heights = (0..side * side).map(|k| height(k % side, k / side)).collect();
        r.land = (0..texels * texels).map(|k| (((k / 7) % 4) | (((k / 300) % 4) << 4)) as u8).collect();
        r
    }

    #[test]
    fn regions_round_trip_and_compress() {
        let smooth = region(|i, j| (16384.0 + 300.0 * ((i as f32) * 0.05).sin() + 2.0 * j as f32) as u16);
        let bytes = smooth.to_bytes();
        assert_eq!(Region::from_bytes(&bytes).unwrap(), smooth);
        assert!(bytes.len() < smooth.heights.len() / 2, "{} bytes", bytes.len());
    }

    #[test]
    fn extreme_heights_round_trip() {
        // Jumps between 0 and 65535 wrap in the prediction.
        let jagged = region(|i, j| if (i * 7 + j * 13) % 3 == 0 { u16::MAX } else { (i * j) as u16 });
        assert_eq!(Region::from_bytes(&jagged.to_bytes()).unwrap(), jagged);
        let flat = region(|_, _| 0);
        assert_eq!(Region::from_bytes(&flat.to_bytes()).unwrap(), flat);
    }

    #[test]
    fn bad_region_files_are_refused() {
        let good = region(|i, _| i as u16).to_bytes();
        let err = |b: &[u8]| Region::from_bytes(b).unwrap_err();
        assert!(err(b"MMOR").contains("not a"));
        let mut magic = good.clone();
        magic[3] = b'H';
        assert!(err(&magic).contains("not a"));
        let mut version = good.clone();
        version[4] = 2;
        assert!(err(&version).contains("version 2"));
        let mut off_map = good.clone();
        off_map[6..8].copy_from_slice(&249u16.to_le_bytes());
        assert!(err(&off_map).contains("off the map"));
        assert!(err(&good[..good.len() - 3]).contains("land use"));
        assert!(err(&good[..20]).contains("heights"));
        let mut extra = good.clone();
        extra.push(0);
        assert!(err(&extra).contains("after the land use"));
        // A region that claims to be bigger than its streams.
        let mut bigger = good.clone();
        bigger[10..12].copy_from_slice(&3u16.to_le_bytes());
        assert!(err(&bigger).contains("expected"));
    }

    #[test]
    fn the_reserved_land_bit_is_refused() {
        let mut r = region(|_, _| 1);
        r.land[5] |= RESERVED_BIT;
        assert!(Region::from_bytes(&r.to_bytes()).unwrap_err().contains("reserved"));
    }
}
