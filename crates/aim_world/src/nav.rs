//! Bot navigation over the map's waypoint graph.

use crate::map::{DEFAULT_PLAYER_RADIUS, MapDef};
use aim_core::math::Vec3;

impl MapDef {
    /// Index of the closest waypoint a player at `p` can walk to in a straight line. Ties
    /// go to the lower index, so the result is deterministic.
    pub fn nearest_waypoint(&self, p: Vec3) -> Option<usize> {
        self.waypoints
            .iter()
            .enumerate()
            .filter(|(_, w)| self.walkable(p, w.position, DEFAULT_PLAYER_RADIUS))
            .map(|(i, w)| (i, (w.position - p).horizontal().length()))
            .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)))
            .map(|(i, _)| i)
    }

    /// Walking route from `from` to `to`: `from`, the shortest chain of waypoints between
    /// their nearest walkable waypoints (Dijkstra on horizontal distance), then `to`.
    /// Returns the direct segment when it is already walkable, and `None` when no route
    /// exists.
    pub fn path(&self, from: Vec3, to: Vec3) -> Option<Vec<Vec3>> {
        if self.walkable(from, to, DEFAULT_PLAYER_RADIUS) {
            return Some(vec![from, to]);
        }
        let start = self.nearest_waypoint(from)?;
        let goal = self.nearest_waypoint(to)?;
        let chain = self.shortest_chain(start, goal)?;
        let mut route = vec![from];
        route.extend(chain.iter().map(|&i| self.waypoints[i].position));
        route.push(to);
        Some(route)
    }

    /// True when every waypoint is reachable from the first one.
    pub(crate) fn nav_connected(&self) -> bool {
        if self.waypoints.is_empty() {
            return true;
        }
        let adj = self.adjacency();
        let mut seen = vec![false; adj.len()];
        let mut stack = vec![0];
        while let Some(i) = stack.pop() {
            if std::mem::replace(&mut seen[i], true) {
                continue;
            }
            stack.extend(adj[i].iter().copied().filter(|&j| !seen[j]));
        }
        seen.iter().all(|&s| s)
    }

    /// Undirected adjacency by waypoint index; unknown link ids are skipped.
    fn adjacency(&self) -> Vec<Vec<usize>> {
        let mut adj = vec![Vec::new(); self.waypoints.len()];
        for (i, w) in self.waypoints.iter().enumerate() {
            for l in &w.links {
                if let Some(j) = self.waypoints.iter().position(|o| &o.id == l) {
                    adj[i].push(j);
                    adj[j].push(i);
                }
            }
        }
        for a in &mut adj {
            a.sort_unstable();
            a.dedup();
        }
        adj
    }

    /// Dijkstra over the waypoint graph (small graphs, so a linear scan for the minimum).
    fn shortest_chain(&self, start: usize, goal: usize) -> Option<Vec<usize>> {
        let adj = self.adjacency();
        let n = adj.len();
        let mut dist = vec![f32::INFINITY; n];
        let mut prev = vec![usize::MAX; n];
        let mut done = vec![false; n];
        dist[start] = 0.0;
        loop {
            let u = (0..n)
                .filter(|&i| !done[i] && dist[i].is_finite())
                .min_by(|&a, &b| dist[a].total_cmp(&dist[b]).then(a.cmp(&b)))?;
            if u == goal {
                break;
            }
            done[u] = true;
            for &v in &adj[u] {
                let step = (self.waypoints[v].position - self.waypoints[u].position)
                    .horizontal()
                    .length();
                if dist[u] + step < dist[v] {
                    dist[v] = dist[u] + step;
                    prev[v] = u;
                }
            }
        }
        let mut chain = vec![goal];
        while let Some(&last) = chain.last()
            && last != start
        {
            chain.push(prev[last]);
        }
        chain.reverse();
        Some(chain)
    }
}

#[cfg(test)]
mod tests {
    use crate::map::MapDef;
    use aim_core::math::Vec3;

    /// A wall across the middle with an opening only on the right (x > 4).
    const SRC: &str = r#"
id = "nav"
name = "Nav"
floor_x = [-10.0, 10.0]
floor_z = [-10.0, 10.0]

[[boxes]]
kind = "wall"
min = [-10.0, 0.0, -0.2]
max = [4.0, 3.0, 0.2]

[[spawns]]
id = "p"
kind = "player"
position = [-5.0, 0.0, 5.0]

[[spawns]]
id = "b"
kind = "bot"
position = [-5.0, 0.0, -5.0]

[[waypoints]]
id = "south"
position = [-5.0, 0.0, 3.0]
links = ["gap-s"]

[[waypoints]]
id = "gap-s"
position = [7.0, 0.0, 2.0]
links = ["gap-n"]

[[waypoints]]
id = "gap-n"
position = [7.0, 0.0, -2.0]
links = ["north"]

[[waypoints]]
id = "north"
position = [-5.0, 0.0, -3.0]
"#;

    #[test]
    fn path_goes_around_the_wall_through_the_gap() {
        let m = MapDef::load(SRC).unwrap();
        let (from, to) = (m.spawns[0].position, m.spawns[1].position);
        let route = m.path(from, to).unwrap();
        assert_eq!(route.first(), Some(&from));
        assert_eq!(route.last(), Some(&to));
        assert!(route.contains(&Vec3::new(7.0, 0.0, 2.0)), "{route:?}");
        for leg in route.windows(2) {
            assert!(
                m.walkable(leg[0], leg[1], crate::DEFAULT_PLAYER_RADIUS),
                "{leg:?}"
            );
        }
    }

    #[test]
    fn direct_route_when_nothing_is_in_the_way() {
        let m = MapDef::load(SRC).unwrap();
        let (a, b) = (Vec3::new(-5.0, 0.0, 5.0), Vec3::new(5.0, 0.0, 5.0));
        assert_eq!(m.path(a, b), Some(vec![a, b]));
    }

    #[test]
    fn disconnected_graph_is_reported() {
        let src = SRC.replace("links = [\"north\"]", "");
        let errs = MapDef::from_toml(&src).unwrap().validate();
        assert!(
            errs.iter().any(|e| e.message.contains("connected")),
            "{errs:?}"
        );
    }
}
