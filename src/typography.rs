use anyhow::{Result, anyhow};
use fontdue::{Font, FontSettings, Metrics};
use image::RgbaImage;
use std::collections::BTreeMap;

/// Rasterize captions at their output size, independently of the pixel-art scene.
pub(crate) struct Typography {
    font: Font,
    glyphs: BTreeMap<(char, u32), (Metrics, Vec<u8>)>,
}
impl Typography {
    pub fn new() -> Result<Self> {
        let font = Font::from_bytes(
            include_bytes!("../assets/fonts/DepartureMono-Regular.otf") as &[u8],
            FontSettings::default(),
        )
        .map_err(|error| anyhow!("Cannot load bundled font: {error}"))?;
        Ok(Self {
            font,
            glyphs: BTreeMap::new(),
        })
    }

    pub fn width(&self, text: &str, size: u32) -> i32 {
        let mut width = 0.0;
        let mut previous = None;
        for ch in text.chars() {
            if let Some(left) = previous {
                width += self
                    .font
                    .horizontal_kern(left, ch, size as f32)
                    .unwrap_or(0.0);
            }
            width += self.font.metrics(ch, size as f32).advance_width;
            previous = Some(ch);
        }
        width.ceil() as i32
    }

    /// Fit by measured pixels without shrinking the font or splitting UTF-8.
    pub fn fit(&self, text: &str, size: u32, max_width: i32) -> String {
        let mut text: String = text
            .chars()
            .map(|ch| {
                if !ch.is_control() && self.font.has_glyph(ch) {
                    ch
                } else {
                    '?'
                }
            })
            .collect();
        if self.width(&text, size) <= max_width {
            return text;
        }
        while !text.is_empty() && self.width(&format!("{text}…"), size) > max_width {
            text.pop();
        }
        if self.width("…", size) <= max_width {
            text.push('…');
        }
        text
    }

    pub fn draw(
        &mut self,
        image: &mut RgbaImage,
        text: &str,
        origin: (i32, i32),
        size: u32,
        color: [u8; 4],
    ) {
        let ascent = self
            .font
            .horizontal_line_metrics(size as f32)
            .unwrap()
            .ascent;
        let baseline = origin.1 + ascent.ceil() as i32;
        let mut cursor = origin.0 as f32;
        let mut previous = None;
        for ch in text.chars() {
            if let Some(left) = previous {
                cursor += self
                    .font
                    .horizontal_kern(left, ch, size as f32)
                    .unwrap_or(0.0);
            }
            let font = &self.font;
            let (metrics, pixels) = self
                .glyphs
                .entry((ch, size))
                .or_insert_with(|| font.rasterize(ch, size as f32));
            let left = cursor.round() as i32 + metrics.xmin;
            let top = baseline - metrics.ymin - metrics.height as i32;
            for y in 0..metrics.height {
                for x in 0..metrics.width {
                    let px = left + x as i32;
                    let py = top + y as i32;
                    if px < 0 || py < 0 || px >= image.width() as i32 || py >= image.height() as i32
                    {
                        continue;
                    }
                    // The bundled font uses a pixel grid. Keep its edges hard at native size.
                    let alpha = if pixels[y * metrics.width + x] >= 128 {
                        u32::from(color[3])
                    } else {
                        0
                    };
                    if alpha == 0 {
                        continue;
                    }
                    let destination = image.get_pixel_mut(px as u32, py as u32);
                    for channel in 0..3 {
                        destination[channel] = ((u32::from(color[channel]) * alpha
                            + u32::from(destination[channel]) * (255 - alpha))
                            / 255) as u8;
                    }
                }
            }
            cursor += metrics.advance_width;
            previous = Some(ch);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    #[test]
    fn long_names_fit_without_reducing_text_size_or_breaking_unicode() {
        let typography = Typography::new().unwrap();
        for text in [
            "a-very-long-repository-name".repeat(5),
            "répertoire".repeat(12),
        ] {
            let fitted = typography.fit(&text, 20, 180);
            assert!(fitted.ends_with('…'));
            assert!(typography.width(&fitted, 20) <= 180);
        }
    }

    #[test]
    fn pixel_captions_keep_their_height_and_only_use_solid_ink() {
        let mut typography = Typography::new().unwrap();
        let mut image = RgbaImage::from_pixel(320, 40, Rgba([208, 229, 219, 255]));
        typography.draw(
            &mut image,
            "2026-10-29   143 files",
            (0, 0),
            22,
            [37, 61, 49, 255],
        );
        let ink: Vec<_> = image
            .enumerate_pixels()
            .filter(|(_, _, p)| p[0] < 120)
            .collect();
        assert!(
            ink.iter().map(|(_, y, _)| *y).max().unwrap()
                - ink.iter().map(|(_, y, _)| *y).min().unwrap()
                >= 12
        );
        assert!(
            image
                .pixels()
                .all(|p| p.0 == [208, 229, 219, 255] || p.0 == [37, 61, 49, 255])
        );
    }
}
