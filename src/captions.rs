use crate::{layout::Layout, typography::Typography};

#[derive(Clone, Copy)]
pub(crate) struct Style {
    pub sx: f32,
    pub sy: f32,
    pub scale: f32,
    pub label: u32,
    pub title: u32,
    pub footer: u32,
    pub padding: i32,
}
impl Style {
    pub fn new(width: u32, height: u32) -> Self {
        let scale = (width as f32 / 960.0).min(height as f32 / 540.0);
        Self {
            sx: width as f32 / 1280.0,
            sy: height as f32 / 720.0,
            scale,
            label: (14.0 * scale).round().max(11.0) as u32,
            title: (22.0 * scale).round().max(22.0) as u32,
            footer: (11.0 * scale).round().max(11.0) as u32,
            padding: (24.0 * scale).round().max(12.0) as i32,
        }
    }
}

pub(crate) struct Caption {
    pub island: usize,
    pub text: String,
    pub bounds: [i32; 4],
}

fn overlaps(a: [i32; 4], b: [i32; 4]) -> bool {
    a[0] < b[0] + b[2] + 4
        && a[0] + a[2] + 4 > b[0]
        && a[1] < b[1] + b[3] + 4
        && a[1] + a[3] + 4 > b[1]
}

fn crosses_rect(a: (f32, f32), b: (f32, f32), bounds: [i32; 4], margin: f32) -> bool {
    let mut near = 0.0f32;
    let mut far = 1.0f32;
    for (start, end, low, high) in [
        (
            a.0,
            b.0,
            bounds[0] as f32 - margin,
            (bounds[0] + bounds[2]) as f32 + margin,
        ),
        (
            a.1,
            b.1,
            bounds[1] as f32 - margin,
            (bounds[1] + bounds[3]) as f32 + margin,
        ),
    ] {
        let delta = end - start;
        if delta.abs() < 0.001 {
            if start < low || start > high {
                return false;
            }
        } else {
            let first = (low - start) / delta;
            let last = (high - start) / delta;
            near = near.max(first.min(last));
            far = far.min(first.max(last));
            if near > far {
                return false;
            }
        }
    }
    true
}

/// Reserve stable water positions for all planned islands, even before they appear.
pub(crate) fn plan(
    layout: &Layout,
    typography: &Typography,
    width: u32,
    height: u32,
) -> Vec<Caption> {
    let style = Style::new(width, height);
    let mut order: Vec<_> = (0..layout.islands.len()).collect();
    order.sort_by(|&a, &b| {
        layout.islands[a].depth.cmp(&layout.islands[b].depth).then(
            layout.islands[b]
                .radius
                .total_cmp(&layout.islands[a].radius),
        )
    });
    let mut captions: Vec<Caption> = Vec::new();
    for index in order {
        let island = &layout.islands[index];
        let name = if island.directory == "." {
            "/"
        } else {
            island.directory.rsplit('/').next().unwrap()
        };
        let budget = (island.radius * 2.5 * style.sx)
            .max(240.0 * style.scale)
            .min(width as f32 - 2.0 * style.padding as f32 - 8.0) as i32;
        let text = typography.fit(name, style.label, budget);
        let w = typography.width(&text, style.label) + 8;
        let h = (style.label as f32 * 1.25).ceil() as i32 + 4;
        let preferred = (
            (island.x * style.sx) as i32 - w / 2,
            ((island.y + island.radius * 0.45 + 16.0) * style.sy) as i32 - 2,
        );
        let above = ((island.y - island.radius * 0.45 - 40.0) * style.sy) as i32 - h;
        let side = (w / 2 + 12).min((34.0 * style.scale).max(22.0) as i32);
        let step = h + 4;
        let positions = [
            (preferred.0, preferred.1),
            (preferred.0, preferred.1 + step),
            (preferred.0, above),
            (preferred.0 - side, preferred.1),
            (preferred.0 + side, preferred.1),
            (preferred.0 - side, preferred.1 + step),
            (preferred.0 + side, preferred.1 + step),
            (preferred.0 - side, above),
            (preferred.0 + side, above),
        ];
        let top = style.padding + (style.title as f32 * 1.3) as i32;
        let bottom = height as i32 - style.padding - (style.footer as f32 * 1.3) as i32 - h;
        let position = positions
            .into_iter()
            .enumerate()
            .map(|(priority, (x, y))| {
                let bounds = [
                    x.clamp(
                        style.padding,
                        (width as i32 - style.padding - w).max(style.padding),
                    ),
                    y.clamp(top, bottom.max(top)),
                    w,
                    h,
                ];
                let text_collisions = captions
                    .iter()
                    .filter(|other| overlaps(bounds, other.bounds))
                    .count();
                let touches_another_island = layout.islands.iter().enumerate().any(|(i, other)| {
                    if i == index {
                        return false;
                    }
                    let x = other.x * style.sx;
                    let y = (other.y - 14.0) * style.sy;
                    let rx = (other.radius + 16.0) * style.sx;
                    let ry = (other.radius * 0.45 + 30.0) * style.sy;
                    let closest_x = x.clamp(bounds[0] as f32, (bounds[0] + w) as f32);
                    let closest_y = y.clamp(bounds[1] as f32, (bounds[1] + h) as f32);
                    ((closest_x - x) / rx).powi(2) + ((closest_y - y) / ry).powi(2) < 1.0
                });
                let touches_bridge = layout.bridges.iter().any(|bridge| {
                    bridge.points.windows(2).any(|p| {
                        crosses_rect(
                            (p[0].0 * style.sx, p[0].1 * style.sy),
                            (p[1].0 * style.sx, p[1].1 * style.sy),
                            bounds,
                            8.0 * style.scale,
                        )
                    })
                });
                let score = text_collisions * 1000
                    + usize::from(touches_another_island) * 20
                    + usize::from(touches_bridge) * 5;
                ((score, priority), bounds)
            })
            .min_by_key(|(score, _)| *score)
            .map(|(_, bounds)| bounds);
        // Keep every island named, using the least crowded nearby position.
        if let Some(bounds) = position {
            captions.push(Caption {
                island: index,
                text,
                bounds,
            });
        }
    }
    captions
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::Island;

    #[test]
    fn crowded_directory_labels_do_not_overlap_or_invade_the_title_and_footer() {
        let layout = Layout {
            islands: (0..12)
                .map(|i| Island {
                    directory: format!("directory-{i}"),
                    depth: 1,
                    parent: None,
                    x: 300.0 + (i % 3) as f32 * 95.0,
                    y: 180.0 + (i / 3) as f32 * 110.0,
                    radius: 30.0,
                })
                .collect(),
            bridges: vec![],
            snapshots: vec![],
            hidden_plants: 0,
            threshold: 1,
            average_files_per_directory: 1.0,
        };
        let captions = plan(&layout, &Typography::new().unwrap(), 960, 540);
        assert_eq!(captions.len(), layout.islands.len());
        assert!(captions.iter().all(|c| !c.text.contains('…')));
        for (index, caption) in captions.iter().enumerate() {
            for other in &captions[index + 1..] {
                assert!(!overlaps(caption.bounds, other.bounds));
            }
        }
    }
    #[test]
    fn bridge_clearance_uses_the_segment_not_its_bounding_box() {
        let label = [40, 40, 20, 20];
        assert!(crosses_rect((0.0, 50.0), (100.0, 50.0), label, 4.0));
        assert!(crosses_rect((50.0, 0.0), (50.0, 100.0), label, 4.0));
        assert!(!crosses_rect((0.0, 35.0), (100.0, 35.0), label, 4.0));
        assert!(!crosses_rect((0.0, 0.0), (100.0, 30.0), label, 4.0));
    }
}
