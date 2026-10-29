use crate::{
    history::History,
    layout::{DirectoryBuilding, Layout, path_position},
    species::hash,
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug)]
struct Tween<const N: usize> {
    from: [f32; N],
    to: [f32; N],
    started: f32,
    duration: f32,
}
impl<const N: usize> Tween<N> {
    fn new(value: [f32; N]) -> Self {
        Self {
            from: value,
            to: value,
            started: 0.0,
            duration: 0.0,
        }
    }
    fn value(&self, time: f32) -> [f32; N] {
        let t = if self.duration <= 0.0 {
            1.0
        } else {
            ((time - self.started) / self.duration).clamp(0.0, 1.0)
        };
        let eased = t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
        std::array::from_fn(|i| self.from[i] + (self.to[i] - self.from[i]) * eased)
    }
    fn target(&mut self, to: [f32; N], time: f32, duration: f32, delay: f32) {
        if self.to == to {
            return;
        }
        self.from = self.value(time);
        self.to = to;
        self.started = time + delay;
        self.duration = duration;
    }
}

// Island appearance, then local growth. Exporters reserve both phases before the hold.
pub const SETTLE_FACTOR: f32 = 2.0;

#[derive(Clone, Debug)]
pub struct Plant {
    pub path: String,
    pub island: usize,
    pub x: f32,
    pub y: f32,
    pub radius: f32,
    pub opacity: f32,
    pub bytes: u64,
    pub born: usize,
}
struct PlantMotion {
    life: Tween<1>,
    island: usize,
    x: f32,
    y: f32,
    radius: f32,
    bytes: u64,
    born: usize,
}
#[derive(Clone, Debug)]
pub struct Building {
    pub directory: String,
    pub island: usize,
    pub x: f32,
    pub y: f32,
    pub radius: f32,
    pub opacity: f32,
    pub sprite: usize,
    pub file_count: usize,
}
struct BuildingMotion {
    life: Tween<1>,
    island: usize,
    radius: f32,
    building: DirectoryBuilding,
}

#[derive(Default)]
pub struct Animation {
    islands: Vec<Tween<1>>,
    bridges: Vec<Tween<1>>,
    plants: BTreeMap<String, PlantMotion>,
    buildings: BTreeMap<(usize, String), BuildingMotion>,
    last_time: Option<f32>,
}
pub struct Scene {
    pub islands: Vec<f32>,
    pub bridges: Vec<f32>,
    pub plants: Vec<Plant>,
    pub buildings: Vec<Building>,
}
impl Animation {
    pub fn update(
        &mut self,
        history: &History,
        layout: &Layout,
        position: f32,
        time: f32,
        seconds: f32,
    ) -> Scene {
        if self.last_time.is_some_and(|t| time < t) {
            *self = Self::default();
        }
        self.last_time = Some(time);
        if self.islands.len() != layout.islands.len() {
            self.islands = vec![Tween::new([0.0]); layout.islands.len()];
            self.bridges = vec![Tween::new([0.0]); layout.bridges.len()];
        }
        let index = (position.floor() as usize).min(history.snapshots.len() - 1);
        let states = &layout.snapshots[index];
        let snapshot = &history.snapshots[index];
        let duration = seconds.max(0.05);
        for (i, state) in states.iter().enumerate() {
            let target = if state.active { 1.0 } else { 0.0 };
            self.islands[i].target([target], time, duration, 0.0);
        }
        for (i, bridge) in layout.bridges.iter().enumerate() {
            let target = if states[bridge.child].active && states[bridge.parent].active {
                1.0
            } else {
                0.0
            };
            let delay = if target > 0.0 { duration * 0.45 } else { 0.0 };
            self.bridges[i].target([target], time, duration * 0.80, delay);
        }
        let mut selected = BTreeSet::new();
        let mut selected_buildings = BTreeSet::new();
        for (i, state) in states.iter().enumerate() {
            let island = &layout.islands[i];
            // Newly exposed land settles before anything grows on it.
            let ready_at = self.islands[i].started + duration * 0.75;
            let land_delay = (ready_at - time).max(0.0);
            for path in &state.plants {
                selected.insert(path.as_str());
                let file = &snapshot.files[path];
                let (dx, dy) = path_position(path, island.radius);
                let motion = self
                    .plants
                    .entry(path.clone())
                    .or_insert_with(|| PlantMotion {
                        life: Tween::new([0.0]),
                        island: i,
                        x: island.x + dx,
                        y: island.y + dy - 3.0,
                        radius: island.radius,
                        bytes: file.bytes,
                        born: file.born,
                    });
                motion.bytes = file.bytes;
                motion.born = file.born;
                let stagger = (hash(path) % 100) as f32 / 100.0 * duration * 0.20;
                motion
                    .life
                    .target([1.0], time, duration, land_delay + stagger);
            }
            for building in &state.buildings {
                let key = (i, building.directory.clone());
                selected_buildings.insert(key.clone());
                let motion = self.buildings.entry(key).or_insert_with(|| BuildingMotion {
                    life: Tween::new([0.0]),
                    island: i,
                    radius: island.radius,
                    building: building.clone(),
                });
                motion.building.file_count = building.file_count;
                motion.life.target([1.0], time, duration * 0.8, land_delay);
            }
        }
        for (path, motion) in &mut self.plants {
            if !selected.contains(path.as_str()) {
                motion.life.target([0.0], time, duration * 0.8, 0.0);
            }
        }
        for (key, motion) in &mut self.buildings {
            if !selected_buildings.contains(key) {
                motion.life.target([0.0], time, duration * 0.7, 0.0);
            }
        }
        self.plants
            .retain(|_, m| m.life.to[0] > 0.0 || m.life.value(time)[0] > 0.001);
        self.buildings
            .retain(|_, m| m.life.to[0] > 0.0 || m.life.value(time)[0] > 0.001);
        Scene {
            islands: self.islands.iter().map(|t| t.value(time)[0]).collect(),
            bridges: self.bridges.iter().map(|t| t.value(time)[0]).collect(),
            plants: self
                .plants
                .iter()
                .filter_map(|(path, motion)| {
                    let life = motion.life.value(time)[0];
                    (life > 0.001).then(|| Plant {
                        path: path.clone(),
                        island: motion.island,
                        x: motion.x,
                        y: motion.y,
                        radius: motion.radius,
                        opacity: life,
                        bytes: motion.bytes,
                        born: motion.born,
                    })
                })
                .collect(),
            buildings: self
                .buildings
                .values()
                .filter_map(|motion| {
                    let life = motion.life.value(time)[0];
                    let b = &motion.building;
                    (life > 0.001).then(|| Building {
                        directory: b.directory.clone(),
                        island: motion.island,
                        x: b.x,
                        y: b.y,
                        radius: motion.radius,
                        opacity: life,
                        sprite: b.sprite,
                        file_count: b.file_count,
                    })
                })
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rapid_commits_do_not_shorten_or_restart_an_entrance() {
        let mut t = Tween::new([0.0]);
        t.target([1.0], 0.0, 1.0, 0.0);
        for frame in 1..=30 {
            t.target([1.0], frame as f32 / 30.0, 1.0, 0.0);
        }
        assert_eq!(t.value(1.0), [1.0]);
        assert!(t.value(0.1)[0] < 0.02);
        assert!((t.value(0.5)[0] - 0.5).abs() < 0.001);
    }
    #[test]
    fn deletion_survives_the_next_commit_and_settles() {
        let h = crate::cli::demo();
        let layout = Layout::new(&h, 6, 32, 700);
        let mut a = Animation::default();
        a.update(&h, &layout, 33.0, 0.0, 1.0);
        let full = a.update(&h, &layout, 33.0, 2.0, 1.0);
        assert!(full.plants.iter().any(|p| p.path == "src/core/terrain.rs"));
        a.update(&h, &layout, 35.0, 2.01, 1.0);
        let fading = a.update(&h, &layout, 36.0, 2.3, 1.0);
        let ghost = fading
            .plants
            .iter()
            .find(|p| p.path == "src/core/terrain.rs")
            .unwrap();
        assert!(ghost.opacity > 0.5 && ghost.opacity < 1.0);
        let gone = a.update(&h, &layout, 36.0, 3.2, 1.0);
        assert!(!gone.plants.iter().any(|p| p.path == "src/core/terrain.rs"));
    }
    #[test]
    fn a_folder_fades_in_place_and_new_plants_wait_for_their_own_land() {
        let mut history = crate::cli::demo();
        let template = history.snapshots[0].files.values().next().unwrap().clone();
        history.snapshots.truncate(3);
        history.total_commits = 3;
        for (snapshot, paths) in history.snapshots.iter_mut().zip([
            vec!["README.md", "module/a.rs"],
            vec!["README.md", "module/a.rs", "module/b.rs", "module/c.rs"],
            vec!["README.md", "module/a.rs"],
        ]) {
            snapshot.files = paths
                .into_iter()
                .map(|path| (path.into(), template.clone()))
                .collect();
        }
        let layout = Layout::with_threshold(&history, 6, 32, 100, Some(2));
        let child = layout
            .islands
            .iter()
            .position(|i| i.directory == "module")
            .unwrap();
        let mut animation = Animation::default();
        animation.update(&history, &layout, 0.0, 0.0, 1.0);
        let initial = animation.update(&history, &layout, 0.0, 2.0, 1.0);
        assert!(!initial.plants.iter().any(|p| p.path == "module/a.rs"));
        let building = initial
            .buildings
            .iter()
            .find(|b| b.directory == "module")
            .unwrap();
        let old_anchor = (building.x, building.y);
        animation.update(&history, &layout, 1.0, 2.1, 1.0);
        let emerging = animation.update(&history, &layout, 1.0, 2.5, 1.0);
        assert!(!emerging.plants.iter().any(|p| p.path == "module/a.rs"));
        let fading = emerging
            .buildings
            .iter()
            .find(|b| b.directory == "module")
            .unwrap();
        assert_eq!((fading.x, fading.y), old_anchor);
        assert!(fading.opacity > 0.0 && fading.opacity < 1.0);
        let growing = animation.update(&history, &layout, 1.0, 3.3, 1.0);
        let sprout = growing
            .plants
            .iter()
            .find(|p| p.path == "module/a.rs")
            .unwrap();
        assert_eq!(sprout.island, child);
        assert!(sprout.opacity > 0.0 && sprout.opacity < 1.0);
        let anchor = (sprout.x, sprout.y);
        let settled = animation.update(&history, &layout, 1.0, 4.2, 1.0);
        let plant = settled
            .plants
            .iter()
            .find(|p| p.path == "module/a.rs")
            .unwrap();
        assert_eq!((plant.x, plant.y), anchor);
        assert_eq!(plant.opacity, 1.0);
        assert!(!settled.buildings.iter().any(|b| b.directory == "module"));
        let shrunk = animation.update(&history, &layout, 2.0, 4.3, 1.0);
        let plant = shrunk
            .plants
            .iter()
            .find(|p| p.path == "module/a.rs")
            .unwrap();
        assert_eq!((plant.x, plant.y), anchor);
        assert_eq!(plant.island, child);
    }
}
