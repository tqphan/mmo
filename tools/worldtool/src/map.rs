//! `worldtool map`: a top-down picture of part of the world as a PNG, for looking at a layout
//! without the client (docs/design/coast.md). Relief is shaded from the north-west, the sea is
//! blue by depth, towns, housing and roads are tinted, and placements are marked: nodes as small
//! dots, stations orange, posts yellow, Wayshrines white, spawn points red, the start cyan. Grid
//! lines every 100 m, darker every 500 m.

use std::io::Write;
use std::path::Path;

use sim::cell::WORLD_HALF_M;
use sim::content::LandUse;
use sim::terrain::{SPACING_M, decode};

use crate::{World, texel_use};

/// The area drawn, in world metres (north is −z, at the top).
#[derive(Clone, Copy, Debug)]
pub struct Area {
    pub x0: f32,
    pub z0: f32,
    pub x1: f32,
    pub z1: f32,
}

impl Area {
    pub fn parse(s: &str) -> Result<Self, String> {
        let v: Vec<f32> = s.split(',').map(|p| p.trim().parse::<f32>()).collect::<Result<_, _>>().map_err(|e| format!("--area {s}: {e}"))?;
        match v[..] {
            [x0, z0, x1, z1] if x1 > x0 && z1 > z0 => Ok(Self { x0, z0, x1, z1 }),
            _ => Err(format!("--area {s}: want x0,z0,x1,z1 with x1 > x0 and z1 > z0")),
        }
    }
}

/// A marker: world position, colour and half-size in pixels.
pub struct Mark {
    pub x: f32,
    pub z: f32,
    pub rgb: [u8; 3],
    pub r: i32,
}

/// An RGB picture of `area`, `px` pixels wide.
pub struct Picture {
    pub w: usize,
    pub h: usize,
    pub rgb: Vec<u8>,
}

fn height_at(world: &World, x: f32, z: f32) -> f32 {
    let (w, h) = world.heights.size();
    let i = (((x + WORLD_HALF_M) / SPACING_M).round().max(0.0) as usize).min(w - 1);
    let j = (((z + WORLD_HALF_M) / SPACING_M).round().max(0.0) as usize).min(h - 1);
    decode(world.heights.samples[j * w + i])
}

fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

/// The ground's colour by height, before shading.
fn ground_colour(h: f32) -> [f32; 3] {
    if h <= 0.0 {
        return mix([110.0, 170.0, 200.0], [20.0, 50.0, 100.0], (-h / 40.0).min(1.0));
    }
    let stops: [(f32, [f32; 3]); 5] =
        [(0.0, [225.0, 210.0, 160.0]), (3.0, [140.0, 175.0, 95.0]), (40.0, [95.0, 140.0, 70.0]), (120.0, [140.0, 120.0, 90.0]), (300.0, [200.0, 200.0, 200.0])];
    for p in stops.windows(2) {
        if h < p[1].0 {
            return mix(p[0].1, p[1].1, (h - p[0].0) / (p[1].0 - p[0].0));
        }
    }
    stops[4].1
}

pub fn render(world: &World, area: Area, px: usize, marks: &[Mark]) -> Picture {
    let m_per_px = (area.x1 - area.x0) / px as f32;
    let (w, h) = (px, ((area.z1 - area.z0) / m_per_px).round() as usize);
    let mut rgb = vec![0u8; w * h * 3];
    for j in 0..h {
        for i in 0..w {
            let (x, z) = (area.x0 + (i as f32 + 0.5) * m_per_px, area.z0 + (j as f32 + 0.5) * m_per_px);
            let ht = height_at(world, x, z);
            let mut c = ground_colour(ht);
            if ht > 0.0 {
                // Light from the north-west: brighter where the ground rises towards it.
                let d = m_per_px.max(SPACING_M);
                let slope = (height_at(world, x - d, z - d) - height_at(world, x + d, z + d)) / (2.0 * d);
                let shade = (1.0 + 1.5 * slope).clamp(0.55, 1.35);
                c = c.map(|v| v * shade);
                c = match world.land_at(x, z).and_then(texel_use) {
                    Some(LandUse::Town) => mix(c, [200.0, 120.0, 200.0], 0.45),
                    Some(LandUse::Housing) => mix(c, [230.0, 220.0, 90.0], 0.35),
                    Some(LandUse::Road) => mix(c, [150.0, 110.0, 70.0], 0.75),
                    _ => c,
                };
            }
            let on_line = |v0: f32, v: f32, step: f32| ((v / step).floor() - ((v - m_per_px) / step).floor()) != 0.0 && v > v0 + m_per_px;
            if on_line(area.x0, x, 500.0) || on_line(area.z0, z, 500.0) {
                c = c.map(|v| v * 0.45);
            } else if m_per_px < 20.0 && (on_line(area.x0, x, 100.0) || on_line(area.z0, z, 100.0)) {
                c = c.map(|v| v * 0.8);
            }
            let o = (j * w + i) * 3;
            rgb[o..o + 3].copy_from_slice(&c.map(|v| v.clamp(0.0, 255.0) as u8));
        }
    }
    let mut pic = Picture { w, h, rgb };
    for m in marks {
        let (ci, cj) = (((m.x - area.x0) / m_per_px) as i32, ((m.z - area.z0) / m_per_px) as i32);
        for dj in -m.r..=m.r {
            for di in -m.r..=m.r {
                let (i, j) = (ci + di, cj + dj);
                if i >= 0 && j >= 0 && (i as usize) < pic.w && (j as usize) < pic.h {
                    let edge = di.abs() == m.r || dj.abs() == m.r;
                    let o = (j as usize * pic.w + i as usize) * 3;
                    pic.rgb[o..o + 3].copy_from_slice(if edge && m.r > 1 { &[0, 0, 0] } else { &m.rgb });
                }
            }
        }
    }
    pic
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &b in bytes {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = if crc & 1 == 1 { (crc >> 1) ^ 0xedb8_8320 } else { crc >> 1 };
        }
    }
    !crc
}

/// The picture as an 8-bit RGB PNG.
pub fn png(pic: &Picture) -> Vec<u8> {
    let mut raw = Vec::with_capacity((pic.w * 3 + 1) * pic.h);
    for row in pic.rgb.chunks(pic.w * 3) {
        raw.push(0);
        raw.extend_from_slice(row);
    }
    let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
    z.write_all(&raw).expect("in memory");
    let idat = z.finish().expect("in memory");
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut chunk = |kind: &[u8], data: &[u8]| {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let mut body = kind.to_vec();
        body.extend_from_slice(data);
        out.extend_from_slice(&body);
        out.extend_from_slice(&crc32(&body).to_be_bytes());
    };
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&(pic.w as u32).to_be_bytes());
    ihdr.extend_from_slice(&(pic.h as u32).to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    chunk(b"IHDR", &ihdr);
    chunk(b"IDAT", &idat);
    chunk(b"IEND", &[]);
    out
}

pub fn save(path: &Path, pic: &Picture) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    std::fs::write(path, png(pic)).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc_matches_the_png_reference() {
        // The CRC of an empty IEND chunk, as every PNG ends.
        assert_eq!(crc32(b"IEND"), 0xae42_6082);
    }

    #[test]
    fn renders_the_area_and_marks() {
        let world = World::flat(sim::terrain::encode(5.0));
        let area = Area::parse("0,0,100,50").unwrap();
        let pic = render(&world, area, 100, &[Mark { x: 50.0, z: 25.0, rgb: [255, 0, 0], r: 0 }]);
        assert_eq!((pic.w, pic.h), (100, 50));
        assert_eq!(&pic.rgb[(25 * 100 + 50) * 3..][..3], &[255, 0, 0]);
        assert!(png(&pic).starts_with(b"\x89PNG"));
        assert!(Area::parse("5,0,1,1").is_err());
    }
}
