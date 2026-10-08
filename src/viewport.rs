// SPDX-License-Identifier: Apache-2.0
use eframe::egui::{self, Color32, FontId, Pos2, Sense, Stroke, Vec2};
use opp_viewer::geometry::Scene;

pub struct Camera {
    yaw: f64,
    pitch: f64,
    zoom: f64,
    pan: Vec2,
}
impl Default for Camera {
    fn default() -> Self {
        Self {
            yaw: 0.72,
            pitch: 0.52,
            zoom: 1.,
            pan: Vec2::ZERO,
        }
    }
}
impl Camera {
    pub fn reset(&mut self) {
        *self = Self::default();
    }
    pub fn fit(&mut self) {
        self.zoom = 1.;
        self.pan = Vec2::ZERO;
    }
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        scene: &Scene,
        selected_node: &str,
        scan_overlay: bool,
        subject: &str,
        height: f32,
    ) {
        let (response, painter) = ui.allocate_painter(
            Vec2::new(ui.available_width(), height.max(120.)),
            Sense::click_and_drag(),
        );
        let rect = response.rect;
        painter.rect_filled(rect, 4, Color32::from_rgb(248, 250, 251));
        if response.dragged_by(egui::PointerButton::Primary) {
            let delta = ui.input(|i| i.pointer.delta());
            self.yaw += f64::from(delta.x) * 0.008;
            self.pitch = (self.pitch + f64::from(delta.y) * 0.008).clamp(-1.4, 1.4);
        }
        if response.dragged_by(egui::PointerButton::Secondary) {
            self.pan += ui.input(|i| i.pointer.delta());
        }
        if response.hovered() {
            self.zoom = (self.zoom
                * (f64::from(ui.input(|i| i.smooth_scroll_delta.y)) * 0.002).exp())
            .clamp(0.15, 20.);
        }
        let (min, max) = scene.bounds();
        let center = std::array::from_fn::<_, 3, _>(|i| (min[i] + max[i]) * 0.5);
        let diagonal = (0..3)
            .map(|i| (max[i] - min[i]).powi(2))
            .sum::<f64>()
            .sqrt()
            .max(0.001);
        let scale = f64::from(rect.width().min(rect.height())) * 0.86 / diagonal * self.zoom;
        let project = |p: [f64; 3]| -> (Pos2, f64) {
            let p = std::array::from_fn::<_, 3, _>(|i| p[i] - center[i]);
            let x = p[0] * self.yaw.cos() - p[1] * self.yaw.sin();
            let y = p[0] * self.yaw.sin() + p[1] * self.yaw.cos();
            let up = p[2] * self.pitch.cos() - y * self.pitch.sin();
            (
                rect.center() + self.pan + Vec2::new((x * scale) as f32, (-up * scale) as f32),
                y * self.pitch.cos() + p[2] * self.pitch.sin(),
            )
        };
        if scene.parts.is_empty() {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "Geometry preview unavailable",
                FontId::proportional(16.),
                Color32::from_rgb(84, 105, 118),
            );
            return;
        }
        let grid = 10f64.powf((diagonal / 8.).log10().floor());
        for i in -12..=12 {
            let offset = f64::from(i) * grid;
            for (a, b) in [
                (
                    [center[0] - 12. * grid, center[1] + offset, min[2] - 0.03],
                    [center[0] + 12. * grid, center[1] + offset, min[2] - 0.03],
                ),
                (
                    [center[0] + offset, center[1] - 12. * grid, min[2] - 0.03],
                    [center[0] + offset, center[1] + 12. * grid, min[2] - 0.03],
                ),
            ] {
                painter.line_segment(
                    [project(a).0, project(b).0],
                    Stroke::new(0.5, Color32::from_rgb(228, 234, 237)),
                );
            }
        }
        let mut faces = Vec::new();
        for part in &scene.parts {
            for triangle in &part.mesh.triangles {
                let points = triangle.map(project);
                let u = std::array::from_fn::<_, 3, _>(|i| triangle[1][i] - triangle[0][i]);
                let v = std::array::from_fn::<_, 3, _>(|i| triangle[2][i] - triangle[0][i]);
                let n = [
                    u[1] * v[2] - u[2] * v[1],
                    u[2] * v[0] - u[0] * v[2],
                    u[0] * v[1] - u[1] * v[0],
                ];
                let norm = n.iter().map(|v| v * v).sum::<f64>().sqrt().max(0.00001);
                let shade = 0.72 + 0.24 * ((n[0] * -0.2 + n[1] * -0.4 + n[2] * 0.8) / norm).abs();
                let base = if selected_node != "root" && part.node_key == selected_node {
                    [102., 159., 149.]
                } else {
                    [188., 198., 203.]
                };
                let color = Color32::from_rgb(
                    (base[0] * shade) as u8,
                    (base[1] * shade) as u8,
                    (base[2] * shade) as u8,
                );
                faces.push((
                    (points[0].1 + points[1].1 + points[2].1) / 3.,
                    points.map(|p| p.0),
                    color,
                ));
            }
        }
        faces.sort_by(|a, b| a.0.total_cmp(&b.0));
        for (_, points, color) in faces {
            painter.add(egui::Shape::convex_polygon(
                points.to_vec(),
                color,
                Stroke::NONE,
            ));
        }
        for part in &scene.parts {
            for (a, b) in &part.mesh.edges {
                painter.line_segment(
                    [project(*a).0, project(*b).0],
                    Stroke::new(0.8, Color32::from_rgb(83, 103, 111)),
                );
            }
        }
        if scan_overlay {
            for scan in scene
                .scans
                .iter()
                .filter(|scan| subject.is_empty() || scan.subject_id == subject)
            {
                for p in &scan.points {
                    painter.circle_filled(project(*p).0, 3.8, Color32::from_rgb(24, 151, 132));
                }
            }
        }
        let axes_origin = Pos2::new(rect.left() + 34., rect.bottom() - 34.);
        let world_origin = project(center).0;
        for (axis, label, color) in [
            (0, "X", Color32::from_rgb(188, 66, 65)),
            (1, "Y", Color32::from_rgb(42, 143, 82)),
            (2, "Z", Color32::from_rgb(41, 113, 197)),
        ] {
            let mut p = center;
            p[axis] += diagonal / 10.;
            let d = project(p).0 - world_origin;
            let endpoint = axes_origin + d.normalized() * 24.;
            painter.arrow(axes_origin, endpoint - axes_origin, Stroke::new(1.4, color));
            painter.text(
                endpoint,
                egui::Align2::CENTER_CENTER,
                label,
                FontId::proportional(11.),
                color,
            );
        }
        painter.text(
            rect.right_bottom() - Vec2::new(14., 12.),
            egui::Align2::RIGHT_BOTTOM,
            "Drag to orbit · Scroll to zoom · Right-drag to pan",
            FontId::proportional(11.),
            Color32::from_rgb(103, 120, 132),
        );
    }
}
