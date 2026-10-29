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
            label: (20.0 * scale).round().max(14.0) as u32,
            title: (32.0 * scale).round().max(22.0) as u32,
            footer: (18.0 * scale).round().max(14.0) as u32,
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
    let step = (style.label as f32 * 1.25).ceil() as i32 + 8;
    let mut offsets: Vec<_> = [0, step]
        .into_iter()
        .flat_map(|row| {
            [0, -step, step]
                .into_iter()
                .map(move |column| (column, row))
        })
        .collect();
    offsets.sort_by_key(|&(dx, dy)| (dx * dx + dy * dy, dx.abs()));
    let mut captions: Vec<Caption> = Vec::new();
    for index in order {
        let island = &layout.islands[index];
        let name = if island.directory == "." {
            "/"
        } else {
            island.directory.rsplit('/').next().unwrap()
        };
        let budget = (island.radius * 1.8 * style.sx)
            .max(90.0 * style.scale)
            .min(width as f32 - 2.0 * style.padding as f32) as i32;
        let text = typography.fit(name, style.label, budget);
        let w = typography.width(&text, style.label) + 8;
        let h = (style.label as f32 * 1.25).ceil() as i32 + 4;
        let preferred = (
            (island.x * style.sx) as i32 - w / 2,
            ((island.y + island.radius * 0.45 + 16.0) * style.sy) as i32 - 2,
        );
        let position = offsets.iter().find_map(|&(dx, dy)| {
            let bounds = [preferred.0 + dx, preferred.1 + dy, w, h];
            if bounds[0] < style.padding
                || bounds[0] + w > width as i32 - style.padding
                || bounds[1] < style.padding + (style.title as f32 * 1.3) as i32
                || bounds[1] + h
                    > height as i32 - style.padding - (style.footer as f32 * 1.3) as i32
                || captions.iter().any(|other| overlaps(bounds, other.bounds))
            {
                return None;
            }
            Some(bounds)
        });
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
        assert!(captions.len() >= 8, "only {} labels fit", captions.len());
        for (index, caption) in captions.iter().enumerate() {
            for other in &captions[index + 1..] {
                assert!(!overlaps(caption.bounds, other.bounds));
            }
        }
    }
}
