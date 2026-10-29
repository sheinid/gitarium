use crate::{
    animation::{Animation, Building, Plant, Scene},
    assets::Assets,
    captions::{self, Caption, Style},
    history::History,
    layout::{Layout, path_position},
    species::{classify, hash},
    typography::Typography,
};
use anyhow::Result;
use image::{
    Rgba, RgbaImage,
    imageops::{self, FilterType},
};
use std::collections::BTreeMap;

const WATER: [u8; 4] = [208, 229, 219, 255];
const INK: [u8; 4] = [37, 61, 49, 255];
const QUIET: [u8; 4] = [52, 78, 62, 255];

struct Canvas {
    image: RgbaImage,
}
impl Canvas {
    fn pixel(&mut self, x: i32, y: i32, color: [u8; 4]) {
        if x < 0 || y < 0 || x >= self.image.width() as i32 || y >= self.image.height() as i32 {
            return;
        }
        let pixel = self.image.get_pixel_mut(x as u32, y as u32);
        let alpha = u32::from(color[3]);
        for i in 0..3 {
            pixel[i] =
                ((u32::from(color[i]) * alpha + u32::from(pixel[i]) * (255 - alpha)) / 255) as u8;
        }
    }
    fn rect(&mut self, x: i32, y: i32, w: i32, h: i32, color: [u8; 4]) {
        for py in y.max(0)..(y + h).min(self.image.height() as i32) {
            for px in x.max(0)..(x + w).min(self.image.width() as i32) {
                self.pixel(px, py, color);
            }
        }
    }
    fn line(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, color: [u8; 4]) {
        let steps = (x1 - x0).abs().max((y1 - y0).abs()).ceil() as usize;
        for s in 0..=steps {
            let t = s as f32 / steps.max(1) as f32;
            self.rect(
                (x0 + (x1 - x0) * t) as i32,
                (y0 + (y1 - y0) * t) as i32,
                2,
                2,
                color,
            );
        }
    }
    fn ellipse(&mut self, x: f32, y: f32, rx: f32, ry: f32, color: [u8; 4]) {
        if rx < 1.0 || ry < 1.0 {
            return;
        }
        for py in ((y - ry) as i32).max(0)..=((y + ry) as i32).min(self.image.height() as i32 - 1) {
            let extent = rx * (1.0 - ((py as f32 - y) / ry).powi(2)).max(0.0).sqrt();
            self.rect((x - extent) as i32, py, (extent * 2.0) as i32 + 1, 1, color);
        }
    }
    fn polygon(&mut self, points: &[(f32, f32)], color: [u8; 4]) {
        let min_y = points.iter().map(|p| p.1 as i32).min().unwrap_or(0).max(0);
        let max_y = points
            .iter()
            .map(|p| p.1 as i32)
            .max()
            .unwrap_or(0)
            .min(self.image.height() as i32 - 1);
        for y in min_y..=max_y {
            let mut crossings = Vec::new();
            for i in 0..points.len() {
                let (x1, y1) = points[i];
                let (x2, y2) = points[(i + 1) % points.len()];
                if (y1 <= y as f32 && y2 > y as f32) || (y2 <= y as f32 && y1 > y as f32) {
                    crossings.push(x1 + (y as f32 - y1) / (y2 - y1) * (x2 - x1));
                }
            }
            crossings.sort_by(f32::total_cmp);
            for pair in crossings.chunks_exact(2) {
                self.rect(pair[0] as i32, y, (pair[1] - pair[0]) as i32 + 1, 1, color);
            }
        }
    }
    fn sprite(&mut self, sprite: &RgbaImage, x: i32, y: i32, opacity: f32) {
        for (sx, sy, pixel) in sprite.enumerate_pixels() {
            if pixel[3] == 0 {
                continue;
            }
            let mut c = pixel.0;
            c[3] = (f32::from(c[3]) * opacity) as u8;
            self.pixel(x + sx as i32, y + sy as i32, c);
        }
    }
}
fn bank(x: f32, y: f32, rx: f32, ry: f32, seed: u64) -> Vec<(f32, f32)> {
    let phase = (seed % 1000) as f32 / 159.0;
    (0..36)
        .map(|i| {
            let a = i as f32 / 36.0 * std::f32::consts::TAU;
            let edge = 1.0 + (a * 3.0 + phase).sin() * 0.055 + (a * 5.0 - phase).cos() * 0.035;
            (x + a.cos() * rx * edge, y + a.sin() * ry * edge)
        })
        .collect()
}

/// A quiet patch of open water, outside every island's eventual shore.
fn open_water(layout: &Layout, key: &str) -> (f32, f32) {
    let seed = hash(key);
    for i in 0..200 {
        let h = hash(&format!("{seed}-{i}"));
        let x = 75.0 + (h % 1130) as f32;
        let y = 95.0 + ((h >> 16) % 495) as f32;
        if layout.islands.iter().all(|island| {
            ((x - island.x) / (island.radius + 54.0)).powi(2)
                + ((y - island.y) / (island.radius * 0.45 + 36.0)).powi(2)
                > 1.0
        }) {
            return (x, y);
        }
    }
    (1175.0, 595.0)
}

struct BridgeSprite {
    x: i32,
    y: i32,
    image: RgbaImage,
    progress: Vec<f32>,
}

fn bridge_sprite(source: &RgbaImage, points: &[(f32, f32)], radius: f32) -> BridgeSprite {
    let width = (radius * 0.30).clamp(7.0, 13.0);
    let min_x = (points.iter().map(|p| p.0).fold(f32::INFINITY, f32::min) - width).floor() as i32;
    let min_y = (points.iter().map(|p| p.1).fold(f32::INFINITY, f32::min) - width).floor() as i32;
    let max_x =
        (points.iter().map(|p| p.0).fold(f32::NEG_INFINITY, f32::max) + width).ceil() as i32;
    let max_y =
        (points.iter().map(|p| p.1).fold(f32::NEG_INFINITY, f32::max) + width).ceil() as i32;
    let lengths: Vec<_> = points
        .windows(2)
        .map(|p| (p[1].0 - p[0].0).hypot(p[1].1 - p[0].1).max(0.001))
        .collect();
    let total = lengths.iter().sum::<f32>().max(0.001);
    let mut image = RgbaImage::new((max_x - min_x + 1) as u32, (max_y - min_y + 1) as u32);
    let mut progress = vec![0.0; image.width() as usize * image.height() as usize];
    let image_width = image.width();
    for (x, y, pixel) in image.enumerate_pixels_mut() {
        let px = (x as i32 + min_x) as f32;
        let py = (y as i32 + min_y) as f32;
        let mut traveled = 0.0;
        let mut nearest = f32::INFINITY;
        let mut uv = (0.0, 0.0);
        for (i, pair) in points.windows(2).enumerate() {
            let length = lengths[i];
            let ux = (pair[1].0 - pair[0].0) / length;
            let uy = (pair[1].1 - pair[0].1) / length;
            let vx = px - pair[0].0;
            let vy = py - pair[0].1;
            let along = (vx * ux + vy * uy).clamp(0.0, length);
            let distance = (vx - ux * along).hypot(vy - uy * along);
            if distance < nearest {
                nearest = distance;
                uv = (
                    (traveled + along) / total,
                    (-vx * uy + vy * ux) / width + 0.5,
                );
            }
            traveled += length;
        }
        if nearest < width * 0.5 && (0.0..1.0).contains(&uv.0) && (0.0..1.0).contains(&uv.1) {
            progress[(y * image_width + x) as usize] = uv.0;
            *pixel = *source.get_pixel(
                (uv.0 * source.width() as f32) as u32,
                (uv.1 * source.height() as f32) as u32,
            );
        }
    }
    BridgeSprite {
        x: min_x,
        y: min_y,
        image,
        progress,
    }
}

enum Object {
    Plant(Plant),
    Building(Building),
}
impl Object {
    fn y(&self) -> f32 {
        match self {
            Self::Plant(p) => p.y,
            Self::Building(b) => b.y,
        }
    }
}

pub struct Renderer {
    pub layout: Layout,
    assets: Assets,
    cache: BTreeMap<(usize, u32, bool), RgbaImage>,
    bridges: Vec<BridgeSprite>,
    background: RgbaImage,
    ducks: (f32, f32),
    animation: Animation,
    typography: Typography,
    captions: BTreeMap<(u32, u32), Vec<Caption>>,
}

impl Renderer {
    pub fn new(layout: Layout) -> Result<Self> {
        let assets = Assets::load()?;
        let mut c = Canvas {
            image: RgbaImage::from_pixel(1280, 720, Rgba(WATER)),
        };
        for i in 0..22 {
            let h = hash(&format!("pond ripple {i}"));
            let x = 24.0 + (h % 1220) as f32;
            let y = 32.0 + ((h >> 16) % 610) as f32;
            let width = 10.0 + ((h >> 32) % 17) as f32;
            c.line(x, y, x + width, y, [183, 212, 201, 125]);
            if i % 3 == 0 {
                c.line(
                    x + 7.0,
                    y + 5.0,
                    x + width + 5.0,
                    y + 5.0,
                    [238, 242, 218, 145],
                );
            }
        }
        let ducks = open_water(&layout, "ducks");
        let bridges = layout
            .bridges
            .iter()
            .map(|b| {
                bridge_sprite(
                    &assets.bridges[b.sprite],
                    &b.points,
                    layout.islands[b.child].radius,
                )
            })
            .collect();
        Ok(Self {
            layout,
            assets,
            cache: BTreeMap::new(),
            bridges,
            background: c.image,
            ducks,
            animation: Animation::default(),
            typography: Typography::new()?,
            captions: BTreeMap::new(),
        })
    }

    fn prop(&self, c: &mut Canvas, id: usize, x: f32, y: f32, scale: u32, opacity: f32) {
        let source = &self.assets.shore[id];
        let sprite = imageops::resize(
            source,
            source.width() * scale,
            source.height() * scale,
            FilterType::Nearest,
        );
        c.sprite(
            &sprite,
            (x - sprite.width() as f32 / 2.0) as i32,
            (y - sprite.height() as f32) as i32,
            opacity,
        );
    }

    fn building(&mut self, c: &mut Canvas, building: &Building, land_opacity: f32) {
        let opacity = building.opacity * land_opacity;
        let source = &self.assets.buildings[building.sprite];
        let scale = (building.radius / 90.0).clamp(0.75, 1.25);
        let h = (source.height() as f32 * scale * building.opacity.sqrt())
            .round()
            .max(2.0) as u32;
        let sprite = self
            .cache
            .entry((32 + building.sprite, h, false))
            .or_insert_with(|| {
                let w = (source.width() as f32 / source.height() as f32 * h as f32)
                    .round()
                    .max(1.0) as u32;
                imageops::resize(source, w, h, FilterType::Nearest)
            });
        c.ellipse(
            building.x,
            building.y,
            sprite.width() as f32 * 0.35,
            2.0,
            [68, 83, 54, (48.0 * opacity) as u8],
        );
        c.sprite(
            sprite,
            (building.x - sprite.width() as f32 / 2.0) as i32,
            (building.y - sprite.height() as f32) as i32,
            opacity,
        );
    }

    /// Still images show the requested committed state immediately.
    pub fn frame(
        &mut self,
        history: &History,
        position: f32,
        time: f32,
        width: u32,
        height: u32,
    ) -> RgbaImage {
        let index = (position.round() as usize).min(history.snapshots.len() - 1);
        let snapshot = &history.snapshots[index];
        let states = &self.layout.snapshots[index];
        let islands: Vec<f32> = states
            .iter()
            .map(|s| if s.active { 1.0 } else { 0.0 })
            .collect();
        let bridges = self
            .layout
            .bridges
            .iter()
            .map(|b| islands[b.parent].min(islands[b.child]))
            .collect();
        let plants = states
            .iter()
            .enumerate()
            .flat_map(|(i, state)| {
                let island = &self.layout.islands[i];
                state.plants.iter().map(move |path| {
                    let file = &snapshot.files[path];
                    let (dx, dy) = path_position(path, island.radius);
                    Plant {
                        path: path.clone(),
                        island: i,
                        x: island.x + dx,
                        y: island.y + dy - 3.0,
                        radius: island.radius,
                        opacity: 1.0,
                        bytes: file.bytes,
                        born: file.born,
                    }
                })
            })
            .collect();
        let buildings = states
            .iter()
            .enumerate()
            .flat_map(|(i, state)| {
                let radius = self.layout.islands[i].radius;
                state.buildings.iter().map(move |b| Building {
                    directory: b.directory.clone(),
                    island: i,
                    x: b.x,
                    y: b.y,
                    radius,
                    opacity: 1.0,
                    sprite: b.sprite,
                    file_count: b.file_count,
                })
            })
            .collect();
        self.paint(
            history,
            position,
            time,
            width,
            height,
            Scene {
                islands,
                bridges,
                plants,
                buildings,
            },
        )
    }

    pub fn animated_frame(
        &mut self,
        history: &History,
        position: f32,
        time: f32,
        width: u32,
        height: u32,
        seconds: f32,
    ) -> RgbaImage {
        let scene = self
            .animation
            .update(history, &self.layout, position, time, seconds);
        self.paint(history, position, time, width, height, scene)
    }

    fn paint(
        &mut self,
        history: &History,
        position: f32,
        time: f32,
        width: u32,
        height: u32,
        scene: Scene,
    ) -> RgbaImage {
        let current =
            &history.snapshots[(position.floor() as usize).min(history.snapshots.len() - 1)];
        let lives = &scene.islands;
        let mut c = Canvas {
            image: self.background.clone(),
        };
        let (duck_x, duck_y) = self.ducks;
        let drift = (time * 0.17).sin() * 7.0;
        self.prop(&mut c, 7, duck_x + drift, duck_y, 2, 1.0);
        self.prop(&mut c, 8, duck_x + drift - 29.0, duck_y + 5.0, 2, 1.0);
        c.line(
            duck_x + drift - 47.0,
            duck_y + 6.0,
            duck_x + drift - 39.0,
            duck_y + 6.0,
            [174, 205, 192, 150],
        );
        // Paint connections underneath the banks, so both ends land on their actual parent and child.
        for (index, bridge) in self.layout.bridges.iter().enumerate() {
            let reveal = scene.bridges[index];
            let opacity = lives[bridge.parent].min(lives[bridge.child]);
            if reveal > 0.001 && opacity > 0.001 {
                let sprite = &self.bridges[index];
                for (sx, sy, pixel) in sprite.image.enumerate_pixels() {
                    if pixel[3] == 0 {
                        continue;
                    }
                    let along = sprite.progress[(sy * sprite.image.width() + sx) as usize];
                    let front = ((reveal - along) * 24.0).clamp(0.0, 1.0);
                    let mut color = pixel.0;
                    color[3] = (f32::from(color[3]) * opacity * front) as u8;
                    if color[3] > 0 {
                        c.pixel(sprite.x + sx as i32, sprite.y + sy as i32, color);
                    }
                }
            }
        }
        let mut ordered: Vec<_> = (0..self.layout.islands.len()).collect();
        ordered.sort_by(|&a, &b| {
            self.layout.islands[a]
                .y
                .total_cmp(&self.layout.islands[b].y)
        });
        for &i in &ordered {
            let island = &self.layout.islands[i];
            let life = lives[i];
            if life < 0.01 {
                continue;
            }
            let r = island.radius * (0.80 + life * 0.20);
            let x = island.x;
            let y = island.y + (1.0 - life) * 7.0;
            let seed = hash(&island.directory);
            c.polygon(
                &bank(x, y + 3.0, r + 9.0, r * 0.45 + 7.0, seed),
                [187, 215, 191, (255.0 * life) as u8],
            );
            c.polygon(
                &bank(x, y + 2.0, r + 4.0, r * 0.45 + 3.0, seed),
                [217, 221, 183, (255.0 * life) as u8],
            );
            c.polygon(
                &bank(x, y, r, r * 0.45, seed),
                [224, 216, 172, (255.0 * life) as u8],
            );
            let grass = if seed % 2 == 0 {
                [157, 177, 113, (255.0 * life) as u8]
            } else {
                [151, 173, 108, (255.0 * life) as u8]
            };
            c.polygon(
                &bank(
                    x - 1.0,
                    y - 2.0,
                    (r - 5.0).max(2.0),
                    (r * 0.45 - 4.0).max(2.0),
                    seed,
                ),
                grass,
            );
            if r > 27.0 {
                let edge = if seed % 2 == 0 { -1.0 } else { 1.0 };
                let prop = [0, 1, 4, 5, 6, 11, 12, 13, 14, 15][(seed % 10) as usize];
                self.prop(&mut c, prop, x + edge * r * 0.73, y + r * 0.22, 1, life);
                if seed % 3 == 0 {
                    self.prop(
                        &mut c,
                        [2, 3, 9, 15][((seed >> 8) % 4) as usize],
                        x - edge * r * 1.04,
                        y + r * 0.42,
                        1,
                        life,
                    );
                }
            }
        }
        let mut objects: Vec<_> = scene
            .plants
            .into_iter()
            .map(Object::Plant)
            .chain(scene.buildings.into_iter().map(Object::Building))
            .collect();
        objects.sort_by(|a, b| a.y().total_cmp(&b.y()));
        for object in objects {
            match object {
                Object::Building(building) => {
                    self.building(&mut c, &building, lives[building.island])
                }
                Object::Plant(plant) => {
                    let path = plant.path.as_str();
                    let px = plant.x;
                    let py = plant.y;
                    let radius = plant.radius;
                    let visibility = plant.opacity * lives[plant.island];
                    let species = classify(path);
                    let source = &self.assets.plants[species.id];
                    let age = (position - plant.born as f32).max(0.0);
                    let maturity = 0.64 + 0.36 * (age / 7.0).min(1.0);
                    let size = 0.90 + ((plant.bytes + 1) as f32).ln().min(12.0) * 0.018;
                    let scale = (radius / 64.0).clamp(0.55, 1.6);
                    let h = (source.height() as f32 * scale * maturity * size * visibility.sqrt())
                        .round()
                        .max(3.0) as u32;
                    let flip = hash(path) & 1 == 1;
                    let sprite = self.cache.entry((species.id, h, flip)).or_insert_with(|| {
                        let w = (source.width() as f32 / source.height() as f32 * h as f32)
                            .round()
                            .max(1.0) as u32;
                        let sprite = imageops::resize(source, w, h, FilterType::Nearest);
                        if flip {
                            imageops::flip_horizontal(&sprite)
                        } else {
                            sprite
                        }
                    });
                    c.ellipse(
                        px,
                        py,
                        source.width() as f32 * scale * 0.22,
                        1.5,
                        [95, 122, 76, (42.0 * visibility) as u8],
                    );
                    let sway = if matches!(species.id, 8 | 14 | 17 | 27) {
                        0.0
                    } else {
                        (time * 1.15 + (hash(path) % 90) as f32).sin() * 0.6
                    };
                    c.sprite(
                        sprite,
                        (px - sprite.width() as f32 / 2.0 + sway) as i32,
                        (py - h as f32) as i32,
                        visibility,
                    );
                }
            }
        }
        // Resize the artwork first. Captions retain their own full-resolution glyphs.
        let image = if width == 1280 && height == 720 {
            c.image
        } else {
            imageops::resize(&c.image, width, height, FilterType::Nearest)
        };
        let style = Style::new(width, height);
        let label_size = style.label;
        let title_size = style.title;
        let footer_size = style.footer;
        let padding = style.padding;
        let captions = self
            .captions
            .entry((width, height))
            .or_insert_with(|| captions::plan(&self.layout, &self.typography, width, height));
        let mut image = image;
        // Nearby captions use separate rows. A one-pixel water halo clears plant details.
        for caption in captions.iter() {
            let life = lives[caption.island];
            if life < 0.01 {
                continue;
            }
            let origin = (caption.bounds[0] + 4, caption.bounds[1] + 2);
            for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                self.typography.draw(
                    &mut image,
                    &caption.text,
                    (origin.0 + dx, origin.1 + dy),
                    label_size,
                    [WATER[0], WATER[1], WATER[2], (255.0 * life) as u8],
                );
            }
            self.typography.draw(
                &mut image,
                &caption.text,
                origin,
                label_size,
                [37, 61, 49, (255.0 * life) as u8],
            );
        }
        let title = self
            .typography
            .fit(&history.name, title_size, width as i32 - padding * 2);
        let title_width = self.typography.width(&title, title_size);
        self.typography.draw(
            &mut image,
            &title,
            ((width as i32 - title_width) / 2, padding - 4),
            title_size,
            INK,
        );
        let mut footer = format!(
            "{}  ·  {} files",
            &current.commit.date[..10],
            current.files.len()
        );
        if history.shallow {
            footer.push_str("  ·  partial history");
        }
        let footer = self
            .typography
            .fit(&footer, footer_size, width as i32 - padding * 2);
        let footer_width = self.typography.width(&footer, footer_size);
        self.typography.draw(
            &mut image,
            &footer,
            (
                width as i32 - padding - footer_width,
                height as i32 - padding - footer_size as i32,
            ),
            footer_size,
            QUIET,
        );
        if current.files.is_empty() {
            let text = "no files yet";
            let text_width = self.typography.width(text, label_size);
            self.typography.draw(
                &mut image,
                text,
                ((width as i32 - text_width) / 2, height as i32 / 2),
                label_size,
                QUIET,
            );
        }
        image
    }
}
