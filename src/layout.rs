use crate::{history::History, species::hash};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug)]
pub struct Island {
    pub directory: String,
    pub depth: usize,
    pub parent: Option<usize>,
    pub x: f32,
    pub y: f32,
    pub radius: f32,
}
#[derive(Clone, Debug)]
pub struct Bridge {
    pub parent: usize,
    pub child: usize,
    pub sprite: usize,
    pub points: Vec<(f32, f32)>,
}
#[derive(Clone, Debug)]
pub struct IslandState {
    pub active: bool,
    pub file_count: usize,
    pub plants: Vec<String>,
}
#[derive(Clone, Debug)]
pub struct Layout {
    pub islands: Vec<Island>,
    pub bridges: Vec<Bridge>,
    pub snapshots: Vec<Vec<IslandState>>,
    pub hidden_plants: usize,
    pub threshold: usize,
    pub average_files_per_directory: f64,
}

fn parent_name(directory: &str) -> &str {
    directory.rsplit_once('/').map_or(".", |(parent, _)| parent)
}
fn directory(path: &str) -> &str {
    path.rsplit_once('/')
        .map_or(".", |(directory, _)| directory)
}
fn depth(directory: &str) -> usize {
    if directory == "." {
        0
    } else {
        directory.split('/').count()
    }
}

/// A directory threshold estimated from this repository, not a claimed global average.
/// Six average-sized leaf directories, rounded to four files. Small repositories scale down.
fn automatic_threshold(history: &History) -> (usize, f64) {
    let latest = &history.snapshots.last().expect("nonempty history").files;
    let fallback;
    let paths: Vec<&str> = if latest.is_empty() {
        fallback = history.union();
        fallback.iter().map(String::as_str).collect()
    } else {
        latest.keys().map(String::as_str).collect()
    };
    let directories: BTreeSet<_> = paths.iter().map(|p| directory(p)).collect();
    let average = paths.len() as f64 / directories.len().max(1) as f64;
    let based_on_mean = ((average * 6.0 / 4.0).ceil() as usize * 4).clamp(12, 64);
    let small_repo_limit = paths.len().div_ceil(12).max(4);
    (based_on_mean.min(small_repo_limit), average)
}

impl Layout {
    pub fn new(history: &History, max_depth: usize, max_islands: usize, max_plants: usize) -> Self {
        Self::with_threshold(history, max_depth, max_islands, max_plants, None)
    }

    pub fn with_threshold(
        history: &History,
        max_depth: usize,
        max_islands: usize,
        max_plants: usize,
        requested_threshold: Option<usize>,
    ) -> Self {
        let union = history.union();
        let (automatic, average_files_per_directory) = automatic_threshold(history);
        let threshold = requested_threshold.unwrap_or(automatic).max(1);
        let mut names = BTreeSet::from([".".to_owned()]);
        for path in &union {
            let mut name = directory(path);
            while name != "." {
                names.insert(name.to_owned());
                name = parent_name(name);
            }
        }
        let mut names: Vec<_> = names.into_iter().collect();
        names.sort_by(|a, b| depth(a).cmp(&depth(b)).then(a.cmp(b)));
        let indices: BTreeMap<_, _> = names
            .iter()
            .enumerate()
            .map(|(i, p)| (p.as_str(), i))
            .collect();
        let parents: Vec<_> = names
            .iter()
            .enumerate()
            .map(|(i, p)| {
                if i == 0 {
                    None
                } else {
                    Some(indices[parent_name(p)])
                }
            })
            .collect();
        let owners: BTreeMap<_, _> = union
            .iter()
            .map(|p| (p.as_str(), indices[directory(p)]))
            .collect();
        let mut all_counts = Vec::with_capacity(history.snapshots.len());
        let mut peaks = vec![0; names.len()];
        for snapshot in &history.snapshots {
            let mut counts = vec![0usize; names.len()];
            for path in snapshot.files.keys() {
                counts[owners[path.as_str()]] += 1;
            }
            for i in (1..names.len()).rev() {
                counts[parents[i].unwrap()] += counts[i];
            }
            for (i, &count) in counts.iter().enumerate() {
                peaks[i] = peaks[i].max(count);
            }
            all_counts.push(counts);
        }
        // Ranking ancestors before descendants keeps the selected tree closed under ancestry.
        let mut ranked: Vec<_> = (1..names.len())
            .filter(|&i| depth(&names[i]) <= max_depth && peaks[i] > threshold)
            .collect();
        ranked.sort_by(|&a, &b| {
            peaks[b]
                .cmp(&peaks[a])
                .then(depth(&names[a]).cmp(&depth(&names[b])))
                .then(names[a].cmp(&names[b]))
        });
        let mut chosen = vec![0];
        chosen.extend(ranked.into_iter().take(max_islands.max(1) - 1));
        chosen.sort_unstable();
        let selected_indices: BTreeMap<_, _> = chosen
            .iter()
            .enumerate()
            .map(|(i, &all)| (all, i))
            .collect();
        let mut islands = Vec::with_capacity(chosen.len());
        for &all in &chosen {
            let mut ancestor = parents[all];
            while ancestor.is_some_and(|a| !selected_indices.contains_key(&a)) {
                ancestor = parents[ancestor.unwrap()];
            }
            let d = depth(&names[all]);
            islands.push(Island {
                directory: names[all].clone(),
                depth: d,
                parent: ancestor.map(|a| selected_indices[&a]),
                x: 0.0,
                y: 0.0,
                radius: 148.0 * 0.80f32.powi(d as i32),
            });
        }
        place_tree(&mut islands);
        let bridges = route_bridges(&islands);
        let membership: BTreeMap<_, _> = owners
            .iter()
            .map(|(&path, &owner)| {
                let mut ancestor = owner;
                while !selected_indices.contains_key(&ancestor) {
                    ancestor = parents[ancestor].unwrap();
                }
                (path, selected_indices[&ancestor])
            })
            .collect();
        let capacities: Vec<_> = islands
            .iter()
            .map(|i| ((i.radius * i.radius * 0.0075).round() as usize).clamp(2, 55))
            .collect();
        let mut snapshots = Vec::with_capacity(history.snapshots.len());
        for (step, snapshot) in history.snapshots.iter().enumerate() {
            let active: Vec<_> = chosen
                .iter()
                .enumerate()
                .map(|(i, &all)| i == 0 || all_counts[step][all] > threshold)
                .collect();
            let mut owned = vec![Vec::new(); islands.len()];
            for path in snapshot.files.keys() {
                let mut owner = membership[path.as_str()];
                while !active[owner] {
                    owner = islands[owner].parent.unwrap();
                }
                owned[owner].push(path.clone());
            }
            let available: Vec<_> = owned
                .iter()
                .zip(&capacities)
                .map(|(p, &cap)| p.len().min(cap))
                .collect();
            let total: usize = available.iter().sum();
            let budget = max_plants.min(total);
            let mut allocations: Vec<_> = available
                .iter()
                .map(|&a| budget * a / total.max(1))
                .collect();
            let mut remaining = budget - allocations.iter().sum::<usize>();
            for i in 0..allocations.len() {
                if remaining > 0 && allocations[i] < available[i] {
                    allocations[i] += 1;
                    remaining -= 1;
                }
            }
            let mut states = Vec::with_capacity(islands.len());
            for (i, mut plants) in owned.into_iter().enumerate() {
                let file_count = plants.len();
                plants.sort_by_key(|p| hash(p));
                plants.truncate(allocations[i]);
                states.push(IslandState {
                    active: active[i],
                    file_count,
                    plants,
                });
            }
            snapshots.push(states);
        }
        let drawn: usize = snapshots
            .last()
            .unwrap()
            .iter()
            .map(|i| i.plants.len())
            .sum();
        let hidden_plants = history
            .snapshots
            .last()
            .unwrap()
            .files
            .len()
            .saturating_sub(drawn);
        Self {
            islands,
            bridges,
            snapshots,
            hidden_plants,
            threshold,
            average_files_per_directory,
        }
    }

    pub fn summary(&self) -> Value {
        json!({"island_threshold":self.threshold,"mean_files_per_nonempty_directory":self.average_files_per_directory,
            "planned_islands":self.islands.len(),"visible_islands":self.snapshots.last().unwrap().iter().filter(|i|i.active).count(),
            "omitted_plants":self.hidden_plants,"bridges":self.bridges.iter().map(|b|json!({"parent":self.islands[b.parent].directory,"child":self.islands[b.child].directory,"sprite":b.sprite,"points":b.points})).collect::<Vec<_>>(),"islands":self.islands.iter().enumerate().map(|(i,n)|json!({"directory":n.directory,"depth":n.depth,
                "parent":n.parent.map(|p|self.islands[p].directory.as_str()),"radius":n.radius,"files":self.snapshots.last().unwrap()[i].file_count})).collect::<Vec<_>>()})
    }
}

fn place_tree(islands: &mut [Island]) {
    let mut children = vec![Vec::new(); islands.len()];
    for (i, island) in islands.iter().enumerate().skip(1) {
        children[island.parent.unwrap()].push(i);
    }
    let mut weight = vec![1.0f32; islands.len()];
    for i in (0..islands.len()).rev() {
        weight[i] = if children[i].is_empty() {
            1.0
        } else {
            children[i].iter().map(|&c| weight[c]).sum::<f32>()
        };
    }
    let mut angles = vec![0.0; islands.len()];
    fn sectors(
        index: usize,
        start: f32,
        end: f32,
        children: &[Vec<usize>],
        weight: &[f32],
        angles: &mut [f32],
    ) {
        angles[index] = (start + end) * 0.5;
        let (start, end) = if index == 0 {
            (start, end)
        } else {
            let midpoint = (start + end) * 0.5;
            let half = ((end - start) * 0.5).min(std::f32::consts::PI * 0.42);
            (midpoint - half, midpoint + half)
        };
        let mut cursor = start;
        for &child in &children[index] {
            let span = (end - start) * weight[child] / weight[index];
            sectors(child, cursor, cursor + span, children, weight, angles);
            cursor += span;
        }
    }
    sectors(
        0,
        -std::f32::consts::PI * 0.90,
        std::f32::consts::PI * 1.10,
        &children,
        &weight,
        &mut angles,
    );
    let max_depth = islands.iter().map(|i| i.depth).max().unwrap_or(0);
    let mut rings = vec![0.0f32; max_depth + 1];
    for (d, ring) in rings.iter_mut().enumerate().skip(1) {
        let radius = 148.0 * 0.80f32.powi(d as i32);
        *ring = (1..=d)
            .map(|level| {
                148.0 * 0.80f32.powi(level as i32 - 1) + 148.0 * 0.80f32.powi(level as i32) + 46.0
            })
            .sum::<f32>();
        let count = islands.iter().filter(|i| i.depth == d).count();
        *ring = ring.max(count as f32 * (radius * 2.0 + 45.0) / std::f32::consts::TAU);
    }
    for (i, island) in islands.iter_mut().enumerate() {
        let distance = rings[island.depth];
        island.x = angles[i].cos() * distance;
        island.y = angles[i].sin() * distance;
    }
    // Relax overlaps while keeping each branch near its initial radial sector.
    let anchors: Vec<_> = islands.iter().map(|i| (i.x, i.y)).collect();
    for _ in 0..220 {
        for i in 0..islands.len() {
            for j in i + 1..islands.len() {
                let dx = islands[j].x - islands[i].x;
                let dy = islands[j].y - islands[i].y;
                let distance = dx.hypot(dy).max(0.1);
                let gap = islands[i].radius + islands[j].radius + 64.0;
                if distance < gap {
                    let push = (gap - distance) * 0.35;
                    islands[i].x -= dx / distance * push;
                    islands[i].y -= dy / distance * push;
                    islands[j].x += dx / distance * push;
                    islands[j].y += dy / distance * push;
                }
            }
        }
        for (i, island) in islands.iter_mut().enumerate() {
            island.x += (anchors[i].0 - island.x) * 0.006;
            island.y += (anchors[i].1 - island.y) * 0.006;
        }
    }
    // Choose the rotation that uses the wide pond best, then project into shallow perspective.
    let bounds = |angle: f32| {
        let (sin, cos) = angle.sin_cos();
        let left = islands
            .iter()
            .map(|i| i.x * cos - i.y * sin - i.radius - 18.0)
            .fold(f32::INFINITY, f32::min);
        let right = islands
            .iter()
            .map(|i| i.x * cos - i.y * sin + i.radius + 18.0)
            .fold(f32::NEG_INFINITY, f32::max);
        let top = islands
            .iter()
            .map(|i| (i.x * sin + i.y * cos) * 0.58 - i.radius * 0.45 - 45.0)
            .fold(f32::INFINITY, f32::min);
        let bottom = islands
            .iter()
            .map(|i| (i.x * sin + i.y * cos) * 0.58 + i.radius * 0.45 + 24.0)
            .fold(f32::NEG_INFINITY, f32::max);
        let scale = (1100.0 / (right - left))
            .min(480.0 / (bottom - top))
            .min(1.5);
        (scale, left, right, top, bottom)
    };
    let angle = (0..36)
        .map(|i| i as f32 * std::f32::consts::PI / 36.0)
        .max_by(|&a, &b| bounds(a).0.total_cmp(&bounds(b).0))
        .unwrap_or(0.0);
    let (scale, left, right, top, bottom) = bounds(angle);
    let (sin, cos) = angle.sin_cos();
    for island in islands.iter_mut() {
        let x = island.x * cos - island.y * sin;
        let y = (island.x * sin + island.y * cos) * 0.58;
        island.x = 640.0 + (x - (left + right) * 0.5) * scale;
        island.y = 350.0 + (y - (top + bottom) * 0.5) * scale;
        island.radius *= scale;
    }
    let anchors: Vec<_> = islands.iter().map(|i| (i.x, i.y)).collect();
    for _ in 0..450 {
        for i in 0..islands.len() {
            for j in i + 1..islands.len() {
                let dx = islands[j].x - islands[i].x;
                let dy = islands[j].y - islands[i].y;
                let rx = islands[i].radius + islands[j].radius + 36.0;
                let ry = (islands[i].radius + islands[j].radius) * 0.45 + 49.0;
                let distance = ((dx / rx).powi(2) + (dy / ry).powi(2)).sqrt().max(0.001);
                if distance < 1.0 {
                    let push = (1.0 - distance) * 0.44;
                    let px = dx / distance * push;
                    let py = dy / distance * push;
                    islands[i].x -= px;
                    islands[i].y -= py;
                    islands[j].x += px;
                    islands[j].y += py;
                }
            }
        }
        for (i, island) in islands.iter_mut().enumerate() {
            island.x += (anchors[i].0 - island.x) * 0.003;
            island.y += (anchors[i].1 - island.y) * 0.003;
            island.x = island.x.clamp(island.radius + 45.0, 1235.0 - island.radius);
            island.y = island
                .y
                .clamp(island.radius * 0.45 + 105.0, 600.0 - island.radius * 0.45);
        }
    }
}

const GRID: f32 = 5.0;
const GRID_W: usize = 256;
const GRID_H: usize = 144;

fn cell(point: (f32, f32)) -> usize {
    let x = (point.0 / GRID).round().clamp(0.0, (GRID_W - 1) as f32) as usize;
    let y = (point.1 / GRID).round().clamp(0.0, (GRID_H - 1) as f32) as usize;
    y * GRID_W + x
}
fn point(index: usize) -> (f32, f32) {
    (
        (index % GRID_W) as f32 * GRID,
        (index / GRID_W) as f32 * GRID,
    )
}
fn line_clear(a: (f32, f32), b: (f32, f32), blocked: &[bool]) -> bool {
    let steps = ((b.0 - a.0).hypot(b.1 - a.1) / (GRID * 0.4))
        .ceil()
        .max(1.0) as usize;
    (0..=steps).all(|i| {
        let t = i as f32 / steps as f32;
        !blocked[cell((a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t))]
    })
}

/// Water routes avoid every unrelated island, including the edge of its sand bank.
fn route_bridges(islands: &[Island]) -> Vec<Bridge> {
    use std::{cmp::Reverse, collections::BinaryHeap};
    let mut traffic = vec![0u32; GRID_W * GRID_H];
    let mut bridges = Vec::new();
    for (child, island) in islands.iter().enumerate().skip(1) {
        let parent = island.parent.unwrap();
        let start = (islands[parent].x, islands[parent].y);
        let end = (island.x, island.y);
        let mut blocked = vec![false; GRID_W * GRID_H];
        for (i, value) in blocked.iter_mut().enumerate() {
            let p = point(i);
            *value = p.0 < 20.0
                || p.0 > 1260.0
                || p.1 < 95.0
                || p.1 > 630.0
                || islands.iter().enumerate().any(|(index, n)| {
                    index != parent
                        && index != child
                        && ((p.0 - n.x) / (n.radius + 16.0)).powi(2)
                            + ((p.1 - n.y) / (n.radius * 0.45 + 14.0)).powi(2)
                            < 1.0
                });
        }
        let source = cell(start);
        let target = cell(end);
        blocked[source] = false;
        blocked[target] = false;
        let heuristic = |index: usize| {
            let dx = (index % GRID_W).abs_diff(target % GRID_W) as u32;
            let dy = (index / GRID_W).abs_diff(target / GRID_W) as u32;
            10 * dx.max(dy) + 4 * dx.min(dy)
        };
        let mut costs = vec![u32::MAX; GRID_W * GRID_H];
        let mut previous = vec![usize::MAX; GRID_W * GRID_H];
        let mut open = BinaryHeap::new();
        costs[source] = 0;
        open.push(Reverse((heuristic(source), 0, source)));
        while let Some(Reverse((_, cost, index))) = open.pop() {
            if index == target {
                break;
            }
            if cost != costs[index] {
                continue;
            }
            let x = index % GRID_W;
            let y = index / GRID_W;
            for dy in -1isize..=1 {
                for dx in -1isize..=1 {
                    if dx == 0 && dy == 0 {
                        continue;
                    }
                    let nx = x as isize + dx;
                    let ny = y as isize + dy;
                    if nx < 0 || ny < 0 || nx >= GRID_W as isize || ny >= GRID_H as isize {
                        continue;
                    }
                    let next = ny as usize * GRID_W + nx as usize;
                    if blocked[next] {
                        continue;
                    }
                    if dx != 0
                        && dy != 0
                        && (blocked[y * GRID_W + nx as usize] || blocked[ny as usize * GRID_W + x])
                    {
                        continue;
                    }
                    let next_cost = cost + if dx == 0 || dy == 0 { 10 } else { 14 } + traffic[next];
                    if next_cost < costs[next] {
                        costs[next] = next_cost;
                        previous[next] = index;
                        open.push(Reverse((next_cost + heuristic(next), next_cost, next)));
                    }
                }
            }
        }
        let mut points = vec![end];
        if costs[target] != u32::MAX {
            let mut reverse = vec![target];
            let mut cursor = target;
            while cursor != source {
                cursor = previous[cursor];
                reverse.push(cursor);
            }
            reverse.reverse();
            for &index in &reverse {
                traffic[index] = traffic[index].saturating_add(35);
            }
            let raw: Vec<_> = reverse.into_iter().map(point).collect();
            points = vec![start];
            let mut cursor = 0;
            while cursor + 1 < raw.len() {
                let next = (cursor + 1..raw.len())
                    .rev()
                    .find(|&i| line_clear(raw[cursor], raw[i], &blocked))
                    .unwrap_or(cursor + 1);
                points.push(raw[next]);
                cursor = next;
            }
            *points.last_mut().unwrap() = end;
        } else {
            points.insert(0, start);
        }
        bridges.push(Bridge {
            parent,
            child,
            sprite: (hash(&island.directory) % 16) as usize,
            points,
        });
    }
    bridges
}

pub fn path_position(path: &str, radius: f32) -> (f32, f32) {
    let seed = hash(path);
    let angle = (seed % 10000) as f32 / 10000.0 * std::f32::consts::TAU;
    let distance = (((seed >> 24) % 10000) as f32 / 10000.0).sqrt() * radius * 0.72;
    (angle.cos() * distance, angle.sin() * distance * 0.36)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bridge_routes_around_an_unrelated_island() {
        let islands = vec![
            Island {
                directory: ".".into(),
                depth: 0,
                parent: None,
                x: 110.0,
                y: 250.0,
                radius: 60.0,
            },
            Island {
                directory: "child".into(),
                depth: 1,
                parent: Some(0),
                x: 500.0,
                y: 250.0,
                radius: 40.0,
            },
            Island {
                directory: "obstacle".into(),
                depth: 1,
                parent: Some(0),
                x: 300.0,
                y: 250.0,
                radius: 70.0,
            },
        ];
        let bridges = route_bridges(&islands);
        let route = &bridges[0].points;
        assert_eq!(route.first(), Some(&(110.0, 250.0)));
        assert_eq!(route.last(), Some(&(500.0, 250.0)));
        assert!(route.len() > 2);
        for pair in route.windows(2) {
            for i in 0..=100 {
                let t = i as f32 / 100.0;
                let x = pair[0].0 + (pair[1].0 - pair[0].0) * t;
                let y = pair[0].1 + (pair[1].1 - pair[0].1) * t;
                assert!(((x - 300.0) / 82.0).powi(2) + ((y - 250.0) / 41.5).powi(2) >= 1.0);
            }
        }
    }
}
