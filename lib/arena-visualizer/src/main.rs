//! Быстрый автономный Rust-визуализатор Desert API.

use std::{
    collections::{HashMap, VecDeque},
    env, fs,
    path::PathBuf,
    sync::mpsc::{self, Receiver, Sender, TryRecvError},
    thread,
    time::{Duration, Instant},
};

use eframe::egui::{
    self, Align2, Color32, FontId, Pos2, Rect, Sense, Shape, Stroke, Vec2 as EVec2,
};
use serde::{Deserialize, Serialize};
use tungstenite::{
    Message, WebSocket, client::IntoClientRequest, http::HeaderValue, stream::MaybeTlsStream,
};

const HISTORY_SECONDS: u64 = 50;
const ADAPTIVE_RISK_STEP_MIN: f64 = 50.0;
const STANDARD_RISK_STEP_MIN: f64 = 150.0;
const RISK_FINE_RADIUS_STEPS: f64 = 3.0;
const RISK_MEDIUM_RADIUS_STEPS: f64 = 12.0;
const RISK_MEDIUM_STEP_MULTIPLIER: f64 = 2.0;
const RISK_FAR_STEP_MULTIPLIER: f64 = 4.0;
const RISK_ROUTE_FOCUS_POINTS: usize = 8;
const RISK_ROUTE_REFINEMENT_RADIUS_STEPS: f64 = 1.0;
const TOP1_RANKING_HORIZON: f64 = 15.3;

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap_or_else(|| std::path::Path::new("."))
        .to_path_buf()
}

fn anomaly_field_alpha(strength: f64) -> u8 {
    if strength.is_finite() {
        (20.0 + strength.abs() * 2.0).clamp(20.0, 92.0) as u8
    } else {
        20
    }
}

type PositionHistory = HashMap<String, VecDeque<(Instant, V)>>;
type CollectedBountyHistory = HashMap<String, VecDeque<(Instant, V)>>;

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
struct V {
    x: f64,
    y: f64,
}
fn lerp_vec(a: V, b: V, alpha: f64) -> V {
    a.add(b.sub(a).mul(alpha))
}
fn history_path(
    samples: &VecDeque<(Instant, V)>,
    start: Instant,
    end: Instant,
    endpoint: V,
) -> Vec<V> {
    let mut points = samples
        .iter()
        .filter(|(time, _)| *time >= start && *time <= end)
        .map(|(_, position)| *position)
        .collect::<Vec<_>>();
    if points
        .last()
        .is_none_or(|last| last.sub(endpoint).len() > 1e-6)
    {
        points.push(endpoint);
    }
    points
}
fn bounty_was_present(bounty: &Bounty, bounties: &[Bounty]) -> bool {
    bounties.iter().any(|candidate| {
        candidate.points == bounty.points
            && (candidate.x - bounty.x).abs() <= 1e-6
            && (candidate.y - bounty.y).abs() <= 1e-6
            && (candidate.radius - bounty.radius).abs() <= 1e-6
    })
}
fn segment_distance_to_point(start: V, end: V, point: V) -> f64 {
    let segment = end.sub(start);
    let length_squared = segment.x * segment.x + segment.y * segment.y;
    if length_squared <= 1e-12 {
        return point.sub(start).len();
    }
    let relative = point.sub(start);
    let fraction =
        ((relative.x * segment.x + relative.y * segment.y) / length_squared).clamp(0.0, 1.0);
    point.sub(start.add(segment.mul(fraction))).len()
}
fn trajectory_eta_near(
    point: V,
    route_points: &[V],
    point_times: &[f64],
    max_distance: f64,
) -> Option<f64> {
    route_points
        .windows(2)
        .enumerate()
        .filter_map(|(index, segment)| {
            let start = segment[0];
            let delta = segment[1].sub(start);
            let length_squared = delta.x * delta.x + delta.y * delta.y;
            let fraction = if length_squared <= 1e-12 {
                0.0
            } else {
                let offset = point.sub(start);
                ((offset.x * delta.x + offset.y * delta.y) / length_squared).clamp(0.0, 1.0)
            };
            let distance = point.sub(start.add(delta.mul(fraction))).len();
            if distance > max_distance {
                return None;
            }
            let start_time = *point_times.get(index)?;
            let end_time = *point_times.get(index + 1)?;
            Some((distance, start_time + (end_time - start_time) * fraction))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, time)| time)
}
fn risk_map_eta(
    point: V,
    route_radius: f64,
    routes: &[&Track],
    carpet_positions: &[V],
    max_speed: f64,
    horizon: f64,
) -> f64 {
    let route_eta = routes
        .iter()
        .filter_map(|track| {
            trajectory_eta_near(point, &track.points, &track.point_times, route_radius)
        })
        .min_by(f64::total_cmp);
    let eta = route_eta.unwrap_or_else(|| {
        carpet_positions
            .iter()
            .map(|position| point.sub(*position).len())
            .min_by(f64::total_cmp)
            .map(|distance| distance / max_speed.max(1.0))
            .unwrap_or(0.0)
    });
    eta.clamp(0.0, horizon.max(0.0))
}
fn collected_bounties_between(previous: &Desert, current: &Desert) -> Vec<(String, V, u32)> {
    let mut collected = Vec::new();
    for bounty in previous
        .bounties
        .iter()
        .filter(|bounty| !bounty_was_present(bounty, &current.bounties))
    {
        let center = V {
            x: bounty.x,
            y: bounty.y,
        };
        let capture_radius = previous.transport_radius.max(0.0) + bounty.radius.max(0.0);
        let collector = previous
            .transports
            .iter()
            .filter_map(|old| {
                let new = current.transports.iter().find(|unit| unit.id == old.id)?;
                let start = V { x: old.x, y: old.y };
                let end = V { x: new.x, y: new.y };
                let distance = segment_distance_to_point(start, end, center);
                (distance <= capture_radius).then_some((distance, old.id.clone()))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0));
        if let Some((_, carpet_id)) = collector {
            collected.push((carpet_id, center, bounty.points));
        }
    }
    collected
}
fn own_death_transitions(previous: Option<&Desert>, current: &Desert) -> u64 {
    let Some(previous) = previous else {
        return 0;
    };
    current
        .transports
        .iter()
        .filter(|current_transport| is_destroyed_status(&current_transport.status))
        .filter(|current_transport| {
            previous
                .transports
                .iter()
                .find(|old| old.id == current_transport.id)
                .is_some_and(|old| !is_destroyed_status(&old.status))
        })
        .count() as u64
}
fn is_destroyed_status(status: &str) -> bool {
    matches!(status, "dead" | "destroyed")
}
fn session_score_lost(
    starting_score: u64,
    confirmed_bounty_points: u64,
    current_score: u64,
) -> u64 {
    starting_score
        .saturating_add(confirmed_bounty_points)
        .saturating_sub(current_score)
}
fn vector_pixels(magnitude: f64, reference: f64, maximum: f32) -> Option<f32> {
    (magnitude > 1e-6).then(|| {
        ((magnitude / reference.max(1e-6) * f64::from(maximum)) as f32).clamp(8.0, maximum)
    })
}
fn world_radius_pixels(radius: f64, scale: f64) -> f32 {
    if radius.is_finite() && scale.is_finite() {
        (radius.max(0.0) * scale.max(0.0)) as f32
    } else {
        0.0
    }
}
struct ArrowStyle {
    color: Color32,
    shaft_width: f32,
    head_length: f32,
    head_half_width: f32,
}
fn draw_arrow(
    painter: &egui::Painter,
    origin: Pos2,
    direction: EVec2,
    length: f32,
    style: ArrowStyle,
) {
    if length <= 0.0 || direction.length_sq() <= 1e-6 {
        return;
    }
    let direction = direction.normalized();
    let tip = origin + direction * length;
    let base = tip - direction * style.head_length.min(length * 0.6);
    let side = EVec2::new(-direction.y, direction.x) * style.head_half_width;
    let stroke = Stroke::new(style.shaft_width, style.color);
    painter.line_segment([origin, base], stroke);
    painter.line_segment([tip, base + side], stroke);
    painter.line_segment([tip, base - side], stroke);
}
impl V {
    fn add(self, b: Self) -> Self {
        Self {
            x: self.x + b.x,
            y: self.y + b.y,
        }
    }
    fn sub(self, b: Self) -> Self {
        Self {
            x: self.x - b.x,
            y: self.y - b.y,
        }
    }
    fn mul(self, k: f64) -> Self {
        Self {
            x: self.x * k,
            y: self.y * k,
        }
    }
    fn len(self) -> f64 {
        (self.x * self.x + self.y * self.y).sqrt()
    }
    fn unit(self) -> Self {
        let n = self.len();
        if n > 1e-9 {
            self.mul(1.0 / n)
        } else {
            Self::default()
        }
    }
}
fn anomaly_overlaps_arena(center: V, radius: f64, map_size: V) -> bool {
    center.x + radius >= 0.0
        && center.y + radius >= 0.0
        && center.x - radius <= map_size.x
        && center.y - radius <= map_size.y
}
fn anomaly_force_at(anomalies: &[Anomaly], point: V, time: f64, scale: f64, map_size: V) -> V {
    anomalies.iter().fold(V::default(), |sum, anomaly| {
        let center = V {
            x: anomaly.x,
            y: anomaly.y,
        }
        .add(anomaly.velocity.mul(time));
        if !anomaly_overlaps_arena(center, anomaly.effective_radius, map_size) {
            return sum;
        }
        let delta = center.sub(point);
        let distance = delta.len();
        if distance <= anomaly.effective_radius && distance > 1e-6 {
            sum.add(delta.mul(anomaly.strength * scale / distance))
        } else {
            sum
        }
    })
}
#[derive(Clone, Copy, Debug)]
struct RiskCell {
    min: V,
    max: V,
    center: V,
    size: f64,
}

fn risk_grid_cells(
    map_size: V,
    fine_step: f64,
    carpet_focus: &[V],
    route_focus: &[V],
    adaptive: bool,
) -> Vec<RiskCell> {
    let fine_step = fine_step.max(1.0);
    let root_step = if adaptive {
        fine_step * RISK_FAR_STEP_MULTIPLIER
    } else {
        fine_step
    };
    let mut cells = Vec::new();
    let mut y = 0.0;
    while y < map_size.y {
        let mut x = 0.0;
        while x < map_size.x {
            append_risk_cells(
                &mut cells,
                V { x, y },
                root_step,
                map_size,
                fine_step,
                carpet_focus,
                route_focus,
                adaptive,
            );
            x += root_step;
        }
        y += root_step;
    }
    cells
}

fn append_risk_cells(
    cells: &mut Vec<RiskCell>,
    origin: V,
    size: f64,
    map_size: V,
    fine_step: f64,
    carpet_focus: &[V],
    route_focus: &[V],
    adaptive: bool,
) {
    if origin.x >= map_size.x || origin.y >= map_size.y {
        return;
    }
    let max = V {
        x: (origin.x + size).min(map_size.x),
        y: (origin.y + size).min(map_size.y),
    };
    let min = V {
        x: origin.x.max(0.0),
        y: origin.y.max(0.0),
    };
    let center = V {
        x: (min.x + max.x) * 0.5,
        y: (min.y + max.y) * 0.5,
    };
    let distance_to = |focus: &[V]| {
        focus
            .iter()
            .map(|point| {
                let dx = (min.x - point.x).max(0.0).max(point.x - max.x);
                let dy = (min.y - point.y).max(0.0).max(point.y - max.y);
                (dx * dx + dy * dy).sqrt()
            })
            .fold(f64::INFINITY, f64::min)
    };
    let nearest_carpet = distance_to(carpet_focus);
    let nearest_route = distance_to(route_focus);
    let desired_size = if !adaptive || nearest_carpet <= fine_step * RISK_FINE_RADIUS_STEPS {
        fine_step
    } else if nearest_route <= fine_step * RISK_ROUTE_REFINEMENT_RADIUS_STEPS {
        fine_step * RISK_MEDIUM_STEP_MULTIPLIER
    } else if nearest_carpet <= fine_step * RISK_MEDIUM_RADIUS_STEPS {
        fine_step * RISK_MEDIUM_STEP_MULTIPLIER
    } else {
        fine_step * RISK_FAR_STEP_MULTIPLIER
    };

    if size > desired_size + 1e-6 && size * 0.5 >= fine_step - 1e-6 {
        let half = size * 0.5;
        for offset_y in [0.0, half] {
            for offset_x in [0.0, half] {
                append_risk_cells(
                    cells,
                    V {
                        x: origin.x + offset_x,
                        y: origin.y + offset_y,
                    },
                    half,
                    map_size,
                    fine_step,
                    carpet_focus,
                    route_focus,
                    adaptive,
                );
            }
        }
    } else {
        cells.push(RiskCell {
            min,
            max,
            center,
            size: size.min((max.x - min.x).max(max.y - min.y)),
        });
    }
}

fn trajectory_focus_points(track: &Track, max_points: usize) -> Vec<V> {
    let count = max_points.min(track.points.len());
    if count == 0 {
        return Vec::new();
    }
    if count == 1 {
        return vec![track.points[0]];
    }
    let duration = track.point_times.last().copied().unwrap_or(0.0).max(0.0);
    let log_span = (1.0 + duration / 0.2).ln();
    let mut selected = Vec::with_capacity(count);
    let mut previous_index = usize::MAX;
    for sample in 0..count {
        let fraction = sample as f64 / (count - 1) as f64;
        let target_time = 0.2 * (fraction * log_span).exp_m1();
        if let Some((index, _)) = track
            .point_times
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| {
                (*a - target_time)
                    .abs()
                    .total_cmp(&(*b - target_time).abs())
            })
            && index != previous_index
        {
            selected.push(track.points[index]);
            previous_index = index;
        }
    }
    selected
}

fn forecast_step_for_time(time: f64, base_step: f64) -> f64 {
    let base_step = base_step.max(1e-6);
    if time <= TOP1_RANKING_HORIZON {
        base_step
    } else {
        base_step * (1.0 + (1.0 + (time - TOP1_RANKING_HORIZON) / 5.0).ln()).min(4.0)
    }
}

fn anomaly_death_risk_at(
    anomalies: &[Anomaly],
    point: V,
    time: f64,
    anomaly_scale: f64,
    max_accel: f64,
    max_speed: f64,
    transport_radius: f64,
    map_size: V,
    grid_step: f64,
) -> f64 {
    if guaranteed_death_cell(
        anomalies,
        point,
        time,
        anomaly_scale,
        max_accel,
        max_speed,
        transport_radius,
        map_size,
        grid_step,
        4,
    ) {
        return 1.0;
    }
    if point.x < 0.0 || point.y < 0.0 || point.x > map_size.x || point.y > map_size.y {
        return 1.0;
    }
    let control = max_accel.max(1e-6);
    let force = anomaly_force_at(anomalies, point, time, anomaly_scale, map_size);
    let mut risk: f64 = 0.0;
    for anomaly in anomalies.iter().filter(|anomaly| anomaly.strength > 0.0) {
        let center = V {
            x: anomaly.x,
            y: anomaly.y,
        }
        .add(anomaly.velocity.mul(time));
        if !anomaly_overlaps_arena(center, anomaly.effective_radius, map_size) {
            continue;
        }
        let delta = center.sub(point);
        let distance = delta.len();
        let lethal_radius = anomaly.radius + transport_radius;
        if distance <= lethal_radius {
            return 1.0;
        }
        if distance > anomaly.effective_radius || distance <= 1e-9 {
            continue;
        }
        let toward_core = delta.mul(1.0 / distance);
        let inward_force = (force.x * toward_core.x + force.y * toward_core.y).max(0.0);
        let pressure = inward_force / control;
        let proximity = ((anomaly.effective_radius - distance)
            / (anomaly.effective_radius - lethal_radius).max(1.0))
        .clamp(0.0, 1.0);
        let trap_risk = if pressure > 1.0 {
            if proximity >= 0.8 {
                1.0
            } else {
                0.75 + 0.25 * proximity
            }
        } else {
            (0.75 * pressure * proximity).clamp(0.0, 0.75)
        };
        risk = risk.max(trap_risk);
    }

    let edge_horizon = (max_speed.max(1.0) * 2.0).max(1.0);
    for (clearance, outward_force) in [
        (point.x, -force.x),
        (map_size.x - point.x, force.x),
        (point.y, -force.y),
        (map_size.y - point.y, force.y),
    ] {
        let closeness = (-clearance.max(0.0) / edge_horizon).exp();
        let pressure = outward_force.max(0.0) / control;
        let edge_risk = if pressure > 1.0 {
            closeness * (0.75 + 0.25 * (1.0 - (-pressure).exp()))
        } else {
            (closeness * pressure * 0.5).clamp(0.0, 0.75)
        };
        risk = risk.max(edge_risk);
    }
    risk.clamp(0.0, 0.99)
}
fn guaranteed_death_cell(
    anomalies: &[Anomaly],
    point: V,
    time: f64,
    anomaly_scale: f64,
    max_accel: f64,
    max_speed: f64,
    transport_radius: f64,
    map_size: V,
    grid_step: f64,
    chain_depth: u8,
) -> bool {
    if point.x < 0.0 || point.y < 0.0 || point.x > map_size.x || point.y > map_size.y {
        return true;
    }
    let control = max_accel.max(1e-6);
    let force = anomaly_force_at(anomalies, point, time, anomaly_scale, map_size);
    for anomaly in anomalies {
        let center = V {
            x: anomaly.x,
            y: anomaly.y,
        }
        .add(anomaly.velocity.mul(time));
        if !anomaly_overlaps_arena(center, anomaly.effective_radius, map_size) {
            continue;
        }
        let delta = center.sub(point);
        let distance = delta.len();
        if distance <= anomaly.radius + transport_radius {
            return true;
        }
        if anomaly.strength <= 0.0 || distance > anomaly.effective_radius || distance <= 1e-9 {
            continue;
        }
        let lethal_radius = anomaly.radius + transport_radius;
        let proximity = ((anomaly.effective_radius - distance)
            / (anomaly.effective_radius - lethal_radius).max(1.0))
        .clamp(0.0, 1.0);
        let inward_force = force.x * delta.x / distance + force.y * delta.y / distance;
        if inward_force > control && proximity >= 0.8 {
            return true;
        }
    }

    for (clearance, outward_force) in [
        (point.x, -force.x),
        (map_size.x - point.x, force.x),
        (point.y, -force.y),
        (map_size.y - point.y, force.y),
    ] {
        if outward_force > control && clearance <= max_speed.max(1.0) * 2.0 {
            return true;
        }
    }

    if chain_depth > 0 && force.len() > control {
        let next = point.add(force.unit().mul(grid_step.max(1.0)));
        if guaranteed_death_cell(
            anomalies,
            next,
            time,
            anomaly_scale,
            max_accel,
            max_speed,
            transport_radius,
            map_size,
            grid_step,
            chain_depth - 1,
        ) {
            return true;
        }
    }
    false
}
fn anomaly_risk_color(risk: f64) -> Color32 {
    if !risk.is_finite() || risk <= 0.015 {
        return Color32::TRANSPARENT;
    }
    let severity = risk.clamp(0.0, 1.0) as f32;
    let alpha = (24.0 + severity.sqrt() * 145.0) as u8;
    if severity >= 1.0 {
        return Color32::from_rgba_unmultiplied(220, 55, 48, alpha);
    }
    let safe = [62.0, 181.0, 105.0];
    let warning = [241.0, 145.0, 48.0];
    let channel = |index: usize| (safe[index] + (warning[index] - safe[index]) * severity) as u8;
    Color32::from_rgba_unmultiplied(channel(0), channel(1), channel(2), alpha)
}
fn anomaly_field_arrow_length(force_ratio: f64, cell_pixels: f64) -> f32 {
    if !force_ratio.is_finite() || force_ratio <= 0.0 {
        return 0.0;
    }
    let cell_limit = (cell_pixels.max(0.0) * 0.42).clamp(1.0, 16.0);
    (cell_limit * force_ratio.clamp(0.0, 1.0).sqrt()).clamp(1.0, cell_limit) as f32
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Desert {
    #[serde(default)]
    errors: Vec<String>,
    #[serde(default)]
    anomalies: Vec<Anomaly>,
    #[serde(default)]
    bounties: Vec<Bounty>,
    #[serde(default)]
    enemies: Vec<Unit>,
    #[serde(default)]
    map_size: V,
    #[serde(default)]
    max_accel: f64,
    #[serde(default)]
    max_speed: f64,
    #[serde(default)]
    name: String,
    #[serde(default)]
    points: u32,
    #[serde(default)]
    transport_radius: f64,
    #[serde(default)]
    transports: Vec<Transport>,
    #[serde(default)]
    wanted_list: Vec<Unit>,
}
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Anomaly {
    #[serde(default)]
    id: String,
    #[serde(default)]
    x: f64,
    #[serde(default)]
    y: f64,
    #[serde(default)]
    radius: f64,
    #[serde(default)]
    effective_radius: f64,
    #[serde(default)]
    strength: f64,
    #[serde(default)]
    velocity: V,
}
#[derive(Clone, Debug, Default, Deserialize)]
struct Bounty {
    #[serde(default)]
    x: f64,
    #[serde(default)]
    y: f64,
    #[serde(default)]
    radius: f64,
    #[serde(default)]
    points: u32,
}
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Transport {
    #[serde(default)]
    id: String,
    #[serde(default)]
    x: f64,
    #[serde(default)]
    y: f64,
    #[serde(default)]
    status: String,
    #[serde(default)]
    death_count: u32,
    #[serde(default)]
    velocity: V,
    #[serde(default)]
    self_acceleration: V,
    #[serde(default)]
    anomaly_acceleration: V,
}
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Unit {
    #[serde(default)]
    id: String,
    #[serde(default)]
    x: f64,
    #[serde(default)]
    y: f64,
    #[serde(default)]
    velocity: V,
    #[serde(default)]
    status: String,
}

/// Spatial index narrows per-step bounty tests to cells around the swept segment.
struct BountyGrid {
    cell_size: f64,
    max_radius: f64,
    cells: HashMap<(i32, i32), Vec<usize>>,
}
impl BountyGrid {
    fn new(bounties: &[Bounty], transport_radius: f64) -> Self {
        let max_radius = bounties
            .iter()
            .map(|b| transport_radius.max(0.0) + b.radius.max(0.0))
            .fold(transport_radius.max(0.0), f64::max);
        let cell_size = (transport_radius.max(max_radius) * 4.0).max(64.0);
        let mut cells: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
        for (index, bounty) in bounties.iter().enumerate() {
            let key = (
                (bounty.x / cell_size).floor() as i32,
                (bounty.y / cell_size).floor() as i32,
            );
            cells.entry(key).or_default().push(index);
        }
        Self {
            cell_size,
            max_radius,
            cells,
        }
    }
}

enum WorkerEvent {
    Snapshot(Box<Desert>, Instant),
    Error(String),
}
enum HubLeaderboardEvent {
    Updated(LeaderboardFile),
    Error(String),
}
enum Command {
    Manual(Option<(String, V)>),
    Stop,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RealtimeTicket {
    ticket: String,
    websocket_url: String,
}

fn connect_realtime(
    ticket: RealtimeTicket,
) -> Result<WebSocket<MaybeTlsStream<std::net::TcpStream>>, String> {
    let mut request = ticket
        .websocket_url
        .into_client_request()
        .map_err(|error| format!("invalid visualizer WebSocket URL: {error}"))?;
    let protocols = format!("stadmagic.v1, stadmagic-ticket.{}", ticket.ticket);
    request.headers_mut().insert(
        "Sec-WebSocket-Protocol",
        HeaderValue::from_str(&protocols)
            .map_err(|error| format!("invalid visualizer ticket: {error}"))?,
    );
    tungstenite::connect(request)
        .map(|(socket, _)| socket)
        .map_err(|error| format!("visualizer WebSocket connect failed: {error}"))
}

fn parse_realtime_snapshot(text: &str) -> Result<Option<(u64, Desert)>, String> {
    let message: serde_json::Value = serde_json::from_str(text)
        .map_err(|error| format!("invalid visualizer WebSocket JSON: {error}"))?;
    match message.get("type").and_then(serde_json::Value::as_str) {
        Some("snapshot") => {
            let tick = message
                .get("tick")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or_default();
            let state = message
                .get("state")
                .cloned()
                .ok_or_else(|| "visualizer snapshot has no state".to_string())?;
            let desert = serde_json::from_value(state)
                .map_err(|error| format!("invalid Desert snapshot: {error}"))?;
            Ok(Some((tick, desert)))
        }
        Some("error") => Err(message
            .get("error")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("visualizer WebSocket error")
            .to_string()),
        _ => Ok(None),
    }
}

struct Network {
    events: Receiver<WorkerEvent>,
    commands: Sender<Command>,
    leaderboard_events: Receiver<HubLeaderboardEvent>,
    leaderboard_stop: Sender<()>,
}
impl Network {
    fn start(hub_url: String, token: String) -> Self {
        let (event_tx, events) = mpsc::sync_channel(1);
        let (commands, command_rx) = mpsc::channel();
        let (leaderboard_tx, leaderboard_events) = mpsc::sync_channel(1);
        let (leaderboard_stop, leaderboard_stop_rx) = mpsc::channel();
        let ticket_hub_url = hub_url.clone();
        thread::Builder::new()
            .name("desert-api".into())
            .spawn(move || {
                let client = match reqwest::blocking::Client::builder()
                    .timeout(Duration::from_secs(3))
                    .build()
                {
                    Ok(c) => c,
                    Err(e) => {
                        let _ = event_tx.send(WorkerEvent::Error(e.to_string()));
                        return;
                    }
                };
                let mut manual: Option<(String, V)> = None;
                let mut backoff = Duration::from_millis(200);
                loop {
                    loop {
                        match command_rx.try_recv() {
                            Ok(Command::Manual(m)) => manual = m,
                            Ok(Command::Stop) => return,
                            Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
                        }
                    }
                    let ticket_endpoint = format!(
                        "{}/api/visualizer/ticket",
                        ticket_hub_url.trim_end_matches('/')
                    );
                    let ticket_result = client
                        .post(&ticket_endpoint)
                        .header("X-Auth-Token", &token)
                        .json(&serde_json::json!({}))
                        .send()
                        .and_then(|response| response.error_for_status())
                        .and_then(|response| response.json::<RealtimeTicket>());
                    let mut socket = match ticket_result {
                        Ok(ticket) => match connect_realtime(ticket) {
                            Ok(socket) => {
                                backoff = Duration::from_millis(200);
                                socket
                            }
                            Err(error) => {
                                let _ = event_tx.try_send(WorkerEvent::Error(error));
                                thread::sleep(backoff);
                                backoff = backoff.saturating_mul(2).min(Duration::from_secs(5));
                                continue;
                            }
                        },
                        Err(error) => {
                            let _ = event_tx.try_send(WorkerEvent::Error(format!(
                                "visualizer ticket request failed: {error}"
                            )));
                            thread::sleep(backoff);
                            backoff = backoff.saturating_mul(2).min(Duration::from_secs(5));
                            continue;
                        }
                    };

                    loop {
                        loop {
                            match command_rx.try_recv() {
                                Ok(Command::Manual(m)) => manual = m,
                                Ok(Command::Stop) => return,
                                Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
                            }
                        }
                        match socket.read() {
                            Ok(Message::Text(text)) => {
                                match parse_realtime_snapshot(text.as_str()) {
                                    Ok(Some((_, snapshot))) => {
                                        let _ = event_tx.try_send(WorkerEvent::Snapshot(
                                            Box::new(snapshot),
                                            Instant::now(),
                                        ));
                                        if let Some((id, acceleration)) = &manual {
                                            let command = serde_json::json!({
                                                "type": "commands",
                                                "transports": [{
                                                    "id": id,
                                                    "acceleration": {
                                                        "x": acceleration.x,
                                                        "y": acceleration.y
                                                    }
                                                }]
                                            });
                                            if let Err(error) = socket
                                                .send(Message::Text(command.to_string().into()))
                                            {
                                                let _ =
                                                    event_tx.try_send(WorkerEvent::Error(format!(
                                                        "visualizer command send failed: {error}"
                                                    )));
                                                break;
                                            }
                                        }
                                    }
                                    Ok(None) => {}
                                    Err(error) => {
                                        let _ = event_tx.try_send(WorkerEvent::Error(error));
                                    }
                                }
                            }
                            Ok(Message::Ping(payload)) => {
                                if socket.send(Message::Pong(payload)).is_err() {
                                    break;
                                }
                            }
                            Ok(Message::Close(_)) => break,
                            Ok(Message::Binary(_)) => {}
                            Ok(Message::Frame(_)) => {}
                            Ok(Message::Pong(_)) => {}
                            Err(error) => {
                                let _ = event_tx.try_send(WorkerEvent::Error(format!(
                                    "visualizer WebSocket disconnected: {error}"
                                )));
                                break;
                            }
                        }
                    }
                    thread::sleep(backoff);
                    backoff = backoff.saturating_mul(2).min(Duration::from_secs(5));
                }
            })
            .expect("network thread");
        thread::Builder::new()
            .name("hub-leaderboard-api".into())
            .spawn(move || {
                let client = match reqwest::blocking::Client::builder()
                    .timeout(Duration::from_secs(3))
                    .build()
                {
                    Ok(client) => client,
                    Err(error) => {
                        let _ = leaderboard_tx.send(HubLeaderboardEvent::Error(error.to_string()));
                        return;
                    }
                };
                let endpoint = format!(
                    "{}/api/leaderboard?scope=all",
                    hub_url.trim_end_matches('/')
                );
                loop {
                    let result = client
                        .get(&endpoint)
                        .send()
                        .and_then(|response| response.error_for_status())
                        .and_then(|response| response.json::<LeaderboardFile>());
                    match result {
                        Ok(mut board) => {
                            board.updated_at_unix_ms = unix_ms();
                            let _ = leaderboard_tx.try_send(HubLeaderboardEvent::Updated(board));
                        }
                        Err(error) => {
                            let _ = leaderboard_tx
                                .try_send(HubLeaderboardEvent::Error(error.to_string()));
                        }
                    }
                    match leaderboard_stop_rx.recv_timeout(Duration::from_secs(1)) {
                        Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => return,
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                    }
                }
            })
            .expect("hub leaderboard network thread");
        Self {
            events,
            commands,
            leaderboard_events,
            leaderboard_stop,
        }
    }
}

#[derive(Clone)]
struct Track {
    points: Vec<V>,
    point_times: Vec<f64>,
    score: u32,
    last_time: f64,
    rank_score: u32,
    rank_last_time: f64,
    rank_first_bounty_time: Option<f64>,
    rank_death_time: Option<f64>,
    rank_speed_integral: f64,
    rank_speed_duration: f64,
    rank_terminal_speed: f64,
    duration: f64,
    death: Option<V>,
    angle: f64,
    current: bool,
    rank: usize,
    rank_total: usize,
    top1: bool,
}
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlayerTelemetry {
    updated_at_unix_ms: u64,
    tick: u64,
    aim_strategy: String,
    movement_strategy: String,
    doom_policy: String,
    horizon_seconds: f64,
    plan_ms: f64,
    cycle_ms: f64,
    rtt_ms: f64,
    trajectory_evaluations: usize,
    carpets: Vec<PlayerCarpetTelemetry>,
}
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlayerCarpetTelemetry {
    id: String,
    alive: bool,
    position: V,
    velocity: V,
    speed: f64,
    current_acceleration: V,
    anomaly_acceleration: V,
    command_acceleration: Option<V>,
    goal: Option<String>,
    target: Option<V>,
    target_points: Option<f64>,
    target_distance: Option<f64>,
    total_potential_score: Option<f64>,
    route_score: Option<f64>,
    time_to_last_bounty: Option<f64>,
    score_rate: Option<f64>,
    first_bounty_rate: Option<f64>,
    time_to_reach_score: Option<f64>,
    bounty_count: usize,
    death_at: Option<f64>,
    death_reason: Option<String>,
    risk_exposure: Option<f64>,
    movement_decision: Option<String>,
}
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
struct LeaderboardFile {
    updated_at_unix_ms: u128,
    #[serde(default)]
    teams: Vec<LeaderboardTeam>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
struct LeaderboardMetrics {
    gold: u64,
    gold_collected: u64,
    carpets_lost: u64,
    distance_travelled: f64,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
struct WorldInfo {
    world_id: String,
    world_number: usize,
    name: String,
    arena_name: String,
    description: String,
    tick_rate_ms: u64,
    arena_width: f64,
    arena_height: f64,
    max_acceleration: f64,
    max_velocity: f64,
    friction: f64,
    anomaly_quota: usize,
    bounty_quota: usize,
    anomaly_speed_min: f64,
    anomaly_speed_max: f64,
    anomaly_core_radius_min: f64,
    anomaly_core_radius_max: f64,
    anomaly_effect_radius_min: f64,
    anomaly_effect_radius_max: f64,
    anomaly_force_min: f64,
    anomaly_force_max: f64,
    anomaly_force_outlier_probability: f64,
    anomaly_force_outlier_min: f64,
    anomaly_force_outlier_max: f64,
    bounty_base_value: u32,
    bounty_max_value: u32,
}
#[derive(Clone, Debug, Default, Deserialize)]
struct LeaderboardTeam {
    rank: usize,
    team_id: String,
    name: String,
    attempts: usize,
    top: LeaderboardMetrics,
    total: LeaderboardMetrics,
}

#[derive(Clone, Copy)]
enum ForecastCommandMode {
    HoldCurrent,
    CandidateAfterLatency,
}
#[derive(Clone, Copy)]
struct ForecastSettings {
    horizon: f64,
    time_step: f64,
    anomaly_scale: f64,
    command_mode: ForecastCommandMode,
}
fn forecast(
    world: &Desert,
    bounty_grid: &BountyGrid,
    start: &Transport,
    radius: f64,
    direction: f64,
    settings: ForecastSettings,
) -> Track {
    let accel = world.max_accel.max(0.0);
    let mut p = V {
        x: start.x,
        y: start.y,
    };
    let mut vel = start.velocity;
    let mut time = 0.0;
    let mut points = vec![p];
    let mut point_times = vec![0.0];
    let mut got = vec![false; world.bounties.len()];
    let mut score = 0;
    let mut last_time = 0.0_f64;
    let mut rank_score = 0;
    let mut rank_last_time = 0.0_f64;
    let mut rank_first_bounty_time = None;
    let mut rank_death_time = None;
    let mut rank_speed_integral = 0.0;
    let mut rank_speed_duration = 0.0;
    let mut rank_terminal_speed = vel.len();
    let mut death = None;
    while time < settings.horizon {
        let mut dt = forecast_step_for_time(time, settings.time_step).min(settings.horizon - time);
        if matches!(
            settings.command_mode,
            ForecastCommandMode::CandidateAfterLatency
        ) && time < 0.3
            && time + dt > 0.3
        {
            dt = 0.3 - time;
        }
        if time < TOP1_RANKING_HORIZON && time + dt > TOP1_RANKING_HORIZON {
            dt = TOP1_RANKING_HORIZON - time;
        }
        let a = V {
            x: direction.cos() * accel,
            y: direction.sin() * accel,
        };
        let command = match settings.command_mode {
            ForecastCommandMode::HoldCurrent => start.self_acceleration,
            ForecastCommandMode::CandidateAfterLatency if time < 0.3 => start.self_acceleration,
            ForecastCommandMode::CandidateAfterLatency => a,
        };
        let environmental = anomaly_force_at(
            &world.anomalies,
            p,
            time,
            settings.anomaly_scale,
            world.map_size,
        );
        // Keep the server/player_2 order: friction first, then both accelerations.
        let previous_velocity = vel;
        vel = vel
            .mul(0.98_f64.powf(dt / 0.2))
            .add(command.add(environmental).mul(dt));
        let speed = vel.len();
        if speed > world.max_speed && world.max_speed > 0.0 {
            vel = vel.mul(world.max_speed / speed);
        }
        let next = p.add(vel.mul(dt));
        let step_start_time = time;
        time += dt;
        let mut death_fraction: Option<f64> = None;
        if next.x < 0.0 || next.y < 0.0 || next.x > world.map_size.x || next.y > world.map_size.y {
            let delta = next.sub(p);
            for (outside, fraction) in [
                (
                    next.x < 0.0,
                    if delta.x.abs() > 1e-12 {
                        Some((0.0 - p.x) / delta.x)
                    } else {
                        None
                    },
                ),
                (
                    next.x > world.map_size.x,
                    if delta.x.abs() > 1e-12 {
                        Some((world.map_size.x - p.x) / delta.x)
                    } else {
                        None
                    },
                ),
                (
                    next.y < 0.0,
                    if delta.y.abs() > 1e-12 {
                        Some((0.0 - p.y) / delta.y)
                    } else {
                        None
                    },
                ),
                (
                    next.y > world.map_size.y,
                    if delta.y.abs() > 1e-12 {
                        Some((world.map_size.y - p.y) / delta.y)
                    } else {
                        None
                    },
                ),
            ] {
                if outside && let Some(f) = fraction {
                    death_fraction = Some(death_fraction.map_or(f, |old| old.min(f)));
                }
            }
        }
        for an in &world.anomalies {
            let a0 = V { x: an.x, y: an.y }.add(an.velocity.mul(time - dt));
            let a1 = V { x: an.x, y: an.y }.add(an.velocity.mul(time));
            if let Some(u) = segment_circle_hit(p.sub(a0), next.sub(a1), an.radius + radius) {
                death_fraction = Some(death_fraction.map_or(u, |old| old.min(u)));
            }
        }
        for other in world
            .transports
            .iter()
            .filter(|other| other.id != start.id && other.status != "destroyed")
        {
            let other_start = V {
                x: other.x,
                y: other.y,
            }
            .add(other.velocity.mul(time - dt));
            let other_end = V {
                x: other.x,
                y: other.y,
            }
            .add(other.velocity.mul(time));
            if let Some(u) =
                segment_circle_hit(p.sub(other_start), next.sub(other_end), radius * 2.0)
            {
                death_fraction = Some(death_fraction.map_or(u, |old| old.min(u)));
            }
        }
        for other in &world.enemies {
            let other_start = V {
                x: other.x,
                y: other.y,
            }
            .add(other.velocity.mul(time - dt));
            let other_end = V {
                x: other.x,
                y: other.y,
            }
            .add(other.velocity.mul(time));
            if let Some(u) =
                segment_circle_hit(p.sub(other_start), next.sub(other_end), radius * 2.0)
            {
                death_fraction = Some(death_fraction.map_or(u, |old| old.min(u)));
            }
        }
        let segment_end = death_fraction
            .map(|u| p.add(next.sub(p).mul(u)))
            .unwrap_or(next);
        let death_time = death_fraction.map(|fraction| step_start_time + dt * fraction);
        let alive_step_duration = death_fraction.map_or(dt, |fraction| dt * fraction);
        let rank_duration =
            alive_step_duration.min((TOP1_RANKING_HORIZON - step_start_time).max(0.0));
        if rank_duration > 0.0 {
            let rank_fraction = rank_duration / dt;
            let rank_end_velocity =
                previous_velocity.add(vel.sub(previous_velocity).mul(rank_fraction));
            rank_speed_integral +=
                (previous_velocity.len() + rank_end_velocity.len()) * 0.5 * rank_duration;
            rank_speed_duration += rank_duration;
            rank_terminal_speed = rank_end_velocity.len();
        }
        if let Some(at) = death_time.filter(|at| *at <= TOP1_RANKING_HORIZON) {
            rank_death_time = Some(at);
        }
        let pad = bounty_grid.max_radius;
        let min_x = ((p.x.min(segment_end.x) - pad) / bounty_grid.cell_size).floor() as i32;
        let max_x = ((p.x.max(segment_end.x) + pad) / bounty_grid.cell_size).floor() as i32;
        let min_y = ((p.y.min(segment_end.y) - pad) / bounty_grid.cell_size).floor() as i32;
        let max_y = ((p.y.max(segment_end.y) + pad) / bounty_grid.cell_size).floor() as i32;
        for cell_x in min_x..=max_x {
            for cell_y in min_y..=max_y {
                if let Some(indices) = bounty_grid.cells.get(&(cell_x, cell_y)) {
                    for &i in indices {
                        if got[i] {
                            continue;
                        }
                        let b = &world.bounties[i];
                        let center = V { x: b.x, y: b.y };
                        if let Some(u) = segment_circle_hit(
                            p.sub(center),
                            segment_end.sub(center),
                            radius.max(0.0) + b.radius.max(0.0),
                        ) {
                            got[i] = true;
                            score += b.points;
                            let bounty_time = step_start_time
                                + dt * death_fraction.map_or(u, |death_u| u * death_u);
                            last_time = last_time.max(bounty_time);
                            if bounty_time <= TOP1_RANKING_HORIZON {
                                rank_score += b.points;
                                rank_last_time = rank_last_time.max(bounty_time);
                                rank_first_bounty_time = Some(
                                    rank_first_bounty_time
                                        .map_or(bounty_time, |first: f64| first.min(bounty_time)),
                                );
                            }
                        }
                    }
                }
            }
        }
        p = segment_end;
        if let Some(at) = death_time {
            death = Some(segment_end);
            time = at;
        }
        points.push(p);
        point_times.push(time);
        if death.is_some() {
            break;
        }
    }
    Track {
        points,
        point_times,
        score,
        last_time,
        rank_score,
        rank_last_time,
        rank_first_bounty_time,
        rank_death_time,
        rank_speed_integral,
        rank_speed_duration,
        rank_terminal_speed,
        duration: time,
        death,
        angle: direction,
        current: false,
        rank: 0,
        rank_total: 0,
        top1: false,
    }
}
fn segment_circle_hit(a: V, b: V, radius: f64) -> Option<f64> {
    let d = b.sub(a);
    let c = a.x * a.x + a.y * a.y - radius * radius;
    if c <= 0.0 {
        return Some(0.0);
    }
    let aa = d.x * d.x + d.y * d.y;
    if aa <= 1e-12 {
        return None;
    }
    let bb = 2.0 * (a.x * d.x + a.y * d.y);
    let disc = bb * bb - 4.0 * aa * c;
    if disc < 0.0 {
        return None;
    }
    let t = (-bb - disc.sqrt()) / (2.0 * aa);
    (0.0..=1.0).contains(&t).then_some(t)
}

struct App {
    network: Network,
    hub_url: String,
    world: Option<Desert>,
    previous: Option<Desert>,
    received: Option<Instant>,
    previous_received: Option<Instant>,
    position_history: PositionHistory,
    collected_bounty_history: CollectedBountyHistory,
    last_error: String,
    player_telemetry: Option<PlayerTelemetry>,
    player_telemetry_path: PathBuf,
    player_telemetry_checked: Instant,
    team_name_path: PathBuf,
    team_name_override: String,
    team_name_edit: String,
    own_team_name: Option<String>,
    own_team_id: String,
    leaderboard: Option<LeaderboardFile>,
    leaderboard_error: Option<String>,
    leaderboard_checked: Instant,
    show_leaderboard: bool,
    world_status_path: PathBuf,
    world_info: Option<WorldInfo>,
    world_info_checked: Instant,
    show_world_info: bool,
    camera: V,
    zoom: f64,
    selected: Option<String>,
    follow: bool,
    manual: bool,
    show_current: bool,
    show_profitable: bool,
    show_death: bool,
    show_anomaly_field: bool,
    show_anomaly_zones: bool,
    anomaly_field_spacing: f64,
    standard_risk_spacing: f64,
    adaptive_risk_grid: bool,
    scan_step: f64,
    horizon: f64,
    simulation_step: f64,
    anomaly_scale: f64,
    show_all_routes: bool,
    tracks: Vec<Track>,
    track_key: (u64, String, u64),
    tick: u64,
    own_deaths: u64,
    session_starting_score: Option<u64>,
    session_bounty_points: u64,
    session_bounties_collected: u64,
    gold_lost: u64,
    frame_at: Instant,
    fps: f32,
    poll: Duration,
    lease_path: PathBuf,
    lease_id: String,
    lease_refresh: Instant,
    lease_active: bool,
}
impl App {
    fn set_manual(&mut self, enabled: bool) {
        self.manual = enabled && self.selected.is_some();
        if !self.manual {
            let _ = self.network.commands.send(Command::Manual(None));
            self.clear_manual_lease();
            self.lease_active = false;
        }
    }

    fn new(hub_url: String, token: String, poll: Duration) -> Self {
        let hash = token_hash(&token);
        let own_team_id = format!("{hash:016x}");
        let hub_url = normalize_hub_url(hub_url);
        let player_telemetry_path = env::var_os("DATS_PLAYER_TELEMETRY_FILE")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                env::temp_dir().join(format!("datsmagic_player2_telemetry_{hash:016x}.json"))
            });
        let lease_path = env::var_os("DATS_MANUAL_CONTROL_FILE")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                repository_root().join(format!(
                    "lib/arena-bots/rust_bytadaniel/manual_control_{hash:016x}.json"
                ))
            });
        let lease_id = format!("visualizer2-{}-{}", std::process::id(), unix_ms());
        let world_status_path = env::var_os("DATS_WORLD_STATUS_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|| repository_root().join("modules/arena-hub/data/current_world.json"));
        let team_name_path = env::var_os("DATS_TEAM_NAME_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                env::temp_dir().join(format!("datsmagic_team_name_{hash:016x}.txt"))
            });
        let team_name_override = fs::read_to_string(&team_name_path)
            .unwrap_or_default()
            .trim()
            .to_owned();
        Self {
            network: Network::start(hub_url.clone(), token),
            hub_url,
            world: None,
            previous: None,
            received: None,
            previous_received: None,
            position_history: HashMap::new(),
            collected_bounty_history: HashMap::new(),
            last_error: String::new(),
            player_telemetry: None,
            player_telemetry_path,
            player_telemetry_checked: Instant::now() - Duration::from_secs(1),
            team_name_path,
            team_name_edit: team_name_override.clone(),
            team_name_override,
            own_team_name: None,
            own_team_id,
            leaderboard: None,
            leaderboard_error: None,
            leaderboard_checked: Instant::now() - Duration::from_secs(1),
            show_leaderboard: false,
            world_status_path,
            world_info: None,
            world_info_checked: Instant::now() - Duration::from_secs(1),
            show_world_info: false,
            camera: V::default(),
            zoom: 1.,
            selected: None,
            follow: false,
            manual: false,
            show_current: true,
            show_profitable: true,
            show_death: true,
            show_anomaly_field: false,
            show_anomaly_zones: true,
            anomaly_field_spacing: ADAPTIVE_RISK_STEP_MIN,
            standard_risk_spacing: STANDARD_RISK_STEP_MIN,
            adaptive_risk_grid: true,
            scan_step: 1.,
            horizon: 75.,
            simulation_step: 0.2,
            anomaly_scale: 1.,
            show_all_routes: false,
            tracks: vec![],
            track_key: (u64::MAX, String::new(), 0),
            tick: 0,
            own_deaths: 0,
            session_starting_score: None,
            session_bounty_points: 0,
            session_bounties_collected: 0,
            gold_lost: 0,
            frame_at: Instant::now(),
            fps: 0.,
            poll,
            lease_path,
            lease_id,
            lease_refresh: Instant::now(),
            lease_active: false,
        }
    }
    fn update_network(&mut self) {
        while let Ok(event) = self.network.leaderboard_events.try_recv() {
            self.leaderboard_checked = Instant::now();
            match event {
                HubLeaderboardEvent::Updated(board) => {
                    self.own_team_name = board
                        .teams
                        .iter()
                        .find(|team| team.team_id == self.own_team_id)
                        .map(|team| team.name.clone());
                    self.leaderboard = Some(board);
                    self.leaderboard_error = None;
                }
                HubLeaderboardEvent::Error(error) => self.leaderboard_error = Some(error),
            }
        }
        if self.world_info_checked.elapsed() >= Duration::from_secs(1) {
            self.world_info_checked = Instant::now();
            self.world_info = fs::read(&self.world_status_path)
                .ok()
                .and_then(|bytes| serde_json::from_slice(&bytes).ok());
        }
        if self.player_telemetry_checked.elapsed() >= Duration::from_millis(200) {
            self.player_telemetry_checked = Instant::now();
            self.player_telemetry = fs::read(&self.player_telemetry_path)
                .ok()
                .and_then(|bytes| serde_json::from_slice(&bytes).ok());
        }
        while let Ok(event) = self.network.events.try_recv() {
            match event {
                WorkerEvent::Snapshot(w, t) => {
                    if self.world.is_none() {
                        self.camera = V {
                            x: w.map_size.x * 0.5,
                            y: w.map_size.y * 0.5,
                        };
                        self.selected = w.transports.first().map(|t| t.id.clone());
                    }
                    let cutoff = t - Duration::from_secs(HISTORY_SECONDS);
                    self.session_starting_score
                        .get_or_insert_with(|| u64::from(w.points));
                    let server_reported_deaths: u64 = w
                        .transports
                        .iter()
                        .map(|transport| u64::from(transport.death_count))
                        .sum();
                    self.own_deaths = server_reported_deaths
                        .max(self.own_deaths + own_death_transitions(self.world.as_ref(), &w));
                    if let Some(previous_world) = self.world.as_ref() {
                        for (carpet_id, position, points) in
                            collected_bounties_between(previous_world, &w)
                        {
                            self.session_bounty_points += u64::from(points);
                            self.session_bounties_collected += 1;
                            self.collected_bounty_history
                                .entry(carpet_id)
                                .or_default()
                                .push_back((t, position));
                        }
                    }
                    self.gold_lost = session_score_lost(
                        self.session_starting_score.unwrap_or_default(),
                        self.session_bounty_points,
                        u64::from(w.points),
                    );
                    self.collected_bounty_history.retain(|_, history| {
                        while history.front().is_some_and(|sample| sample.0 < cutoff) {
                            history.pop_front();
                        }
                        !history.is_empty()
                    });
                    for transport in &w.transports {
                        let history = self
                            .position_history
                            .entry(transport.id.clone())
                            .or_default();
                        history.push_back((
                            t,
                            V {
                                x: transport.x,
                                y: transport.y,
                            },
                        ));
                        while history.len() > 2
                            && history.get(1).is_some_and(|sample| sample.0 <= cutoff)
                        {
                            history.pop_front();
                        }
                    }
                    self.previous_received = self.received;
                    self.previous = self.world.replace(*w);
                    self.received = Some(t);
                    self.tick += 1;
                    self.last_error.clear();
                }
                WorkerEvent::Error(e) => self.last_error = e,
            }
        }
    }
    fn current(&self) -> Option<&Transport> {
        self.world
            .as_ref()?
            .transports
            .iter()
            .find(|t| Some(&t.id) == self.selected.as_ref())
    }
    fn interpolation_alpha(&self, now: Instant) -> f64 {
        let (Some(t0), Some(t1)) = (self.previous_received, self.received) else {
            return 1.0;
        };
        let span = t1.duration_since(t0).as_secs_f64();
        if span <= 1e-6 {
            return 1.0;
        }
        let render_elapsed =
            now.saturating_duration_since(t1).as_secs_f64() - self.poll.as_secs_f64() + span;
        (render_elapsed / span).clamp(0.0, 1.0)
    }
    fn render_time(&self, now: Instant) -> Instant {
        let target = now.checked_sub(self.poll).unwrap_or(now);
        match (self.previous_received, self.received) {
            (Some(start), Some(end)) => target.max(start).min(end),
            _ => self.received.unwrap_or(target),
        }
    }
    fn transport_position(&self, t: &Transport, alpha: f64) -> V {
        let latest = V { x: t.x, y: t.y };
        self.previous
            .as_ref()
            .and_then(|old| old.transports.iter().find(|p| p.id == t.id))
            .map(|p| lerp_vec(V { x: p.x, y: p.y }, latest, alpha))
            .unwrap_or(latest)
    }
    fn recalc(&mut self) {
        let Some(w) = self.world.as_ref() else { return };
        let Some(t) = self.current().cloned() else {
            return;
        };
        let selected = t.id.clone();
        let reserved_targets = self
            .player_telemetry
            .as_ref()
            .filter(|telemetry| {
                telemetry.aim_strategy == "agile-top1"
                    && unix_ms().saturating_sub(u128::from(telemetry.updated_at_unix_ms)) <= 2000
            })
            .map(|telemetry| {
                telemetry
                    .carpets
                    .iter()
                    .filter(|carpet| carpet.alive && carpet.id.as_str() < selected.as_str())
                    .filter_map(|carpet| carpet.target)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let mut forecast_world = w.clone();
        forecast_world.bounties = w
            .bounties
            .iter()
            .filter(|bounty| {
                let position = V {
                    x: bounty.x,
                    y: bounty.y,
                };
                reserved_targets
                    .iter()
                    .all(|target| position.sub(*target).len() >= 1.0)
            })
            .cloned()
            .collect::<Vec<_>>();
        let bounty_grid = BountyGrid::new(&forecast_world.bounties, w.transport_radius);
        let mut config_key = self.scan_step.to_bits();
        for value in [self.horizon, self.simulation_step, self.anomaly_scale] {
            config_key = config_key.rotate_left(9) ^ value.to_bits();
        }
        for target in &reserved_targets {
            config_key = config_key.rotate_left(9) ^ target.x.to_bits() ^ target.y.to_bits();
        }
        let key = (self.tick, selected.clone(), config_key);
        if key == self.track_key {
            return;
        }
        let mut all = Vec::new();
        let direction_count = (360.0 / self.scan_step).ceil() as usize;
        for direction_index in 0..direction_count {
            let angle = (direction_index as f64 * self.scan_step).to_radians();
            all.push(forecast(
                &forecast_world,
                &bounty_grid,
                &t,
                w.transport_radius,
                angle,
                ForecastSettings {
                    horizon: self.horizon,
                    time_step: self.simulation_step,
                    anomaly_scale: self.anomaly_scale,
                    command_mode: ForecastCommandMode::CandidateAfterLatency,
                },
            ));
        }
        let current_angle = t.velocity.y.atan2(t.velocity.x);
        let mut best = all
            .iter()
            .filter(|track| track.rank_death_time.is_none() && track.rank_score > 0)
            .cloned()
            .collect::<Vec<_>>();
        best.sort_by(|a, b| compare_rank(b, a));
        if best.is_empty() {
            let mut safe_empty = all
                .iter()
                .cloned()
                .filter(|track| track.rank_death_time.is_none())
                .collect::<Vec<_>>();
            safe_empty.sort_by(|a, b| compare_rank(b, a));
            if let Some(best_safe) = safe_empty.into_iter().next() {
                best.push(best_safe);
            } else {
                let mut dying = all
                    .iter()
                    .cloned()
                    .filter(|track| track.rank_death_time.is_some())
                    .collect::<Vec<_>>();
                dying.sort_by(|a, b| compare_rank(b, a));
                best = dying;
            }
        }
        let rank_total = best.len();
        for (rank, track) in best.iter_mut().enumerate() {
            track.rank = rank + 1;
            track.rank_total = rank_total;
        }
        let mut active = forecast(
            &forecast_world,
            &bounty_grid,
            &t,
            w.transport_radius,
            current_angle,
            ForecastSettings {
                horizon: self.horizon,
                time_step: self.simulation_step,
                anomaly_scale: self.anomaly_scale,
                command_mode: ForecastCommandMode::HoldCurrent,
            },
        );
        active.angle = current_angle;
        active.current = true;
        if let Some((rank, total)) = current_route_rank(&active, &best) {
            active.rank = rank;
            active.rank_total = total;
            active.top1 = active.rank == 1;
        }
        best.insert(0, active);
        self.tracks = best;
        self.track_key = key;
    }
    fn render_world(&mut self, ui: &mut egui::Ui, rect: Rect) {
        let now = Instant::now();
        let alpha = self.interpolation_alpha(now);
        let render_time = self.render_time(now);
        if self.follow
            && let Some(position) = self.current().map(|t| self.transport_position(t, alpha))
        {
            self.camera = position;
        }
        let Some(w) = self.world.as_ref() else { return };
        let painter = ui.painter_at(rect);
        let world_size = w.map_size;
        let s = (rect.width() / world_size.x.max(1.) as f32)
            .min(rect.height() / world_size.y.max(1.) as f32)
            * self.zoom as f32;
        let camera = self.camera;
        let to_screen = |p: V| {
            Pos2::new(
                rect.center().x + ((p.x - camera.x) * s as f64) as f32,
                rect.center().y - ((p.y - camera.y) * s as f64) as f32,
            )
        };
        painter.rect_filled(rect, 0., Color32::from_rgb(232, 237, 242));
        let arena = Rect::from_min_max(
            to_screen(V {
                x: 0.0,
                y: world_size.y,
            }),
            to_screen(V {
                x: world_size.x,
                y: 0.0,
            }),
        );
        painter.rect_filled(arena, 0., Color32::from_rgb(248, 250, 252));
        painter.rect_stroke(
            arena,
            0.,
            Stroke::new(1., Color32::from_rgb(120, 135, 150)),
            egui::StrokeKind::Inside,
        );
        if self.show_anomaly_field && self.anomaly_scale > 0.0 {
            let field_painter = painter.with_clip_rect(arena.intersect(rect));
            let field_anomalies = w
                .anomalies
                .iter()
                .map(|anomaly| {
                    let mut sample = anomaly.clone();
                    if let Some(previous) = self
                        .previous
                        .as_ref()
                        .and_then(|old| old.anomalies.iter().find(|old| old.id == anomaly.id))
                    {
                        let position = lerp_vec(
                            V {
                                x: previous.x,
                                y: previous.y,
                            },
                            V {
                                x: anomaly.x,
                                y: anomaly.y,
                            },
                            alpha,
                        );
                        sample.x = position.x;
                        sample.y = position.y;
                        sample.velocity = lerp_vec(previous.velocity, anomaly.velocity, alpha);
                    }
                    sample
                })
                .collect::<Vec<_>>();
            let spacing = if self.adaptive_risk_grid {
                self.anomaly_field_spacing
            } else {
                self.standard_risk_spacing
            }
            .max(1.0);
            let carpet_positions = w
                .transports
                .iter()
                .filter(|carpet| !is_destroyed_status(&carpet.status))
                .map(|carpet| self.transport_position(carpet, alpha))
                .collect::<Vec<_>>();
            let route_focus = self
                .tracks
                .iter()
                .filter(|track| track.current || track.rank == 1)
                .flat_map(|track| trajectory_focus_points(track, RISK_ROUTE_FOCUS_POINTS))
                .collect::<Vec<_>>();
            let cells = risk_grid_cells(
                world_size,
                spacing,
                &carpet_positions,
                &route_focus,
                self.adaptive_risk_grid,
            );
            let forecast_routes = self
                .tracks
                .iter()
                .filter(|track| track.current || track.rank == 1)
                .collect::<Vec<_>>();
            for cell in cells {
                let point = cell.center;
                let route_eta = risk_map_eta(
                    point,
                    cell.size * std::f64::consts::FRAC_1_SQRT_2 + w.transport_radius,
                    &forecast_routes,
                    &carpet_positions,
                    w.max_speed,
                    self.horizon,
                );
                let force = anomaly_force_at(
                    &field_anomalies,
                    point,
                    route_eta,
                    self.anomaly_scale,
                    world_size,
                );
                let magnitude = force.len();
                let ratio = magnitude / w.max_accel.max(1e-6);
                let risk = anomaly_death_risk_at(
                    &field_anomalies,
                    point,
                    route_eta,
                    self.anomaly_scale,
                    w.max_accel,
                    w.max_speed,
                    w.transport_radius,
                    world_size,
                    cell.size,
                );
                let tile_top_left = to_screen(V {
                    x: cell.min.x,
                    y: cell.max.y,
                });
                let tile_bottom_right = to_screen(V {
                    x: cell.max.x,
                    y: cell.min.y,
                });
                if risk > 0.015 {
                    field_painter.rect_filled(
                        Rect::from_two_pos(tile_top_left, tile_bottom_right),
                        0.0,
                        anomaly_risk_color(risk),
                    );
                }
                if magnitude > 1e-6 {
                    let cell_pixels = cell.size * s as f64;
                    let length = anomaly_field_arrow_length(ratio, cell_pixels);
                    let arrow_alpha = (100.0 + ratio.min(1.0) * 120.0) as u8;
                    draw_arrow(
                        &field_painter,
                        to_screen(point),
                        EVec2::new(force.x as f32, -force.y as f32),
                        length,
                        ArrowStyle {
                            color: Color32::from_rgba_unmultiplied(49, 72, 86, arrow_alpha),
                            shaft_width: (length * 0.1).clamp(0.55, 1.2),
                            head_length: (length * 0.5).clamp(0.75, 4.0),
                            head_half_width: (length * 0.2).clamp(0.5, 1.8),
                        },
                    );
                }
            }
        }
        let mut shapes =
            Vec::with_capacity(w.bounties.len() + w.anomalies.len() * 2 + w.transports.len() * 5);
        for a in &w.anomalies {
            let latest = V { x: a.x, y: a.y };
            let position = self
                .previous
                .as_ref()
                .and_then(|old| old.anomalies.iter().find(|p| p.id == a.id))
                .map(|old| lerp_vec(V { x: old.x, y: old.y }, latest, alpha))
                .unwrap_or(latest);
            let c = to_screen(position);
            let repel = a.strength < 0.;
            let alpha = anomaly_field_alpha(a.strength);
            let field = if repel {
                Color32::from_rgba_unmultiplied(38, 126, 225, alpha)
            } else {
                Color32::from_rgba_unmultiplied(231, 77, 64, alpha)
            };
            let core = if repel {
                Color32::from_rgb(25, 103, 214)
            } else {
                Color32::from_rgb(215, 57, 51)
            };
            let r = (a.effective_radius * s as f64).max(0.) as f32;
            if self.show_anomaly_zones && r > 0. {
                shapes.push(Shape::circle_filled(c, r, field));
            }
            shapes.push(Shape::circle_filled(
                c,
                (a.radius * s as f64).max(2.) as f32,
                core,
            ));
        }
        for b in &w.bounties {
            let c = to_screen(V { x: b.x, y: b.y });
            let r = world_radius_pixels(b.radius, s as f64);
            let gold = Color32::from_rgb(226, 164, 25);
            if r < 0.75 {
                // Location cue only: keep subpixel coins discoverable without
                // pretending that the marker represents their physical radius.
                shapes.push(Shape::circle_filled(c, 0.75, gold));
            } else {
                let outline = r * 0.12;
                let fill_radius = (r - outline * 0.5).max(0.0);
                shapes.push(Shape::circle_filled(c, fill_radius, gold));
                shapes.push(Shape::circle_stroke(
                    c,
                    fill_radius,
                    Stroke::new(outline, Color32::from_rgb(112, 81, 16)),
                ));
            }
        }
        painter.extend(shapes);
        let max_anomaly_speed = w
            .anomalies
            .iter()
            .map(|a| a.velocity.len())
            .fold(0.0, f64::max)
            .max(1e-6);
        for a in &w.anomalies {
            let latest = V { x: a.x, y: a.y };
            let (position, velocity) = self
                .previous
                .as_ref()
                .and_then(|old| old.anomalies.iter().find(|old| old.id == a.id))
                .map(|old| {
                    (
                        lerp_vec(V { x: old.x, y: old.y }, latest, alpha),
                        lerp_vec(old.velocity, a.velocity, alpha),
                    )
                })
                .unwrap_or((latest, a.velocity));
            if let Some(length) = (velocity.len() > 1e-6)
                .then(|| (velocity.len() / max_anomaly_speed * 54.0).clamp(14.0, 54.0) as f32)
            {
                let direction = EVec2::new(velocity.x as f32, -velocity.y as f32).normalized();
                let center = to_screen(position);
                let core_radius = (a.radius * s as f64).max(2.) as f32;
                let origin = center + direction * (core_radius + 1.5);
                let color = if a.strength < 0.0 {
                    Color32::from_rgb(25, 103, 214)
                } else {
                    Color32::from_rgb(215, 57, 51)
                };
                draw_arrow(
                    &painter,
                    origin,
                    direction,
                    length,
                    ArrowStyle {
                        color,
                        shaft_width: 1.8,
                        head_length: 4.5,
                        head_half_width: 2.7,
                    },
                );
            }
        }
        for a in &w.anomalies {
            let latest = V { x: a.x, y: a.y };
            let position = self
                .previous
                .as_ref()
                .and_then(|old| old.anomalies.iter().find(|old| old.id == a.id))
                .map(|old| lerp_vec(V { x: old.x, y: old.y }, latest, alpha))
                .unwrap_or(latest);
            let center = to_screen(position);
            let label = if a.strength == 0.0 {
                "0".to_owned()
            } else {
                format!("{:+.0}", a.strength)
            };
            let font_size = (a.radius * s as f64 * 0.8).clamp(9.0, 15.0) as f32;
            painter.text(
                center,
                Align2::CENTER_CENTER,
                label,
                FontId::proportional(font_size),
                Color32::WHITE,
            );
        }
        let selected_snapshot_position = self.current().map(|t| V { x: t.x, y: t.y });
        let selected_render_position = self.current().map(|t| self.transport_position(t, alpha));
        let trajectory_offset = selected_snapshot_position
            .zip(selected_render_position)
            .map(|(snapshot, rendered)| rendered.sub(snapshot))
            .unwrap_or_default();
        let selected_main_color = self.tracks.iter().find(|track| track.current).map(|track| {
            if track.death.is_some() {
                Color32::from_rgb(218, 63, 70)
            } else if track.top1 {
                Color32::from_rgb(238, 164, 27)
            } else {
                Color32::from_rgb(27, 76, 137)
            }
        });
        let history_start = render_time
            .checked_sub(Duration::from_secs(HISTORY_SECONDS))
            .unwrap_or(render_time);
        for carpet in &w.transports {
            let Some(samples) = self.position_history.get(&carpet.id) else {
                continue;
            };
            let endpoint = self.transport_position(carpet, alpha);
            let history = history_path(samples, history_start, render_time, endpoint)
                .into_iter()
                .map(to_screen)
                .collect::<Vec<_>>();
            if history.len() > 1 {
                let base = if self.selected.as_deref() == Some(&carpet.id) {
                    selected_main_color.unwrap_or(Color32::from_rgb(27, 76, 137))
                } else {
                    Color32::from_rgb(34, 91, 161)
                };
                let alpha = if self.selected.as_deref() == Some(&carpet.id) {
                    125
                } else {
                    85
                };
                let history_color =
                    Color32::from_rgba_unmultiplied(base.r(), base.g(), base.b(), alpha);
                painter.add(Shape::line(history, Stroke::new(2.2, history_color)));
            }
            if let Some(events) = self.collected_bounty_history.get(&carpet.id) {
                for (_, bounty_position) in events
                    .iter()
                    .filter(|(time, _)| *time >= history_start && *time <= render_time)
                {
                    painter.circle_filled(
                        to_screen(*bounty_position),
                        4.0,
                        Color32::from_gray(155),
                    );
                }
            }
        }
        let mut visible_tracks = self
            .tracks
            .iter()
            .filter(|track| track.current)
            .collect::<Vec<_>>();
        let ranked_tracks = self.tracks.iter().filter(|track| !track.current);
        if self.show_all_routes {
            visible_tracks.extend(ranked_tracks);
        } else {
            visible_tracks.extend(ranked_tracks.take(1));
        }
        for tr in visible_tracks {
            let mandatory = tr.current || tr.rank == 1;
            if !mandatory
                && ((tr.death.is_some() && !self.show_death)
                    || (tr.death.is_none() && !self.show_profitable))
            {
                continue;
            }
            let color = if tr.current && tr.death.is_some() {
                Color32::from_rgb(218, 63, 70)
            } else if tr.rank == 1 || (tr.current && tr.top1) {
                Color32::from_rgb(238, 164, 27)
            } else if tr.current {
                Color32::from_rgb(27, 76, 137)
            } else {
                route_rank_color(tr.rank)
            };
            let pts = tr
                .points
                .iter()
                .map(|p| to_screen(p.add(trajectory_offset)))
                .collect::<Vec<_>>();
            if pts.len() > 1 {
                let stroke_width = if !tr.current || tr.death.is_some() {
                    1.8
                } else {
                    2.5
                };
                let use_time_gradient = !tr.current && tr.rank != 1 && !tr.top1;
                for (index, segment) in pts.windows(2).enumerate() {
                    let segment_color = if use_time_gradient {
                        trajectory_time_color(
                            tr.point_times.get(index + 1).copied().unwrap_or_else(|| {
                                tr.duration * (index + 1) as f64 / (pts.len() - 1) as f64
                            }),
                            self.horizon,
                        )
                    } else {
                        color
                    };
                    painter.add(Shape::line(
                        segment.to_vec(),
                        Stroke::new(stroke_width, segment_color),
                    ));
                }
            }
            if let Some(d) = tr.death {
                let c = to_screen(d.add(trajectory_offset));
                let marker_color = Color32::from_rgb(218, 63, 70);
                painter.line_segment(
                    [c + EVec2::new(-4., -4.), c + EVec2::new(4., 4.)],
                    Stroke::new(1.3, marker_color),
                );
                painter.line_segment(
                    [c + EVec2::new(-4., 4.), c + EVec2::new(4., -4.)],
                    Stroke::new(1.3, marker_color),
                );
            }
        }
        for (i, t) in w.transports.iter().enumerate() {
            let p = self.transport_position(t, alpha);
            let c = to_screen(p);
            let selected = self.selected.as_deref() == Some(&t.id);
            let own = Color32::from_rgb(34, 91, 161);
            if selected {
                painter.circle_filled(
                    c,
                    (w.max_speed * s as f64 * 0.15).max(10.) as f32,
                    Color32::from_rgba_unmultiplied(240, 181, 48, 38),
                );
            }
            painter.circle_filled(
                c,
                (w.transport_radius * s as f64).max(4.) as f32,
                if selected {
                    Color32::from_rgb(20, 70, 135)
                } else {
                    own
                },
            );
            painter.text(
                c + EVec2::new(7., -9.),
                Align2::LEFT_BOTTOM,
                if t.id.trim().is_empty() {
                    format!("own-{:02}", i + 1)
                } else {
                    t.id.clone()
                },
                FontId::proportional(11.),
                Color32::DARK_GRAY,
            );
            let prior = self
                .previous
                .as_ref()
                .and_then(|old| old.transports.iter().find(|old| old.id == t.id));
            let vectors = [
                (
                    prior
                        .map(|o| lerp_vec(o.velocity, t.velocity, alpha))
                        .unwrap_or(t.velocity),
                    Color32::from_rgb(30, 145, 80),
                    w.max_speed,
                    80.0_f32,
                ),
                (
                    prior
                        .map(|o| lerp_vec(o.self_acceleration, t.self_acceleration, alpha))
                        .unwrap_or(t.self_acceleration),
                    Color32::from_rgb(208, 135, 13),
                    w.max_accel,
                    64.0_f32,
                ),
                (
                    prior
                        .map(|o| lerp_vec(o.anomaly_acceleration, t.anomaly_acceleration, alpha))
                        .unwrap_or(t.anomaly_acceleration),
                    Color32::from_rgb(126, 70, 175),
                    w.max_accel,
                    96.0_f32,
                ),
            ];
            for (v, col, reference, max_pixels) in vectors {
                if let Some(pixels) = vector_pixels(v.len(), reference, max_pixels) {
                    let direction = EVec2::new(v.x as f32, -v.y as f32).normalized();
                    draw_arrow(
                        &painter,
                        c,
                        direction,
                        pixels,
                        ArrowStyle {
                            color: col,
                            shaft_width: 1.8,
                            head_length: 5.0,
                            head_half_width: 3.0,
                        },
                    );
                }
            }
        }
        for (index, e) in w.enemies.iter().enumerate() {
            if e.status == "destroyed" {
                continue;
            }
            let latest = V { x: e.x, y: e.y };
            let position = self
                .previous
                .as_ref()
                .filter(|old| old.enemies.len() == w.enemies.len())
                .and_then(|old| old.enemies.get(index))
                .map(|old| lerp_vec(V { x: old.x, y: old.y }, latest, alpha))
                .unwrap_or(latest);
            let c = to_screen(position);
            painter.circle_filled(
                c,
                (w.transport_radius * s as f64).max(4.) as f32,
                Color32::from_rgb(63, 154, 98),
            );
            let label = if e.id.trim().is_empty() {
                format!("enemy-{:02}", index + 1)
            } else {
                e.id.clone()
            };
            painter.text(
                c + EVec2::new(7., -9.),
                Align2::LEFT_BOTTOM,
                label,
                FontId::proportional(11.),
                Color32::DARK_GRAY,
            );
        }
        let response = ui.interact(rect, ui.id().with("world"), Sense::click_and_drag());
        if response.dragged() {
            let d = ui.input(|i| i.pointer.delta());
            self.camera.x -= d.x as f64 / s as f64;
            self.camera.y += d.y as f64 / s as f64;
            self.follow = false;
        }
        if response.clicked()
            && let Some(pos) = response.interact_pointer_pos()
        {
            let hit = w
                .transports
                .iter()
                .find(|t| to_screen(self.transport_position(t, alpha)).distance(pos) < 16.);
            let old_selection = self.selected.clone();
            self.selected = hit.map(|t| t.id.clone());
            self.track_key.0 = u64::MAX;
            self.follow = self.selected.is_some();
            if old_selection != self.selected && self.manual {
                self.manual = false;
                let _ = self.network.commands.send(Command::Manual(None));
                self.clear_manual_lease();
            }
        }
        if response.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0. {
                let mouse = ui.input(|i| i.pointer.hover_pos()).unwrap_or(rect.center());
                let offset = mouse - rect.center();
                let old_scale = s as f64;
                let world_at_pointer = V {
                    x: self.camera.x + offset.x as f64 / old_scale,
                    y: self.camera.y - offset.y as f64 / old_scale,
                };
                self.zoom = (self.zoom * (1. + scroll as f64 * 0.001)).clamp(0.1, 20.);
                let new_scale = (rect.width() as f64 / world_size.x.max(1.))
                    .min(rect.height() as f64 / world_size.y.max(1.))
                    * self.zoom;
                self.camera = V {
                    x: world_at_pointer.x - offset.x as f64 / new_scale,
                    y: world_at_pointer.y + offset.y as f64 / new_scale,
                };
                self.follow = false;
            }
        }
        if self.manual
            && let Some(tr) = self.current().cloned()
        {
            if let Some(mouse) = ui
                .input(|i| i.pointer.hover_pos())
                .filter(|p| rect.contains(*p))
            {
                let origin = to_screen(self.transport_position(&tr, alpha));
                let delta = mouse - origin;
                let len = (delta.x * delta.x + delta.y * delta.y).sqrt() as f64;
                let mag = len.min(w.max_accel);
                let dir = V {
                    x: delta.x as f64,
                    y: -delta.y as f64,
                }
                .unit();
                let a = dir.mul(mag);
                painter.line_segment([origin, mouse], Stroke::new(1., Color32::GRAY));
                if !self.lease_active || self.lease_refresh.elapsed() >= Duration::from_millis(200)
                {
                    self.write_manual_lease(&tr.id);
                    self.lease_active = true;
                }
                let _ = self
                    .network
                    .commands
                    .send(Command::Manual(Some((tr.id.clone(), a))));
            }
            if self.lease_active && self.lease_refresh.elapsed() >= Duration::from_millis(200) {
                self.write_manual_lease(&tr.id);
            }
        }
    }

    fn write_manual_lease(&mut self, carpet_id: &str) {
        let lease = serde_json::json!({
            "carpetId": carpet_id,
            "leaseId": self.lease_id,
            "expiresAtUnixMs": unix_ms() + 1000
        });
        if let Some(parent) = self.lease_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::write(&self.lease_path, lease.to_string());
        self.lease_refresh = Instant::now();
    }

    fn clear_manual_lease(&self) {
        let is_ours = fs::read(&self.lease_path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
            .and_then(|value| {
                value
                    .get("leaseId")
                    .and_then(|id| id.as_str().map(str::to_owned))
            })
            .is_some_and(|id| id == self.lease_id);
        if is_ours {
            let _ = fs::remove_file(&self.lease_path);
        }
    }
}
fn default_hub_url(arena_url: &str) -> String {
    let Ok(mut url) = reqwest::Url::parse(arena_url) else {
        return "http://127.0.0.1:8090".to_string();
    };
    if url.set_port(Some(8090)).is_err() {
        return "http://127.0.0.1:8090".to_string();
    }
    url.set_path("/");
    url.set_query(None);
    url.set_fragment(None);
    url.to_string().trim_end_matches('/').to_string()
}
fn normalize_hub_url(url: String) -> String {
    url.trim_end_matches('/').to_string()
}
fn token_hash(token: &str) -> u64 {
    token
        .as_bytes()
        .iter()
        .fold(0xcbf29ce484222325_u64, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
        })
}
fn rate(t: &Track) -> f64 {
    if t.score == 0 {
        0.
    } else {
        t.score as f64 / t.last_time.max(0.2)
    }
}
fn rank_rate(track: &Track) -> f64 {
    if track.rank_score == 0 {
        0.0
    } else {
        f64::from(track.rank_score) / track.rank_last_time.max(0.2)
    }
}
fn rank_average_speed(track: &Track) -> f64 {
    if track.rank_speed_duration > 0.0 {
        track.rank_speed_integral / track.rank_speed_duration
    } else {
        track.rank_terminal_speed
    }
}
fn compare_rank(a: &Track, b: &Track) -> std::cmp::Ordering {
    match (a.rank_death_time, b.rank_death_time) {
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (Some(_), None) => std::cmp::Ordering::Less,
        (Some(a_death), Some(b_death)) => rank_average_speed(a)
            .total_cmp(&rank_average_speed(b))
            .then_with(|| a.rank_terminal_speed.total_cmp(&b.rank_terminal_speed))
            .then_with(|| a_death.total_cmp(&b_death))
            .then_with(|| a.rank_score.cmp(&b.rank_score)),
        (None, None) => rank_rate(a)
            .total_cmp(&rank_rate(b))
            .then_with(|| a.rank_score.cmp(&b.rank_score))
            .then_with(|| rank_average_speed(a).total_cmp(&rank_average_speed(b)))
            .then_with(|| a.rank_terminal_speed.total_cmp(&b.rank_terminal_speed))
            .then_with(|| {
                b.rank_first_bounty_time
                    .unwrap_or(TOP1_RANKING_HORIZON)
                    .total_cmp(&a.rank_first_bounty_time.unwrap_or(TOP1_RANKING_HORIZON))
            }),
    }
}
fn current_route_rank(current: &Track, candidates: &[Track]) -> Option<(usize, usize)> {
    if current.rank_death_time.is_some() {
        return None;
    }
    let eligible = candidates
        .iter()
        .filter(|route| route.rank_death_time.is_none() && route.rank_score > 0)
        .collect::<Vec<_>>();
    let rank = eligible
        .iter()
        .take_while(|route| compare_rank(route, current).is_gt())
        .count()
        + 1;
    (current.rank_score > 0).then_some((rank, eligible.len() + 1))
}
fn route_rank_color(rank: usize) -> Color32 {
    let hue = (rank as f32 * 0.618_034) % 1.0;
    let sector = hue * 6.0;
    let chroma = 0.78;
    let x = chroma * (1.0 - ((sector % 2.0) - 1.0).abs());
    let (r, g, b) = match sector as u8 {
        0 => (chroma, x, 0.0),
        1 => (x, chroma, 0.0),
        2 => (0.0, chroma, x),
        3 => (0.0, x, chroma),
        4 => (x, 0.0, chroma),
        _ => (chroma, 0.0, x),
    };
    let value = |channel: f32| ((channel + 0.16) * 255.0) as u8;
    Color32::from_rgb(value(r), value(g), value(b))
}
fn trajectory_time_color(elapsed: f64, horizon: f64) -> Color32 {
    let progress = if horizon.is_finite() && horizon > 0.0 {
        (elapsed / horizon).clamp(0.0, 1.0) as f32
    } else {
        0.0
    };
    let near = [46.0, 190.0, 112.0];
    let middle = [238.0, 194.0, 64.0];
    let far = [196.0, 76.0, 132.0];
    let (from, to, local) = if progress <= 0.5 {
        (near, middle, progress * 2.0)
    } else {
        (middle, far, (progress - 0.5) * 2.0)
    };
    let channel = |index: usize| (from[index] + (to[index] - from[index]) * local) as u8;
    Color32::from_rgb(channel(0), channel(1), channel(2))
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.update_network();
        let now = Instant::now();
        let dt = now.duration_since(self.frame_at).as_secs_f32();
        self.frame_at = now;
        if dt > 0. {
            self.fps = 0.9 * self.fps + 0.1 / dt;
        }
        self.recalc();
        let mut forecast_settings_changed = false;
        ctx.input(|i| {
            if i.key_pressed(egui::Key::A) {
                self.show_anomaly_field = !self.show_anomaly_field;
            }
            if i.key_pressed(egui::Key::P) {
                self.show_all_routes = !self.show_all_routes;
            }
            if i.key_pressed(egui::Key::L) {
                self.show_leaderboard = !self.show_leaderboard;
            }
            if i.key_pressed(egui::Key::M) {
                self.set_manual(!self.manual);
            }
            if i.key_pressed(egui::Key::Space) {
                self.follow = !self.follow;
            }
            if i.key_pressed(egui::Key::R) {
                if let Some(w) = &self.world {
                    self.camera = V {
                        x: w.map_size.x * 0.5,
                        y: w.map_size.y * 0.5,
                    };
                }
                self.zoom = 1.;
                self.follow = false;
            }
            let pan = self
                .world
                .as_ref()
                .map(|w| w.map_size.x.max(w.map_size.y) * 0.015 / self.zoom)
                .unwrap_or(10.);
            if i.key_pressed(egui::Key::ArrowUp) {
                self.camera.y += pan;
                self.follow = false;
            }
            if i.key_pressed(egui::Key::ArrowDown) {
                self.camera.y -= pan;
                self.follow = false;
            }
            if i.key_pressed(egui::Key::ArrowLeft) {
                self.camera.x -= pan;
                self.follow = false;
            }
            if i.key_pressed(egui::Key::ArrowRight) {
                self.camera.x += pan;
                self.follow = false;
            }
            let index = [
                egui::Key::Num1,
                egui::Key::Num2,
                egui::Key::Num3,
                egui::Key::Num4,
                egui::Key::Num5,
            ]
            .iter()
            .position(|key| i.key_pressed(*key));
            if let (Some(index), Some(world)) = (index, self.world.as_ref())
                && let Some(t) = world.transports.get(index)
            {
                self.selected = Some(t.id.clone());
                self.manual = false;
                self.track_key.0 = u64::MAX;
                let _ = self.network.commands.send(Command::Manual(None));
                self.clear_manual_lease();
            }
            if i.key_pressed(egui::Key::Plus) || i.key_pressed(egui::Key::Equals) {
                self.zoom = (self.zoom * 1.1).min(20.);
            }
            if i.key_pressed(egui::Key::Minus) {
                self.zoom = (self.zoom / 1.1).max(0.1);
            }
        });
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(
                EVec2::new(275., ui.available_height()),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.heading("StadMagic · Rust");
                    if ui.button("Лидерборд (L)").clicked() {
                        self.show_leaderboard = !self.show_leaderboard;
                    }
                    if ui.button("О мире").clicked() {
                        self.show_world_info = !self.show_world_info;
                    }
                    let mut manual_toggle_changed = false;
                    let mut selected_to_activate = None;
                    if let Some(w) = &self.world {
                        egui::Grid::new("session-summary")
                            .num_columns(2)
                            .spacing([12.0, 3.0])
                            .show(ui, |ui| {
                                ui.strong(format!("Очки {}", w.points));
                                ui.label(format!("Монет собрано {}", self.session_bounties_collected));
                                ui.end_row();
                                ui.label(format!("Смерти {}", self.own_deaths));
                                ui.label(format!("Потери {}", self.gold_lost));
                                ui.end_row();
                        });
                        ui.small(format!("{} · {:.0} FPS · tick {} · zoom {:.2}×", w.name, self.fps, self.tick, self.zoom));
                        ui.horizontal_wrapped(|ui| {
                            ui.checkbox(&mut self.show_anomaly_field, "Риск (A)");
                            ui.checkbox(&mut self.show_anomaly_zones, "Зоны");
                            ui.checkbox(&mut self.follow, "Следовать (Space)");
                            if ui.checkbox(&mut self.manual, "Ручное управление (M)").changed() {
                                manual_toggle_changed = true;
                            }
                        });
                        if !self.last_error.is_empty() {
                            ui.colored_label(Color32::DARK_RED, &self.last_error);
                        }
                        if !w.errors.is_empty() {
                            ui.colored_label(Color32::DARK_RED, w.errors.join("; "));
                        }

                        ui.collapsing("Карта", |ui| {
                            ui.label(format!("{:.0} × {:.0}", w.map_size.x, w.map_size.y));
                            ui.label(format!("Ковры {} · враги {} · монеты на карте {} · аномалии {} · розыск {}", w.transports.len(), w.enemies.len(), w.bounties.len(), w.anomalies.len(), w.wanted_list.len()));
                            if self.show_anomaly_field {
                                ui.checkbox(&mut self.adaptive_risk_grid, "Адаптивная сетка (общая для своих ковров)");
                                if self.adaptive_risk_grid {
                                    ui.add(egui::Slider::new(&mut self.anomaly_field_spacing, ADAPTIVE_RISK_STEP_MIN..=1000.0).text("Мелкий шаг адаптивной сетки"));
                                } else {
                                    ui.add(egui::Slider::new(&mut self.standard_risk_spacing, STANDARD_RISK_STEP_MIN..=1000.0).text("Шаг стандартной сетки"));
                                }
                                ui.small("Белый — спокойно, жёлтый/оранжевый — компенсируемая опасность, красный — фатальная зона по модели; стрелки — результирующая сила.");
                                ui.small("Адаптивная сетка детализирует области рядом с коврами и укрупняет дальние; стандартная использует одинаковый шаг. Настройки режимов независимы.");
                                ui.small("Поле аномалий сдвигается на ETA: по current/top-1 — фактическое, вне маршрута — грубая оценка подхода со max speed.");
                            }
                        });

                        ui.collapsing("Прогноз", |ui| {
                            if ui.add(egui::Slider::new(&mut self.scan_step, 1.0..=20.0).text("Шаг направления °")).changed() {
                                self.track_key.0 = u64::MAX;
                                forecast_settings_changed = true;
                            }
                            if ui.button(if self.show_all_routes { "Оставить только top-1 (P)" } else { "Показать весь веер (P)" }).clicked() {
                                self.show_all_routes = !self.show_all_routes;
                            }
                            ui.horizontal_wrapped(|ui| {
                                ui.checkbox(&mut self.show_current, "Текущий");
                                ui.checkbox(&mut self.show_profitable, "Монеты");
                                ui.checkbox(&mut self.show_death, "Смертельные");
                            });
                            let forecast_points = self.tracks.iter().find(|track| track.current)
                                .map(|track| track.points.len()).unwrap_or(0);
                            ui.small(format!("~{} направлений · прогноз до 75 с · окно top-1 15,3 с · точек текущего маршрута {forecast_points}", (360.0 / self.scan_step).ceil() as usize));
                            ui.collapsing("Пояснения", |ui| {
                                ui.small("Больше градусов между направлениями — меньше расчётная нагрузка, но грубее выбор траектории.");
                                ui.small("Score/time — очки за секунду до последней монеты маршрута.");
                                ui.small("Top-1 ранжируется по первым 15,3 с от snapshot (0,3 с сетевой lead-in + 15 с игрока, шаг 0,2 с); дальше хвост логарифмически растёт до 0,8 с. Дальние столкновения приблизительнее.");
                            });
                        });

                        ui.collapsing("Легенда", |ui| {
                            ui.colored_label(Color32::from_rgb(30, 145, 80), format!("V — скорость (до {:.0})", w.max_speed));
                            ui.colored_label(Color32::from_rgb(208, 135, 13), format!("S — своё ускорение (до {:.0})", w.max_accel));
                            ui.colored_label(Color32::from_rgb(126, 70, 175), format!("W — ускорение аномалий (до {:.0})", w.max_accel));
                            ui.colored_label(Color32::from_rgb(25, 103, 214), "Синяя аномалия — отталкивает");
                            ui.colored_label(Color32::from_rgb(215, 57, 51), "Красная аномалия — притягивает");
                            ui.colored_label(Color32::from_rgb(238, 164, 27), "Золотой — agile top-1 (15 с + lead-in)");
                            ui.colored_label(Color32::from_rgb(27, 76, 137), "Синий — текущий маршрут");
                            ui.colored_label(Color32::from_rgb(218, 63, 70), "Красный крест — смерть");
                            ui.small("Серый след — путь за последние 50 секунд; серые точки — собранные монеты.");
                        });

                        ui.collapsing(format!("Свои ковры ({})", w.transports.len()), |ui| {
                            for t in &w.transports {
                                let label = format!("{} · {:.0},{:.0} · |v| {:.1}", t.id, t.x, t.y, t.velocity.len());
                                if ui.selectable_label(self.selected.as_deref() == Some(&t.id), label).clicked() {
                                    selected_to_activate = Some(t.id.clone());
                                }
                            }
                        });

                        if let Some(t) = self.current() {
                            ui.collapsing(format!("Выбранный · {}", t.id), |ui| {
                                if let Some(route) = self.tracks.iter().find(|route| route.current) {
                                    let rank = if route.rank > 0 { format!("#{} / {} по score/time за окно 15 с игрока", route.rank, route.rank_total) } else if route.rank_death_time.is_some() { "смерть в окне top-1".into() } else { "нет в прибыльном рейтинге".into() };
                                    ui.label(format!("Текущий маршрут: {rank}"));
                                    ui.label(format!("Окно игрока: {} оч. · {:.2}/с; весь прогноз: {} оч. · {:.2}/с", route.rank_score, rank_rate(route), route.score, rate(route)));
                                }
                                ui.label(format!("Позиция {:.1}, {:.1}", t.x, t.y));
                                ui.label(format!("V ({:.1}, {:.1}) · {:.1}", t.velocity.x, t.velocity.y, t.velocity.len()));
                                ui.label(format!("S ({:.1}, {:.1})", t.self_acceleration.x, t.self_acceleration.y));
                                ui.label(format!("W ({:.1}, {:.1})", t.anomaly_acceleration.x, t.anomaly_acceleration.y));
                                ui.separator();
                                ui.strong("Лог player_2");
                                if let Some(telemetry) = &self.player_telemetry {
                                    let age_ms = unix_ms().saturating_sub(u128::from(telemetry.updated_at_unix_ms));
                                    if age_ms > 2000 {
                                        ui.colored_label(Color32::DARK_RED, format!("Данные устарели · {:.1} с", age_ms as f64 / 1000.0));
                                    } else {
                                        ui.small(format!("tick {} · {} / {} · doom {} · {:.0} с", telemetry.tick, telemetry.aim_strategy, telemetry.movement_strategy, telemetry.doom_policy, telemetry.horizon_seconds));
                                        ui.small(format!("План {:.1} мс · цикл {:.1} мс · RTT {:.1} мс · маршрутов {}", telemetry.plan_ms, telemetry.cycle_ms, telemetry.rtt_ms, telemetry.trajectory_evaluations));
                                    }
                                    if let Some(log) = telemetry.carpets.iter().find(|log| log.id == t.id) {
                                        let goal = log.goal.as_deref().unwrap_or(if log.alive { "planning" } else { "dead" });
                                        let movement = log.movement_decision.as_deref().unwrap_or("aim");
                                        ui.label(format!("Цель {goal} · решение {movement} · риск {}", log.risk_exposure.map(|v| format!("{v:.2} с")).unwrap_or_else(|| "—".into())));
                                        if let Some(target) = log.target {
                                            ui.label(format!("Монета ({:.0}, {:.0}) +{:.0} · d {}", target.x, target.y, log.target_points.unwrap_or(0.0), log.target_distance.map(|v| format!("{v:.0}")).unwrap_or_else(|| "—".into())));
                                        } else {
                                            ui.label("Цель-монета отсутствует");
                                        }
                                        ui.small(format!("Маршрут {:.0} оч. · ETA {} · до последней {} · {:.2} оч./с · первая {:.2} оч./с · potential {:.1} · монет {}", log.route_score.unwrap_or(0.0), log.time_to_reach_score.map(|v| format!("{v:.1} с")).unwrap_or_else(|| "—".into()), log.time_to_last_bounty.map(|v| format!("{v:.1} с")).unwrap_or_else(|| "—".into()), log.score_rate.unwrap_or(0.0), log.first_bounty_rate.unwrap_or(0.0), log.total_potential_score.unwrap_or(0.0), log.bounty_count));
                                        ui.small(format!("P({:.0},{:.0}) V({:.0},{:.0}) |V| {:.1} · A_next {} · A_now ({:.0},{:.0}) · W ({:.0},{:.0})", log.position.x, log.position.y, log.velocity.x, log.velocity.y, log.speed, log.command_acceleration.map(|v| format!("({:.0},{:.0}) {:.0}", v.x, v.y, v.len())).unwrap_or_else(|| "—".into()), log.current_acceleration.x, log.current_acceleration.y, log.anomaly_acceleration.x, log.anomaly_acceleration.y));
                                        if let Some(death_at) = log.death_at {
                                            ui.colored_label(Color32::DARK_RED, format!("Смерть через {death_at:.1} с · {}", log.death_reason.as_deref().unwrap_or("причина неизвестна")));
                                        }
                                    } else if age_ms <= 2000 {
                                        ui.weak("В snapshot player_2 нет этого ковра");
                                    }
                                } else {
                                    ui.weak("Нет лога player_2: запустите игрока с тем же токеном или задайте общий DATS_PLAYER_TELEMETRY_FILE");
                                }
                                for (i, route) in self.tracks.iter().take(5).enumerate() {
                                    ui.small(format!("{}{} · {} очков / {:.1}с · {:.2}/с{}", if route.current { "Текущий" } else { "Маршрут" }, if route.current { String::new() } else { format!(" {i}") }, route.score, route.last_time, rate(route), if route.death.is_some() { " · смерть" } else if route.top1 { " · top-1" } else { "" }));
                                }
                            });
                        }
                    }
                    if let Some(id) = selected_to_activate {
                        self.selected = Some(id);
                        self.follow = true;
                        self.manual = false;
                        let _ = self.network.commands.send(Command::Manual(None));
                        self.clear_manual_lease();
                        self.track_key.0 = u64::MAX;
                    }
                    if manual_toggle_changed {
                        self.set_manual(self.manual);
                    }
                },
            );
            if forecast_settings_changed {
                self.recalc();
            }
            ui.allocate_ui_with_layout(
                ui.available_size(),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    let rect = ui.available_rect_before_wrap();
                    self.render_world(ui, rect);
                },
            );
        });
        if self.show_leaderboard {
            egui::Window::new("Лидерборд команд")
                .open(&mut self.show_leaderboard)
                .resizable(true)
                .default_width(620.0)
                .show(&ctx, |ui| {
                    if let Some(board) = self.leaderboard.clone() {
                        let age_seconds = unix_ms().saturating_sub(board.updated_at_unix_ms) / 1000;
                        ui.small(format!(
                            "Обновлено {} с назад · команд {}",
                            age_seconds,
                            board.teams.len()
                        ));
                        if let Some(error) = &self.leaderboard_error {
                            ui.colored_label(
                                egui::Color32::from_rgb(180, 45, 45),
                                format!("Ошибка обновления Hub: {error}"),
                            );
                        }
                        if let Some(generated_name) = self.own_team_name.clone() {
                            ui.separator();
                            ui.label(format!(
                                "Ваша команда: {}",
                                if self.team_name_override.is_empty() {
                                    generated_name.as_str()
                                } else {
                                    self.team_name_override.as_str()
                                }
                            ));
                            ui.horizontal(|ui| {
                                ui.label("Переименовать:");
                                ui.text_edit_singleline(&mut self.team_name_edit);
                                if ui.button("Сохранить имя").clicked() {
                                    let new_name = self.team_name_edit.trim().chars().take(48).collect::<String>();
                                    if fs::write(&self.team_name_path, &new_name).is_ok() {
                                        self.team_name_override = new_name;
                                    }
                                }
                                if ui.button("Сбросить").clicked() {
                                    if fs::write(&self.team_name_path, "").is_ok() {
                                        self.team_name_edit.clear();
                                        self.team_name_override.clear();
                                    }
                                }
                            });
                        } else {
                            ui.weak("Ваша команда пока не найдена в лидерборде; дождитесь следующего обновления после подключения игрока.");
                        }
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            egui::Grid::new("leaderboard-table")
                                .striped(true)
                                .num_columns(9)
                                .show(ui, |ui| {
                                    for title in [
                                        "#",
                                        "Команда",
                                        "Попыток",
                                        "Top: золото",
                                        "Top: собрано",
                                        "Total: золото",
                                        "Total: собрано",
                                        "Total: потери",
                                        "Total: дистанция",
                                    ] {
                                        ui.strong(title);
                                    }
                                    ui.end_row();
                                    for team in &board.teams {
                                        let is_own = team.team_id == self.own_team_id;
                                        if is_own {
                                            ui.colored_label(egui::Color32::from_rgb(30, 130, 45), format!("{} · вы", team.rank));
                                        } else {
                                            ui.label(team.rank.to_string());
                                        }
                                        let display_name = if is_own && !self.team_name_override.is_empty() {
                                            self.team_name_override.as_str()
                                        } else {
                                            team.name.as_str()
                                        };
                                        if is_own {
                                            ui.strong(display_name);
                                        } else {
                                            ui.label(display_name);
                                        }
                                        ui.label(team.attempts.to_string());
                                        ui.label(team.top.gold.to_string());
                                        ui.label(team.top.gold_collected.to_string());
                                        ui.label(team.total.gold.to_string());
                                        ui.label(team.total.gold_collected.to_string());
                                        ui.label(team.total.carpets_lost.to_string());
                                        ui.label(format!("{:.0}", team.total.distance_travelled));
                                        ui.end_row();
                                    }
                                });
                        });
                    } else {
                        ui.label("Нет данных лидерборда из Hub API.");
                        if let Some(error) = &self.leaderboard_error {
                            ui.colored_label(egui::Color32::from_rgb(180, 45, 45), error);
                        }
                        ui.small(format!("Источник: {}/api/leaderboard?scope=all", self.hub_url));
                    }
                });
        }
        if self.show_world_info {
            egui::Window::new("О мире")
                .open(&mut self.show_world_info)
                .resizable(true)
                .default_width(420.0)
                .show(&ctx, |ui| {
                    if let Some(info) = &self.world_info {
                        ui.heading(&info.name);
                        ui.label(format!("Мир {} · {} · запуск {}", info.world_number, info.world_id, info.arena_name));
                        ui.label(&info.description);
                        ui.separator();
                        egui::Grid::new("world-info-grid")
                            .num_columns(2)
                            .spacing([14.0, 5.0])
                            .show(ui, |ui| {
                                ui.label("Арена"); ui.label(format!("{:.0} × {:.0}", info.arena_width, info.arena_height)); ui.end_row();
                                ui.label("Тик сервера"); ui.label(format!("{} мс (фиксированный)", info.tick_rate_ms)); ui.end_row();
                                ui.label("Ковёр: max скорость / ускорение"); ui.label(format!("{:.0} / {:.0}", info.max_velocity, info.max_acceleration)); ui.end_row();
                                ui.label("Аномалии"); ui.label(format!("{} · скорость {:.0}–{:.0} · сила {:.0}–{:.0}", info.anomaly_quota, info.anomaly_speed_min, info.anomaly_speed_max, info.anomaly_force_min, info.anomaly_force_max)); ui.end_row();
                                ui.label("Ядро аномалии"); ui.label(format!("радиус {:.0}–{:.0}", info.anomaly_core_radius_min, info.anomaly_core_radius_max)); ui.end_row();
                                ui.label("Зона воздействия"); ui.label(format!("радиус {:.0}–{:.0}", info.anomaly_effect_radius_min, info.anomaly_effect_radius_max)); ui.end_row();
                                ui.label("Редкие сильные аномалии"); ui.label(format!("{:.0}% · сила {:.0}–{:.0}", info.anomaly_force_outlier_probability * 100.0, info.anomaly_force_outlier_min, info.anomaly_force_outlier_max)); ui.end_row();
                                ui.label("Монеты одновременно"); ui.label(info.bounty_quota.to_string()); ui.end_row();
                                ui.label("Номинал монет"); ui.label(format!("{}–{}", info.bounty_base_value, info.bounty_max_value)); ui.end_row();
                                ui.label("Трение"); ui.label(format!("{:.3}", info.friction)); ui.end_row();
                            });
                    } else {
                        ui.label("Информация о мире недоступна. Проверьте, что сервер запущен и публикует current_world.json.");
                        ui.small(format!("Путь: {}", self.world_status_path.display()));
                    }
                });
        }
        ctx.request_repaint_after(Duration::from_millis(16));
    }
}
impl Drop for App {
    fn drop(&mut self) {
        let _ = self.network.commands.send(Command::Stop);
        let _ = self.network.leaderboard_stop.send(());
        self.clear_manual_lease();
    }
}

fn unix_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn args() -> Result<(String, String, String, Duration), String> {
    let mut url = env::var("STADMAGIC_ARENA_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:8080/play/magcarp/player/move".to_string());
    let mut hub_url = env::var("STADMAGIC_HUB_URL").ok();
    let token = env::var("DATS_PLAYER_TOKEN").ok();
    let mut poll = Duration::from_millis(200);
    let mut it = env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--url" => url = it.next().ok_or("--url needs a value")?,
            "--hub-url" => hub_url = Some(it.next().ok_or("--hub-url needs a value")?),
            "--token" => {
                return Err(
                    "set the token with DATS_PLAYER_TOKEN, not a command-line argument".into(),
                );
            }
            "--poll-ms" => {
                poll = Duration::from_millis(
                    it.next()
                        .ok_or("--poll-ms needs a value")?
                        .parse()
                        .map_err(|_| "invalid --poll-ms")?,
                )
            }
            "--help" | "-h" => {
                return Err(
                    "Usage: DATS_PLAYER_TOKEN=... lib/arena-visualizer --url URL [--hub-url URL] [--poll-ms 200]"
                        .into(),
                );
            }
            _ => return Err(format!("unknown argument: {a}")),
        }
    }
    let token = token
        .filter(|s| !s.trim().is_empty())
        .ok_or("DATS_PLAYER_TOKEN is required and cannot be empty")?;
    if poll < Duration::from_millis(50) {
        return Err("--poll-ms must be at least 50".into());
    }
    let hub_url = normalize_hub_url(hub_url.unwrap_or_else(|| default_hub_url(&url)));
    Ok((url, hub_url, token, poll))
}
fn main() -> eframe::Result<()> {
    if env::args().any(|arg| arg == "--help" || arg == "-h") {
        println!(
            "Usage: DATS_PLAYER_TOKEN=... lib/arena-visualizer --url URL [--hub-url URL] [--poll-ms 200]"
        );
        return Ok(());
    }
    let (_url, hub_url, token, poll) = args().unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(2)
    });
    let window_title = format!("StadMagic · token: {token}");
    let app = App::new(hub_url, token, poll);
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Glow,
        viewport: egui::ViewportBuilder::default()
            .with_title(window_title)
            .with_inner_size([1600., 1000.])
            .with_min_inner_size([900., 600.]),
        ..Default::default()
    };
    eframe::run_native(
        "StadMagic Rust Visualizer",
        options,
        Box::new(|_| Ok(Box::new(app))),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_hub_url_uses_arena_host_and_control_plane_port() {
        assert_eq!(
            default_hub_url("http://localhost:8080/play/magcarp/player/move"),
            "http://localhost:8090"
        );
        assert_eq!(
            default_hub_url("https://game.example:8443/api"),
            "https://game.example:8090"
        );
    }

    #[test]
    fn hub_leaderboard_response_uses_team_id_and_top_total_metrics() {
        let board: LeaderboardFile = serde_json::from_str(
            r#"{"scope":"all","teams":[{"team_id":"0123456789abcdef","name":"Team Name","attempts":2,"rank":1,"top":{"gold":10,"gold_collected":20,"carpets_lost":1,"distance_travelled":30.0},"total":{"gold":40,"gold_collected":50,"carpets_lost":3,"distance_travelled":60.0}}]}"#,
        )
        .unwrap();
        assert_eq!(board.teams.len(), 1);
        assert_eq!(board.teams[0].team_id, "0123456789abcdef");
        assert_eq!(board.teams[0].top.gold_collected, 20);
        assert_eq!(board.teams[0].total.carpets_lost, 3);
    }

    #[test]
    fn parses_realtime_snapshot_envelope_without_losing_desert_state() {
        let (tick, snapshot) = parse_realtime_snapshot(
            r#"{"type":"snapshot","tick":42,"state":{"mapSize":{"x":1000,"y":800},"transports":[],"bounties":[{"x":10,"y":20,"radius":3,"points":7}]}}"#,
        )
        .unwrap()
        .unwrap();
        assert_eq!(tick, 42);
        assert_eq!(
            snapshot.map_size,
            V {
                x: 1000.0,
                y: 800.0
            }
        );
        assert_eq!(snapshot.bounties.len(), 1);
        assert_eq!(snapshot.bounties[0].points, 7);
    }

    #[test]
    fn parses_realtime_error_envelope_as_error() {
        assert_eq!(
            parse_realtime_snapshot(r#"{"type":"error","error":"observer is read-only"}"#)
                .unwrap_err(),
            "observer is read-only"
        );
    }

    #[test]
    fn own_death_counter_counts_each_alive_to_destroyed_transition_once() {
        let snapshot = |status: &str| {
            serde_json::from_str::<Desert>(&format!(
                r#"{{"transports":[{{"id":"player_1_0","status":"{status}"}}]}}"#
            ))
            .unwrap()
        };
        let alive = snapshot("normal");
        let dead = snapshot("destroyed");
        let respawned = snapshot("normal");
        let legacy_dead = snapshot("dead");

        assert_eq!(own_death_transitions(None, &dead), 0);
        assert_eq!(own_death_transitions(Some(&alive), &dead), 1);
        assert_eq!(own_death_transitions(Some(&dead), &dead), 0);
        assert_eq!(own_death_transitions(Some(&respawned), &dead), 1);
        assert_eq!(own_death_transitions(Some(&alive), &legacy_dead), 1);
    }

    #[test]
    fn manual_lease_token_hash_matches_player_two() {
        assert_eq!(token_hash("player_2"), 0x20252df859219fad);
    }

    #[test]
    fn score_rate_uses_time_to_last_collected_bounty() {
        let track = Track {
            points: vec![],
            point_times: vec![],
            score: 12,
            last_time: 3.0,
            rank_score: 12,
            rank_last_time: 3.0,
            rank_first_bounty_time: Some(1.0),
            rank_death_time: None,
            rank_speed_integral: 0.0,
            rank_speed_duration: 0.0,
            rank_terminal_speed: 0.0,
            duration: 10.0,
            death: None,
            angle: 0.0,
            current: false,
            rank: 0,
            rank_total: 0,
            top1: false,
        };
        assert_eq!(rate(&track), 4.0);
        let immediate = Track {
            last_time: 0.0,
            ..track
        };
        assert_eq!(rate(&immediate), 60.0);
    }

    #[test]
    fn multiple_bounties_in_one_step_use_latest_actual_collection_time() {
        let world = Desert {
            bounties: vec![
                Bounty {
                    x: 128.0,
                    y: 500.0,
                    radius: 1.0,
                    points: 20,
                },
                Bounty {
                    x: 125.0,
                    y: 500.0,
                    radius: 1.0,
                    points: 10,
                },
            ],
            map_size: V {
                x: 1000.0,
                y: 1000.0,
            },
            max_speed: 100.0,
            max_accel: 0.0,
            transport_radius: 1.0,
            ..Desert::default()
        };
        let transport = Transport {
            id: "p".into(),
            x: 100.0,
            y: 500.0,
            velocity: V { x: 100.0, y: 0.0 },
            ..Transport::default()
        };
        let grid = BountyGrid::new(&world.bounties, world.transport_radius);
        let track = forecast(
            &world,
            &grid,
            &transport,
            world.transport_radius,
            0.0,
            ForecastSettings {
                horizon: 0.4,
                time_step: 0.2,
                anomaly_scale: 1.0,
                command_mode: ForecastCommandMode::CandidateAfterLatency,
            },
        );
        assert_eq!(track.score, 30);
        assert!(track.last_time >= track.rank_first_bounty_time.unwrap());
        assert!(track.rank_last_time > track.rank_first_bounty_time.unwrap());
    }

    #[test]
    fn current_route_rank_uses_unfiltered_agile_top1_score_rate() {
        let route = |score, last_time| Track {
            points: vec![],
            point_times: vec![],
            score,
            last_time,
            rank_score: score,
            rank_last_time: last_time,
            rank_first_bounty_time: Some(last_time),
            rank_death_time: None,
            rank_speed_integral: 0.0,
            rank_speed_duration: 0.0,
            rank_terminal_speed: 0.0,
            duration: 30.0,
            death: None,
            angle: 0.0,
            current: false,
            rank: 0,
            rank_total: 0,
            top1: false,
        };
        let candidates = vec![route(30, 3.0), route(6, 2.0)];
        assert_eq!(
            current_route_rank(&route(16, 4.0), &candidates),
            Some((2, 3))
        );
        assert_eq!(
            current_route_rank(&route(500, 1.0), &candidates),
            Some((1, 3))
        );
    }

    #[test]
    fn segment_circle_detects_a_coin_between_ticks() {
        let t = segment_circle_hit(V { x: -5.0, y: 0.0 }, V { x: 5.0, y: 0.0 }, 1.0).unwrap();
        assert!((t - 0.4).abs() < 1e-9);
    }

    #[test]
    fn risk_map_uses_forecast_eta_near_route_segments() {
        let points = [V { x: 0.0, y: 0.0 }, V { x: 100.0, y: 0.0 }];
        let times = [0.0, 10.0];
        assert_eq!(
            trajectory_eta_near(V { x: 50.0, y: 5.0 }, &points, &times, 10.0),
            Some(5.0)
        );
        assert_eq!(
            trajectory_eta_near(V { x: 50.0, y: 20.0 }, &points, &times, 10.0),
            None
        );
        let route = Track {
            points: points.to_vec(),
            point_times: times.to_vec(),
            score: 0,
            last_time: 0.0,
            rank_score: 0,
            rank_last_time: 0.0,
            rank_first_bounty_time: None,
            rank_death_time: None,
            rank_speed_integral: 0.0,
            rank_speed_duration: 0.0,
            rank_terminal_speed: 0.0,
            duration: 10.0,
            death: None,
            angle: 0.0,
            current: true,
            rank: 0,
            rank_total: 0,
            top1: false,
        };
        assert_eq!(
            risk_map_eta(
                V { x: 50.0, y: 5.0 },
                10.0,
                &[&route],
                &[V::default()],
                100.0,
                15.0,
            ),
            5.0
        );
        assert_eq!(
            risk_map_eta(
                V { x: 1000.0, y: 0.0 },
                10.0,
                &[&route],
                &[V::default()],
                100.0,
                15.0,
            ),
            10.0
        );
    }

    #[test]
    fn interpolation_is_midpoint_and_vector_visibility_has_pixel_floor() {
        let mid = lerp_vec(V { x: 0.0, y: 2.0 }, V { x: 10.0, y: 6.0 }, 0.5);
        assert_eq!(mid, V { x: 5.0, y: 4.0 });
        assert_eq!(vector_pixels(0.001, 40.0, 64.0), Some(8.0));
        assert_eq!(vector_pixels(0.0, 40.0, 64.0), None);
    }

    #[test]
    fn bounty_radius_scales_linearly_with_zoom_without_a_minimum_size_clamp() {
        let world_radius = 8.0;
        let zoomed_out = world_radius_pixels(world_radius, 0.1);
        let normal = world_radius_pixels(world_radius, 1.0);
        let zoomed_in = world_radius_pixels(world_radius, 2.5);
        assert!((zoomed_out - 0.8).abs() < 1e-6);
        assert!((normal - 8.0).abs() < 1e-6);
        assert!((zoomed_in - 20.0).abs() < 1e-6);
        assert!((zoomed_in / zoomed_out - 25.0).abs() < 1e-5);
    }

    #[test]
    fn anomaly_field_opacity_increases_with_force_and_is_bounded() {
        let alphas = [0.0, 3.0, 15.0, 35.0, 100.0].map(anomaly_field_alpha);
        assert!(alphas.windows(2).all(|pair| pair[0] <= pair[1]));
        assert_eq!(alphas[0], 20);
        assert_eq!(alphas[4], 92);
        assert_eq!(anomaly_field_alpha(-35.0), anomaly_field_alpha(35.0));
        assert_eq!(anomaly_field_alpha(f64::NAN), 20);
    }

    #[test]
    fn anomaly_field_vector_uses_signed_force_and_sums_contributions() {
        let anomaly = |x, strength| Anomaly {
            x,
            effective_radius: 50.0,
            strength,
            ..Anomaly::default()
        };
        let point = V::default();
        let map = V {
            x: 1000.0,
            y: 1000.0,
        };
        let pull = anomaly_force_at(&[anomaly(10.0, 4.0)], point, 0.0, 1.0, map);
        let push = anomaly_force_at(&[anomaly(10.0, -4.0)], point, 0.0, 1.0, map);
        assert_eq!(pull, V { x: 4.0, y: 0.0 });
        assert_eq!(push, V { x: -4.0, y: 0.0 });

        let canceled = anomaly_force_at(
            &[anomaly(-10.0, 4.0), anomaly(10.0, 4.0)],
            point,
            0.0,
            1.0,
            map,
        );
        assert!(canceled.len() < 1e-9);
        assert_eq!(
            anomaly_force_at(&[anomaly(100.0, 4.0)], point, 0.0, 1.0, map),
            V::default()
        );
    }

    #[test]
    fn future_anomaly_field_despawns_after_leaving_the_arena() {
        let anomaly = Anomaly {
            x: 950.0,
            y: 500.0,
            effective_radius: 100.0,
            strength: 40.0,
            velocity: V { x: 200.0, y: 0.0 },
            ..Anomaly::default()
        };
        let map = V {
            x: 1000.0,
            y: 1000.0,
        };
        assert_eq!(
            anomaly_force_at(&[anomaly], V { x: 900.0, y: 500.0 }, 1.0, 1.0, map),
            V::default()
        );
    }

    #[test]
    fn anomaly_risk_heatmap_moves_from_safe_green_to_lethal_red() {
        let safe = anomaly_risk_color(0.1);
        let danger = anomaly_risk_color(0.5);
        let deadly = anomaly_risk_color(1.0);
        assert!(safe.a() < danger.a() && danger.a() < deadly.a());
        assert!(safe.g() > safe.r());
        assert!(deadly.r() > deadly.g());
        assert!(anomaly_risk_color(0.99).g() > deadly.g());
        assert_eq!(anomaly_risk_color(0.0), Color32::TRANSPARENT);
    }

    #[test]
    fn adaptive_risk_grid_is_shared_multiresolution_and_covers_the_map() {
        let map = V {
            x: 4000.0,
            y: 3000.0,
        };
        let focus = [
            V {
                x: 2000.0,
                y: 1500.0,
            },
            V {
                x: 1000.0,
                y: 1000.0,
            },
        ];
        let adaptive = risk_grid_cells(map, 100.0, &focus, &[], true);
        let uniform = risk_grid_cells(map, 100.0, &focus, &[], false);
        let area: f64 = adaptive
            .iter()
            .map(|cell| (cell.max.x - cell.min.x) * (cell.max.y - cell.min.y))
            .sum();
        assert!((area - map.x * map.y).abs() < 1e-6);
        assert!(adaptive.len() < uniform.len());

        let smallest_at = |point: V| {
            adaptive
                .iter()
                .filter(|cell| {
                    point.x >= cell.min.x
                        && point.x <= cell.max.x
                        && point.y >= cell.min.y
                        && point.y <= cell.max.y
                })
                .map(|cell| cell.size)
                .fold(f64::INFINITY, f64::min)
        };
        assert!(smallest_at(focus[0]) <= 100.0 + 1e-6);
        assert!(
            smallest_at(V {
                x: 1450.0,
                y: 1500.0
            }) <= 200.0 + 1e-6
        );
        assert!(
            smallest_at(V {
                x: 3900.0,
                y: 2900.0
            }) >= 300.0
        );
    }

    #[test]
    fn fifty_unit_local_step_stays_coarse_away_from_carpets_on_default_arena() {
        let map = V {
            x: 9000.0,
            y: 9000.0,
        };
        let focus = [
            V {
                x: 1800.0,
                y: 1800.0,
            },
            V {
                x: 3600.0,
                y: 2400.0,
            },
            V {
                x: 5400.0,
                y: 6000.0,
            },
            V {
                x: 7200.0,
                y: 4200.0,
            },
            V {
                x: 4500.0,
                y: 4500.0,
            },
        ];
        let adaptive = risk_grid_cells(map, 50.0, &focus, &[], true);
        let uniform = risk_grid_cells(map, 50.0, &focus, &[], false);
        assert!(adaptive.len() < 12000, "adaptive cells: {}", adaptive.len());
        assert_eq!(uniform.len(), 180 * 180);
        let far_resolution = adaptive
            .iter()
            .filter(|cell| {
                8200.0 >= cell.min.x
                    && 8200.0 <= cell.max.x
                    && 8200.0 >= cell.min.y
                    && 8200.0 <= cell.max.y
            })
            .map(|cell| cell.size)
            .fold(f64::INFINITY, f64::min);
        assert!(far_resolution >= 200.0, "far step: {far_resolution}");
        assert!(
            far_resolution < 400.0,
            "far step should not exceed 4x: {far_resolution}"
        );
    }

    #[test]
    fn route_focus_only_adds_a_narrow_medium_resolution_band() {
        let map = V {
            x: 1200.0,
            y: 1200.0,
        };
        let carpet = [V { x: 100.0, y: 100.0 }];
        let route = [V { x: 600.0, y: 600.0 }];
        let cells = risk_grid_cells(map, 50.0, &carpet, &route, true);
        let size_at = |point: V| {
            cells
                .iter()
                .find(|cell| {
                    point.x >= cell.min.x
                        && point.x <= cell.max.x
                        && point.y >= cell.min.y
                        && point.y <= cell.max.y
                })
                .map(|cell| cell.size)
                .unwrap()
        };
        assert!(size_at(route[0]) <= 100.0 + 1e-6);
        assert!(size_at(V { x: 900.0, y: 600.0 }) >= 200.0);
        assert!(size_at(carpet[0]) <= 50.0 + 1e-6);
    }

    #[test]
    fn forecast_spacing_keeps_near_steps_dense_and_extends_far_horizon() {
        assert!((forecast_step_for_time(0.0, 0.2) - 0.2).abs() < 1e-9);
        assert!((forecast_step_for_time(5.0, 0.2) - 0.2).abs() < 1e-9);
        assert!((forecast_step_for_time(15.0, 0.2) - 0.2).abs() < 1e-9);
        assert!(forecast_step_for_time(20.0, 0.2) > 0.2);
        assert!(forecast_step_for_time(75.0, 0.2) < 0.8);
        assert!(forecast_step_for_time(1000.0, 0.2) <= 0.8 + 1e-9);
        assert!(forecast_step_for_time(20.0, 0.2) > forecast_step_for_time(10.0, 0.2));
    }

    #[test]
    fn anomaly_death_risk_marks_unescapable_cores_and_edge_push() {
        let anomaly = Anomaly {
            x: 500.0,
            y: 500.0,
            radius: 10.0,
            effective_radius: 300.0,
            strength: 80.0,
            ..Anomaly::default()
        };
        let risk = |point, anomalies: &[Anomaly]| {
            anomaly_death_risk_at(
                anomalies,
                point,
                0.0,
                1.0,
                40.0,
                110.0,
                5.0,
                V {
                    x: 1000.0,
                    y: 1000.0,
                },
                350.0,
            )
        };
        assert_eq!(risk(V { x: 510.0, y: 500.0 }, &[anomaly.clone()]), 1.0);
        let near_trap = risk(V { x: 700.0, y: 500.0 }, &[anomaly.clone()]);
        assert!(near_trap > 0.8);
        assert_eq!(risk(V { x: 220.0, y: 500.0 }, &[anomaly.clone()]), 1.0);
        assert_eq!(risk(V { x: 900.0, y: 500.0 }, &[]), 0.0);

        let edge_repeller = Anomaly {
            x: 300.0,
            y: 500.0,
            radius: 10.0,
            effective_radius: 500.0,
            strength: -80.0,
            ..Anomaly::default()
        };
        assert!(risk(V { x: 100.0, y: 500.0 }, &[edge_repeller]) > 0.5);
    }

    #[test]
    fn anomaly_field_arrows_shrink_with_zoomed_out_grid_cells() {
        let whole_map = anomaly_field_arrow_length(1.0, 20.0);
        let zoomed_in = anomaly_field_arrow_length(1.0, 100.0);
        assert!((whole_map - 8.4).abs() < 1e-5);
        assert_eq!(zoomed_in, 16.0);
        assert!(whole_map < zoomed_in);
        assert_eq!(anomaly_field_arrow_length(0.0, 20.0), 0.0);
    }

    #[test]
    fn recent_path_is_time_bounded_and_ends_at_interpolated_position() {
        let base = Instant::now();
        let mut samples = VecDeque::new();
        for second in 0..=4 {
            samples.push_back((
                base + Duration::from_secs(second),
                V {
                    x: second as f64,
                    y: 0.0,
                },
            ));
        }
        let endpoint = V { x: 3.5, y: 0.0 };
        let path = history_path(
            &samples,
            base + Duration::from_secs(1),
            base + Duration::from_secs(3),
            endpoint,
        );
        assert_eq!(
            path,
            vec![
                V { x: 1.0, y: 0.0 },
                V { x: 2.0, y: 0.0 },
                V { x: 3.0, y: 0.0 },
                endpoint,
            ]
        );
    }

    #[test]
    fn records_disappeared_bounty_only_when_a_carpet_crossed_it() {
        let bounty = Bounty {
            x: 50.0,
            y: 20.0,
            radius: 5.0,
            points: 10,
        };
        let previous = Desert {
            transport_radius: 5.0,
            bounties: vec![bounty],
            transports: vec![Transport {
                id: "carpet-1".into(),
                x: 0.0,
                y: 20.0,
                ..Transport::default()
            }],
            ..Desert::default()
        };
        let current = Desert {
            transport_radius: 5.0,
            transports: vec![Transport {
                id: "carpet-1".into(),
                x: 100.0,
                y: 20.0,
                ..Transport::default()
            }],
            ..Desert::default()
        };
        assert_eq!(
            collected_bounties_between(&previous, &current),
            vec![("carpet-1".into(), V { x: 50.0, y: 20.0 }, 10)]
        );

        let missed = Desert {
            transports: vec![Transport {
                id: "carpet-1".into(),
                x: 100.0,
                y: 100.0,
                ..Transport::default()
            }],
            ..current
        };
        assert!(collected_bounties_between(&previous, &missed).is_empty());
    }

    #[test]
    fn session_score_balance_reports_cumulative_points_lost() {
        assert_eq!(session_score_lost(0, 150, 100), 50);
        assert_eq!(session_score_lost(40, 0, 0), 40);
        assert_eq!(session_score_lost(20, 10, 30), 0);
    }

    #[test]
    fn ranked_routes_receive_distinct_colors() {
        let colors = (1..=10).map(route_rank_color).collect::<Vec<_>>();
        for (index, color) in colors.iter().enumerate() {
            assert!(!colors[index + 1..].contains(color));
        }
    }

    #[test]
    fn trajectory_time_gradient_moves_from_green_to_magenta() {
        let soon = trajectory_time_color(0.0, 15.0);
        let middle = trajectory_time_color(7.5, 15.0);
        let later = trajectory_time_color(15.0, 15.0);
        assert!(soon.g() > soon.r());
        assert!(middle.r() > middle.g());
        assert!(later.r() > later.g());
        assert_ne!(soon, middle);
        assert_ne!(middle, later);
        assert_eq!(trajectory_time_color(30.0, 15.0), later);
    }

    #[test]
    fn current_forecast_holds_the_last_applied_acceleration() {
        let world = Desert {
            map_size: V {
                x: 10_000.0,
                y: 10_000.0,
            },
            max_accel: 40.0,
            max_speed: 110.0,
            transport_radius: 5.0,
            ..Desert::default()
        };
        let transport = Transport {
            x: 5_000.0,
            y: 5_000.0,
            velocity: V { x: 30.0, y: 0.0 },
            self_acceleration: V { x: 0.0, y: 40.0 },
            ..Transport::default()
        };
        let grid = BountyGrid::new(&[], world.transport_radius);
        let held = forecast(
            &world,
            &grid,
            &transport,
            world.transport_radius,
            0.0,
            ForecastSettings {
                horizon: 1.0,
                time_step: 0.2,
                anomaly_scale: 1.0,
                command_mode: ForecastCommandMode::HoldCurrent,
            },
        );
        let candidate = forecast(
            &world,
            &grid,
            &transport,
            world.transport_radius,
            0.0,
            ForecastSettings {
                horizon: 1.0,
                time_step: 0.2,
                anomaly_scale: 1.0,
                command_mode: ForecastCommandMode::CandidateAfterLatency,
            },
        );
        assert!(held.points.last().unwrap().y > candidate.points.last().unwrap().y);
        assert_eq!(held.points.len(), held.point_times.len());
        assert_eq!(candidate.points.len(), candidate.point_times.len());
    }

    #[test]
    fn desert_uses_camel_case_external_fields() {
        let d: Desert = serde_json::from_str(r#"{"mapSize":{"x":2200,"y":1600},"maxAccel":40,"maxSpeed":110,"transportRadius":7,"anomalies":[{"x":10,"y":20,"radius":4,"effectiveRadius":50,"strength":-3,"velocity":{"x":1,"y":0}}],"bounties":[{"x":30,"y":40,"radius":7,"points":5}],"transports":[{"id":"player_2_1","status":"alive","deathCount":7,"x":2,"y":3,"velocity":{"x":4,"y":5},"selfAcceleration":{"x":1,"y":0},"anomalyAcceleration":{"x":-2,"y":0}}]}"#).unwrap();
        assert_eq!(d.map_size.x, 2200.0);
        assert_eq!(d.max_speed, 110.0);
        assert_eq!(d.transports[0].self_acceleration.x, 1.0);
        assert_eq!(d.transports[0].death_count, 7);
        assert_eq!(d.anomalies[0].strength, -3.0);
    }
}
