//! Session-owned synthetic pointer state and bounded motion planning.
//!
//! The synthetic pointer is deliberately separate from the hardware cursor.
//! It is composited into exact-window frames so both the model and a person
//! watching the shared frame can see the agent's attention and last action.

use std::f64::consts::PI;
use std::time::Duration;

use image::{Rgba, RgbaImage};
use serde::Serialize;

#[cfg(test)]
use super::map_image_point;
use super::{FrameState, TargetPoint, WindowDescriptor, now_iso};

const FRAME_INTERVAL_MS: u64 = 16;
const DEFAULT_PEAK_SPEED: f64 = 900.0;
const MIN_DURATION_MS: u64 = 110;
const MAX_DURATION_MS: u64 = 850;
const CANDIDATE_COUNT: usize = 20;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CursorStyle {
    pub theme: &'static str,
    pub fill: String,
    pub outline: &'static str,
    pub logical_size: u32,
    pub hotspot: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CursorSnapshot {
    pub id: String,
    pub visible: bool,
    pub window_id: Option<String>,
    pub image_x: Option<f64>,
    pub image_y: Option<f64>,
    pub screen_x: Option<i32>,
    pub screen_y: Option<i32>,
    pub heading_degrees: f64,
    pub action: String,
    pub pressed: bool,
    pub sequence: u64,
    pub revision: u64,
    pub buttons_mask: u8,
    pub updated_at: String,
    pub coordinate_space: &'static str,
    pub style: CursorStyle,
}

#[derive(Debug, Clone)]
pub(crate) struct CursorTrajectory {
    pub points: Vec<TargetPoint>,
    pub duration_ms: u64,
    pub sequence: u64,
    pub heading_radians: f64,
    pub seeded: bool,
    pub path_score: f64,
}

impl CursorTrajectory {
    pub(crate) fn step_delay(&self) -> Duration {
        Duration::from_millis((self.duration_ms / self.points.len().max(1) as u64).max(1))
    }

    pub(crate) fn metadata(&self) -> serde_json::Value {
        serde_json::json!({
            "curve": "bounded-cubic-bezier-minimum-jerk",
            "profile": "candidate-scored-spring-v1",
            "candidateCount": CANDIDATE_COUNT,
            "pathScore": self.path_score,
            "durationMs": self.duration_ms,
            "steps": self.points.len(),
            "sequence": self.sequence,
            "seeded": self.seeded,
            "arrivalHeadingDegrees": self.heading_radians.to_degrees(),
            "arrivalAcknowledged": true,
        })
    }
}

#[derive(Debug, Clone)]
pub(crate) struct SyntheticCursor {
    id: String,
    window_id: Option<String>,
    local_x: Option<i32>,
    local_y: Option<i32>,
    heading_radians: f64,
    visible: bool,
    action: String,
    pressed: bool,
    sequence: u64,
    updated_at: String,
    fill: [u8; 4],
}

impl SyntheticCursor {
    pub(crate) fn new(id: String) -> Self {
        let fill = session_color(&id);
        Self {
            id,
            window_id: None,
            local_x: None,
            local_y: None,
            heading_radians: PI / 4.0,
            visible: false,
            action: "idle".to_owned(),
            pressed: false,
            sequence: 0,
            updated_at: now_iso(),
            fill,
        }
    }

    pub(crate) fn plan(
        &self,
        frame: &FrameState,
        destination: TargetPoint,
        requested_duration_ms: Option<u64>,
        _action: &str,
    ) -> CursorTrajectory {
        let same_window = self.window_id.as_deref() == Some(frame.target.id.as_str());
        let seeded =
            !self.visible || !same_window || self.local_x.is_none() || self.local_y.is_none();
        let start = if seeded {
            TargetPoint {
                local_x: destination.local_x.saturating_sub(140).max(0),
                local_y: destination.local_y.saturating_sub(140).max(0),
                screen_x: destination.screen_x.saturating_sub(140).max(frame.target.x),
                screen_y: destination.screen_y.saturating_sub(140).max(frame.target.y),
            }
        } else {
            let local_x = self.local_x.unwrap_or(destination.local_x);
            let local_y = self.local_y.unwrap_or(destination.local_y);
            TargetPoint {
                local_x,
                local_y,
                screen_x: frame.target.x.saturating_add(local_x),
                screen_y: frame.target.y.saturating_add(local_y),
            }
        };
        let distance = f64::from(destination.local_x - start.local_x)
            .hypot(f64::from(destination.local_y - start.local_y));
        let duration_ms = requested_duration_ms.unwrap_or_else(|| {
            ((distance / DEFAULT_PEAK_SPEED * 1_000.0) as u64)
                .clamp(MIN_DURATION_MS, MAX_DURATION_MS)
        });
        let steps = (duration_ms / FRAME_INTERVAL_MS).clamp(8, 120) as usize;
        let curve = BoundedBezier::select(
            start,
            destination,
            &frame.target,
            self.sequence.saturating_add(1),
        );
        let mut points = Vec::with_capacity(steps);
        let mut prior = (f64::from(start.local_x), f64::from(start.local_y));
        let mut heading = self.heading_radians;
        for step in 1..=steps {
            let time = step as f64 / steps as f64;
            let progress = spring_minimum_jerk(time);
            let (x, y) = curve.sample(progress);
            let dx = x - prior.0;
            let dy = y - prior.1;
            if dx.hypot(dy) > 0.01 {
                heading = dy.atan2(dx);
            }
            let local_x =
                x.round()
                    .clamp(0.0, frame.target.width.saturating_sub(1) as f64) as i32;
            let local_y = y
                .round()
                .clamp(0.0, frame.target.height.saturating_sub(1) as f64)
                as i32;
            let point = TargetPoint {
                local_x,
                local_y,
                screen_x: frame.target.x.saturating_add(local_x),
                screen_y: frame.target.y.saturating_add(local_y),
            };
            if points.last().is_none_or(|prior: &TargetPoint| {
                prior.local_x != point.local_x || prior.local_y != point.local_y
            }) {
                points.push(point);
            }
            prior = (x, y);
        }
        if points.last().is_none_or(|point| {
            point.local_x != destination.local_x || point.local_y != destination.local_y
        }) {
            points.push(destination);
        }

        CursorTrajectory {
            points,
            duration_ms,
            sequence: self.sequence.saturating_add(1),
            heading_radians: heading,
            seeded,
            path_score: curve.score,
        }
    }

    pub(crate) fn commit(
        &mut self,
        frame: &FrameState,
        trajectory: &CursorTrajectory,
        action: &str,
    ) {
        let Some(destination) = trajectory.points.last() else {
            return;
        };
        self.sequence = trajectory.sequence.max(self.sequence.saturating_add(1));
        self.window_id = Some(frame.target.id.clone());
        self.local_x = Some(destination.local_x);
        self.local_y = Some(destination.local_y);
        self.heading_radians = trajectory.heading_radians;
        self.visible = true;
        self.action = action.to_owned();
        self.pressed = action == "drag";
        self.updated_at = now_iso();
    }

    pub(crate) fn mark_unknown(&mut self, action: &str) {
        self.sequence = self.sequence.saturating_add(1);
        self.window_id = None;
        self.local_x = None;
        self.local_y = None;
        self.visible = false;
        self.action = action.to_owned();
        self.pressed = false;
        self.updated_at = now_iso();
    }

    pub(crate) fn settle(&mut self, action: &str) {
        self.action = action.to_owned();
        self.pressed = false;
        self.updated_at = now_iso();
    }

    pub(crate) fn snapshot(&self, frame: Option<&FrameState>) -> CursorSnapshot {
        let (image_x, image_y) = match (
            frame.filter(|frame| self.window_id.as_deref() == Some(frame.target.id.as_str())),
            self.local_x,
            self.local_y,
        ) {
            (Some(frame), Some(x), Some(y)) => (
                Some(
                    f64::from(x) / f64::from(frame.target.width.max(1))
                        * f64::from(frame.image_width),
                ),
                Some(
                    f64::from(y) / f64::from(frame.target.height.max(1))
                        * f64::from(frame.image_height),
                ),
            ),
            _ => (None, None),
        };
        let screen_x = self
            .local_x
            .zip(frame)
            .filter(|(_, frame)| self.window_id.as_deref() == Some(frame.target.id.as_str()))
            .map(|(x, frame)| frame.target.x.saturating_add(x));
        let screen_y = self
            .local_y
            .zip(frame)
            .filter(|(_, frame)| self.window_id.as_deref() == Some(frame.target.id.as_str()))
            .map(|(y, frame)| frame.target.y.saturating_add(y));
        CursorSnapshot {
            id: self.id.clone(),
            visible: self.visible && image_x.is_some(),
            window_id: self.window_id.clone(),
            image_x,
            image_y,
            screen_x,
            screen_y,
            heading_degrees: self.heading_radians.to_degrees(),
            action: self.action.clone(),
            pressed: self.pressed,
            sequence: self.sequence,
            revision: self.sequence,
            buttons_mask: u8::from(self.pressed),
            updated_at: self.updated_at.clone(),
            coordinate_space: "image-pixels",
            style: CursorStyle {
                theme: "lbb.session-pointer.v1",
                fill: format!(
                    "#{:02X}{:02X}{:02X}",
                    self.fill[0], self.fill[1], self.fill[2]
                ),
                outline: "#FFFFFF",
                logical_size: 42,
                hotspot: "tip",
            },
        }
    }

    pub(crate) fn composite(&self, image: &mut RgbaImage, target: &WindowDescriptor) {
        if !self.visible || self.window_id.as_deref() != Some(target.id.as_str()) {
            return;
        }
        let (Some(local_x), Some(local_y)) = (self.local_x, self.local_y) else {
            return;
        };
        let x = f64::from(local_x) / f64::from(target.width.max(1)) * f64::from(image.width());
        let y = f64::from(local_y) / f64::from(target.height.max(1)) * f64::from(image.height());
        let scale = (f64::from(image.width()) / f64::from(target.width.max(1))).clamp(0.65, 2.5);
        paint_pointer(
            image,
            x,
            y,
            self.heading_radians,
            scale,
            self.fill,
            &self.action,
        );
    }
}

#[derive(Debug, Clone, Copy)]
struct BoundedBezier {
    p0: (f64, f64),
    p1: (f64, f64),
    p2: (f64, f64),
    p3: (f64, f64),
    score: f64,
}

impl BoundedBezier {
    fn select(
        start: TargetPoint,
        end: TargetPoint,
        target: &WindowDescriptor,
        sequence: u64,
    ) -> Self {
        let p0 = (f64::from(start.local_x), f64::from(start.local_y));
        let p3 = (f64::from(end.local_x), f64::from(end.local_y));
        let dx = p3.0 - p0.0;
        let dy = p3.1 - p0.1;
        let distance = dx.hypot(dy).max(1.0);
        let perpendicular = (-dy / distance, dx / distance);
        (0..CANDIDATE_COUNT)
            .map(|index| {
                let sign = if index % 2 == 0 { 1.0 } else { -1.0 };
                let bend_scale = if index == 0 {
                    0.0
                } else {
                    0.025 + deterministic_unit(sequence, index * 5) * 0.13
                };
                let bend = (distance * bend_scale).clamp(0.0, 64.0) * sign;
                let first_progress = 0.22 + deterministic_unit(sequence, index * 5 + 1) * 0.20;
                let second_progress = 0.62 + deterministic_unit(sequence, index * 5 + 2) * 0.22;
                let first_bend = bend * (0.70 + deterministic_unit(sequence, index * 5 + 3) * 0.45);
                let second_bend =
                    bend * (0.15 + deterministic_unit(sequence, index * 5 + 4) * 0.55);
                let max_x = f64::from(target.width.saturating_sub(1));
                let max_y = f64::from(target.height.saturating_sub(1));
                let mut candidate = Self {
                    p0,
                    p1: (
                        (p0.0 + dx * first_progress + perpendicular.0 * first_bend)
                            .clamp(0.0, max_x),
                        (p0.1 + dy * first_progress + perpendicular.1 * first_bend)
                            .clamp(0.0, max_y),
                    ),
                    p2: (
                        (p0.0 + dx * second_progress + perpendicular.0 * second_bend)
                            .clamp(0.0, max_x),
                        (p0.1 + dy * second_progress + perpendicular.1 * second_bend)
                            .clamp(0.0, max_y),
                    ),
                    p3,
                    score: 0.0,
                };
                candidate.score = candidate.penalty(target);
                candidate
            })
            .min_by(|left, right| left.score.total_cmp(&right.score))
            .unwrap_or(Self {
                p0,
                p1: p0,
                p2: p3,
                p3,
                score: 0.0,
            })
    }

    fn sample(self, t: f64) -> (f64, f64) {
        let t = t.clamp(0.0, 1.0);
        let u = 1.0 - t;
        (
            u.powi(3) * self.p0.0
                + 3.0 * u.powi(2) * t * self.p1.0
                + 3.0 * u * t.powi(2) * self.p2.0
                + t.powi(3) * self.p3.0,
            u.powi(3) * self.p0.1
                + 3.0 * u.powi(2) * t * self.p1.1
                + 3.0 * u * t.powi(2) * self.p2.1
                + t.powi(3) * self.p3.1,
        )
    }

    fn penalty(self, target: &WindowDescriptor) -> f64 {
        let samples = (0..=32)
            .map(|index| self.sample(index as f64 / 32.0))
            .collect::<Vec<_>>();
        let direct = (self.p3.0 - self.p0.0)
            .hypot(self.p3.1 - self.p0.1)
            .max(1.0);
        let direction = (
            (self.p3.0 - self.p0.0) / direct,
            (self.p3.1 - self.p0.1) / direct,
        );
        let mut length = 0.0;
        let mut reverse = 0.0;
        let mut boundary = 0.0;
        let mut curvature = 0.0;
        let mut prior_segment: Option<(f64, f64, f64)> = None;
        for (index, pair) in samples.windows(2).enumerate() {
            let segment = (pair[1].0 - pair[0].0, pair[1].1 - pair[0].1);
            let segment_length = segment.0.hypot(segment.1);
            length += segment_length;
            let forward = segment.0 * direction.0 + segment.1 * direction.1;
            if forward < 0.0 {
                reverse += -forward;
            }
            if index + 1 < samples.len() - 1 {
                let point = pair[1];
                let clearance = point
                    .0
                    .min(point.1)
                    .min(f64::from(target.width.saturating_sub(1)) - point.0)
                    .min(f64::from(target.height.saturating_sub(1)) - point.1);
                if clearance < 6.0 {
                    boundary += (6.0 - clearance).powi(2);
                }
            }
            if let Some((prior_x, prior_y, prior_length)) = prior_segment
                && segment_length > 0.0
                && prior_length > 0.0
            {
                let cosine = ((segment.0 * prior_x + segment.1 * prior_y)
                    / (segment_length * prior_length))
                    .clamp(-1.0, 1.0);
                curvature += cosine.acos().powi(2);
            }
            prior_segment = Some((segment.0, segment.1, segment_length));
        }
        let ratio = length / direct;
        let length_penalty = (ratio - 1.035).abs() * 42.0 + (ratio - 1.2).max(0.0) * 180.0;
        length_penalty + curvature * 22.0 + reverse * 80.0 + boundary * 14.0
    }
}

fn minimum_jerk(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    6.0 * t.powi(5) - 15.0 * t.powi(4) + 10.0 * t.powi(3)
}

fn spring_minimum_jerk(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    let rate: f64 = 7.5;
    let spring_end = 1.0 - (1.0 + rate) * (-rate).exp();
    let spring = ((1.0 - (1.0 + rate * t) * (-rate * t).exp()) / spring_end).clamp(0.0, 1.0);
    (minimum_jerk(t) * 0.78 + spring * 0.22).clamp(0.0, 1.0)
}

fn deterministic_unit(seed: u64, index: usize) -> f64 {
    let mut value = (seed as u32)
        .wrapping_add(1)
        .wrapping_mul(0x9e37_79b1)
        .wrapping_add((index as u32).wrapping_add(1).wrapping_mul(0x85eb_ca6b));
    value ^= value >> 16;
    value = value.wrapping_mul(0x7feb_352d);
    value ^= value >> 15;
    value = value.wrapping_mul(0x846c_a68b);
    value ^= value >> 16;
    f64::from(value) / 4_294_967_296.0
}

fn session_color(id: &str) -> [u8; 4] {
    const PALETTE: [[u8; 4]; 6] = [
        [38, 198, 255, 255],
        [184, 245, 93, 255],
        [176, 132, 255, 255],
        [255, 122, 184, 255],
        [255, 181, 71, 255],
        [51, 224, 190, 255],
    ];
    let hash = id.bytes().fold(2_166_136_261_u32, |hash, byte| {
        (hash ^ u32::from(byte)).wrapping_mul(16_777_619)
    });
    PALETTE[hash as usize % PALETTE.len()]
}

fn paint_pointer(
    image: &mut RgbaImage,
    x: f64,
    y: f64,
    heading: f64,
    scale: f64,
    fill: [u8; 4],
    action: &str,
) {
    let arrow = [
        (0.0, 0.0),
        (0.0, 27.0),
        (7.0, 21.0),
        (12.0, 34.0),
        (18.0, 31.0),
        (13.0, 19.0),
        (24.0, 19.0),
    ];
    let rotation = heading - PI / 4.0;
    let transformed = |outline_scale: f64| {
        arrow
            .iter()
            .map(|(px, py)| {
                let px = px * scale * outline_scale;
                let py = py * scale * outline_scale;
                (
                    x + px * rotation.cos() - py * rotation.sin(),
                    y + px * rotation.sin() + py * rotation.cos(),
                )
            })
            .collect::<Vec<_>>()
    };

    draw_radial_glow(image, x + 8.0 * scale, y + 11.0 * scale, 17.0 * scale, fill);
    if action == "click" || action == "doubleClick" || action == "rightClick" {
        draw_ring(
            image,
            x,
            y,
            18.0 * scale,
            2.4 * scale,
            [fill[0], fill[1], fill[2], 205],
        );
    } else if action == "drag" {
        draw_ring(image, x, y, 13.0 * scale, 3.0 * scale, [255, 255, 255, 185]);
    }
    fill_polygon(image, &transformed(1.13), [255, 255, 255, 255]);
    fill_polygon(image, &transformed(1.0), fill);
}

fn draw_radial_glow(image: &mut RgbaImage, cx: f64, cy: f64, radius: f64, color: [u8; 4]) {
    let min_x = (cx - radius).floor().max(0.0) as u32;
    let max_x = (cx + radius)
        .ceil()
        .min(f64::from(image.width().saturating_sub(1))) as u32;
    let min_y = (cy - radius).floor().max(0.0) as u32;
    let max_y = (cy + radius)
        .ceil()
        .min(f64::from(image.height().saturating_sub(1))) as u32;
    for py in min_y..=max_y {
        for px in min_x..=max_x {
            let distance = (f64::from(px) - cx).hypot(f64::from(py) - cy);
            if distance <= radius {
                let alpha = ((1.0 - distance / radius).powi(2) * 92.0) as u8;
                blend(
                    image.get_pixel_mut(px, py),
                    [color[0], color[1], color[2], alpha],
                );
            }
        }
    }
}

fn draw_ring(image: &mut RgbaImage, cx: f64, cy: f64, radius: f64, width: f64, color: [u8; 4]) {
    let outer = radius + width / 2.0;
    let inner = (radius - width / 2.0).max(0.0);
    let min_x = (cx - outer).floor().max(0.0) as u32;
    let max_x = (cx + outer)
        .ceil()
        .min(f64::from(image.width().saturating_sub(1))) as u32;
    let min_y = (cy - outer).floor().max(0.0) as u32;
    let max_y = (cy + outer)
        .ceil()
        .min(f64::from(image.height().saturating_sub(1))) as u32;
    for py in min_y..=max_y {
        for px in min_x..=max_x {
            let distance = (f64::from(px) - cx).hypot(f64::from(py) - cy);
            if (inner..=outer).contains(&distance) {
                blend(image.get_pixel_mut(px, py), color);
            }
        }
    }
}

fn fill_polygon(image: &mut RgbaImage, polygon: &[(f64, f64)], color: [u8; 4]) {
    let min_x = polygon
        .iter()
        .map(|point| point.0)
        .fold(f64::INFINITY, f64::min)
        .floor()
        .max(0.0) as u32;
    let max_x = polygon
        .iter()
        .map(|point| point.0)
        .fold(f64::NEG_INFINITY, f64::max)
        .ceil()
        .min(f64::from(image.width().saturating_sub(1))) as u32;
    let min_y = polygon
        .iter()
        .map(|point| point.1)
        .fold(f64::INFINITY, f64::min)
        .floor()
        .max(0.0) as u32;
    let max_y = polygon
        .iter()
        .map(|point| point.1)
        .fold(f64::NEG_INFINITY, f64::max)
        .ceil()
        .min(f64::from(image.height().saturating_sub(1))) as u32;
    for py in min_y..=max_y {
        for px in min_x..=max_x {
            if point_in_polygon(f64::from(px) + 0.5, f64::from(py) + 0.5, polygon) {
                blend(image.get_pixel_mut(px, py), color);
            }
        }
    }
}

fn point_in_polygon(x: f64, y: f64, polygon: &[(f64, f64)]) -> bool {
    let mut inside = false;
    let mut previous = polygon.len().saturating_sub(1);
    for current in 0..polygon.len() {
        let (xi, yi) = polygon[current];
        let (xj, yj) = polygon[previous];
        if ((yi > y) != (yj > y)) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
            inside = !inside;
        }
        previous = current;
    }
    inside
}

fn blend(destination: &mut Rgba<u8>, source: [u8; 4]) {
    let alpha = f32::from(source[3]) / 255.0;
    let inverse = 1.0 - alpha;
    for channel in 0..3 {
        destination[channel] = (f32::from(source[channel]) * alpha
            + f32::from(destination[channel]) * inverse)
            .round() as u8;
    }
    destination[3] = 255;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame() -> FrameState {
        FrameState {
            id: "frame".to_owned(),
            target: WindowDescriptor {
                id: "window".to_owned(),
                pid: 1,
                app_name: "Fixture".to_owned(),
                title: "Cursor".to_owned(),
                x: -100,
                y: 40,
                width: 800,
                height: 600,
                minimized: false,
                focused: false,
            },
            image_width: 400,
            image_height: 300,
            elements: vec![],
            captured_at: std::time::Instant::now(),
        }
    }

    #[test]
    fn plans_bounded_minimum_jerk_trajectory_and_acknowledges_arrival() {
        let frame = frame();
        let destination = map_image_point(&frame, 350.0, 260.0);
        let cursor = SyntheticCursor::new("session-a".to_owned());
        let path = cursor.plan(&frame, destination, Some(320), "move");
        assert!(path.seeded);
        assert!((4..=120).contains(&path.points.len()));
        assert_eq!(path.points.last().unwrap().local_x, destination.local_x);
        assert_eq!(path.metadata()["arrivalAcknowledged"], true);
        assert!(path.points.iter().all(|point| {
            point.local_x >= 0
                && point.local_y >= 0
                && point.local_x < frame.target.width as i32
                && point.local_y < frame.target.height as i32
        }));
    }

    #[test]
    fn composites_session_pointer_without_touching_hardware_state() {
        let frame = frame();
        let destination = map_image_point(&frame, 200.0, 150.0);
        let mut cursor = SyntheticCursor::new("session-b".to_owned());
        let trajectory = cursor.plan(&frame, destination, Some(160), "click");
        assert!(!cursor.snapshot(Some(&frame)).visible);
        cursor.commit(&frame, &trajectory, "click");
        let mut image = RgbaImage::from_pixel(400, 300, Rgba([16, 18, 17, 255]));
        cursor.composite(&mut image, &frame.target);
        assert!(
            image
                .pixels()
                .any(|pixel| *pixel != Rgba([16, 18, 17, 255]))
        );
        let snapshot = cursor.snapshot(Some(&frame));
        assert!(snapshot.visible);
        assert_eq!(snapshot.action, "click");
        assert_eq!(snapshot.coordinate_space, "image-pixels");
    }

    #[test]
    fn planning_is_pure_until_delivery_is_committed() {
        let frame = frame();
        let destination = map_image_point(&frame, 240.0, 190.0);
        let mut cursor = SyntheticCursor::new("session-c".to_owned());
        let before = cursor.snapshot(Some(&frame));
        let trajectory = cursor.plan(&frame, destination, Some(160), "move");
        let planned = cursor.snapshot(Some(&frame));
        assert_eq!(planned.sequence, before.sequence);
        assert!(!planned.visible);

        cursor.commit(&frame, &trajectory, "move");
        let committed = cursor.snapshot(Some(&frame));
        assert_eq!(committed.sequence, before.sequence + 1);
        assert!(committed.visible);
    }

    #[test]
    fn session_palette_is_stable_and_distinguishes_common_ids() {
        assert_eq!(session_color("alpha"), session_color("alpha"));
        assert_ne!(session_color("alpha"), session_color("bravo"));
    }
}
