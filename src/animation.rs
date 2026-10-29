use crate::{
    history::History,
    layout::{Layout, path_position},
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

#[derive(Clone, Debug)]
pub struct Plant {
    pub path: String,
    pub x: f32,
    pub y: f32,
    pub radius: f32,
    pub opacity: f32,
    pub bytes: u64,
    pub born: usize,
}
struct PlantMotion {
    pose: Tween<4>,
    bytes: u64,
    born: usize,
}

#[derive(Default)]
pub struct Animation {
    islands: Vec<Tween<1>>,
    bridges: Vec<Tween<1>>,
    plants: BTreeMap<String, PlantMotion>,
    last_time: Option<f32>,
}
pub struct Scene {
    pub islands: Vec<f32>,
    pub bridges: Vec<f32>,
    pub plants: Vec<Plant>,
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
            let delay = if target > 0.0 { duration * 0.16 } else { 0.0 };
            self.bridges[i].target([target], time, duration * 0.78, delay);
        }
        let mut selected = BTreeSet::new();
        for (i, state) in states.iter().enumerate() {
            let island = &layout.islands[i];
            for path in &state.plants {
                selected.insert(path.as_str());
                let file = &snapshot.files[path];
                let (dx, dy) = path_position(path, island.radius);
                let target = [island.x + dx, island.y + dy - 3.0, island.radius, 1.0];
                let motion = self
                    .plants
                    .entry(path.clone())
                    .or_insert_with(|| PlantMotion {
                        pose: Tween::new([target[0], target[1] + 6.0, target[2], 0.0]),
                        bytes: file.bytes,
                        born: file.born,
                    });
                motion.bytes = file.bytes;
                motion.born = file.born;
                motion.pose.target(target, time, duration, 0.0);
            }
        }
        for (path, motion) in &mut self.plants {
            if !selected.contains(path.as_str()) {
                let mut target = motion.pose.to;
                target[3] = 0.0;
                motion.pose.target(target, time, duration, 0.0);
            }
        }
        self.plants
            .retain(|_, motion| motion.pose.to[3] > 0.0 || motion.pose.value(time)[3] > 0.001);
        Scene {
            islands: self.islands.iter().map(|t| t.value(time)[0]).collect(),
            bridges: self.bridges.iter().map(|t| t.value(time)[0]).collect(),
            plants: self
                .plants
                .iter()
                .filter_map(|(path, motion)| {
                    let p = motion.pose.value(time);
                    (p[3] > 0.001).then(|| Plant {
                        path: path.clone(),
                        x: p[0],
                        y: p[1] + (1.0 - p[3]) * 4.0,
                        radius: p[2],
                        opacity: p[3],
                        bytes: motion.bytes,
                        born: motion.born,
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
        let full = a.update(&h, &layout, 33.0, 1.0, 1.0);
        assert!(full.plants.iter().any(|p| p.path == "src/core/terrain.rs"));
        a.update(&h, &layout, 35.0, 1.01, 1.0);
        let fading = a.update(&h, &layout, 36.0, 1.3, 1.0);
        let ghost = fading
            .plants
            .iter()
            .find(|p| p.path == "src/core/terrain.rs")
            .unwrap();
        assert!(ghost.opacity > 0.5 && ghost.opacity < 1.0);
        let gone = a.update(&h, &layout, 36.0, 2.2, 1.0);
        assert!(!gone.plants.iter().any(|p| p.path == "src/core/terrain.rs"));
    }
}
