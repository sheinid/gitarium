use anyhow::{Result, ensure};
use image::{
    RgbaImage,
    imageops::{self, FilterType},
};

const PALETTE: [[u8; 3]; 22] = [
    [68, 83, 54],
    [91, 108, 63],
    [119, 137, 79],
    [149, 165, 106],
    [178, 189, 134],
    [96, 80, 57],
    [135, 107, 73],
    [177, 148, 103],
    [216, 200, 155],
    [240, 231, 199],
    [160, 109, 105],
    [195, 147, 139],
    [216, 176, 164],
    [94, 119, 148],
    [130, 154, 177],
    [121, 110, 140],
    [155, 138, 164],
    [191, 132, 76],
    [212, 177, 94],
    [132, 137, 121],
    [172, 176, 157],
    [163, 189, 172],
];

pub struct Assets {
    pub plants: Vec<RgbaImage>,
    pub shore: Vec<RgbaImage>,
    pub bridges: Vec<RgbaImage>,
}

impl Assets {
    pub fn load() -> Result<Self> {
        let mut plants = sheet(
            include_bytes!("../assets/pond-plants-a.png"),
            &[
                26, 28, 30, 28, 20, 38, 23, 38, 14, 18, 28, 24, 26, 16, 14, 15,
            ],
        )?;
        plants.extend(sheet(
            include_bytes!("../assets/pond-plants-b.png"),
            &[
                36, 25, 27, 38, 27, 29, 20, 16, 30, 25, 25, 18, 32, 25, 22, 18,
            ],
        )?);
        let shore = sheet(
            include_bytes!("../assets/pond-shore.png"),
            &[12, 23, 12, 11, 10, 10, 13, 14, 9, 9, 14, 24, 7, 18, 8, 9],
        )?;
        let bridges = sheet(include_bytes!("../assets/bridges.png"), &[12; 16])?;
        Ok(Self {
            plants,
            shore,
            bridges,
        })
    }
}

/// Treat the sheets as game assets: trim padding, use a fixed pixel grid, and hard alpha.
/// This keeps the same sprite silhouettes legible at both PNG and video resolutions.
fn sheet(bytes: &[u8], heights: &[u32; 16]) -> Result<Vec<RgbaImage>> {
    let atlas = image::load_from_memory(bytes)?.to_rgba8();
    let transparent = atlas.pixels().filter(|p| p[3] < 32).count();
    ensure!(
        transparent > atlas.width() as usize * atlas.height() as usize / 4,
        "Sprite sheets must have a transparent background"
    );
    let mut sprites = Vec::with_capacity(16);
    for (i, &height) in heights.iter().enumerate() {
        let x = i as u32 % 4 * atlas.width() / 4;
        let y = i as u32 / 4 * atlas.height() / 4;
        let right = (i as u32 % 4 + 1) * atlas.width() / 4;
        let bottom = (i as u32 / 4 + 1) * atlas.height() / 4;
        let cell = imageops::crop_imm(&atlas, x, y, right - x, bottom - y).to_image();
        let mut min_x = cell.width();
        let mut min_y = cell.height();
        let mut max_x = 0;
        let mut max_y = 0;
        for (x, y, p) in cell.enumerate_pixels() {
            if p[3] > 96 {
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
        }
        ensure!(min_x <= max_x && min_y <= max_y, "Empty sprite cell {i}");
        let trimmed = imageops::crop_imm(&cell, min_x, min_y, max_x - min_x + 1, max_y - min_y + 1)
            .to_image();
        let width = ((trimmed.width() as f32 / trimmed.height() as f32) * height as f32)
            .round()
            .max(1.0) as u32;
        ensure!(width < 160, "Unexpected sprite proportions in cell {i}");
        let mut sprite = imageops::resize(&trimmed, width, height, FilterType::Nearest);
        for pixel in sprite.pixels_mut() {
            pixel[3] = if pixel[3] > 96 { 255 } else { 0 };
            if pixel[3] > 0 {
                let nearest = PALETTE
                    .iter()
                    .min_by_key(|color| {
                        (0..3)
                            .map(|i| (i32::from(pixel[i]) - i32::from(color[i])).pow(2))
                            .sum::<i32>()
                    })
                    .unwrap();
                for i in 0..3 {
                    pixel[i] = nearest[i];
                }
            }
        }
        sprites.push(sprite);
    }
    ensure!(sprites.len() == 16, "Incomplete sprite sheet");
    Ok(sprites)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_embedded_asset_has_a_visible_silhouette() {
        let assets = Assets::load().unwrap();
        assert_eq!(assets.plants.len(), 32);
        assert_eq!(assets.shore.len(), 16);
        assert_eq!(assets.bridges.len(), 16);
        for sprite in assets
            .plants
            .iter()
            .chain(&assets.shore)
            .chain(&assets.bridges)
        {
            assert!(sprite.pixels().filter(|p| p[3] > 0).count() > 8);
            assert!(sprite.pixels().all(|p| p[3] == 0 || p[3] == 255));
        }
    }
}
