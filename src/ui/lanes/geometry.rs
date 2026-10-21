use crate::{
    model::*,
    ui::{group_geometry, state::Viewport, timeline},
};
use eframe::egui::*;

#[derive(Clone)]
pub(super) struct Mapping {
    pub canvas: Rect,
    pub reference: GroupRef,
    pub viewport: Viewport,
    pub target: AutomationTarget,
    pub zoom: f32,
}

impl Mapping {
    pub fn visible_range(&self) -> (f64, f64) {
        let (low, high) = self.target.range();
        let center = (low + high) * 0.5;
        let half = (high - low) * 0.5 / f64::from(self.zoom);
        (center - half, center + half)
    }

    pub fn point(&self, (time, value): (i64, f64)) -> Pos2 {
        let (low, high) = self.visible_range();
        pos2(
            timeline::x_at(
                self.canvas,
                &self.viewport,
                beats(time.saturating_add(self.reference.blick_offset)),
            ),
            self.canvas.bottom() - ((value - low) / (high - low)) as f32 * self.canvas.height(),
        )
    }

    pub fn value_at(&self, pos: Pos2) -> (i64, f64) {
        let (low, high) = self.visible_range();
        let (minimum, maximum) = self.target.range();
        let (begin, end) = self.bounds();
        let time = ticks(timeline::beat_at(self.canvas, &self.viewport, pos.x))
            .saturating_sub(self.reference.blick_offset)
            .clamp(begin, end);
        let value = (low
            + f64::from(self.canvas.bottom() - pos.y) / f64::from(self.canvas.height())
                * (high - low))
            .clamp(minimum, maximum) as f32 as f64;
        (time, value)
    }

    pub fn bounds(&self) -> (i64, i64) {
        (
            self.reference
                .blick_absolute_begin
                .saturating_sub(self.reference.blick_offset),
            self.reference
                .visible_end()
                .saturating_sub(self.reference.blick_offset)
                .saturating_sub(1),
        )
    }

    pub fn clip(&self) -> Rect {
        group_geometry::clip_rect(&self.reference, self.canvas, &self.viewport)
    }

    pub fn nearest(&self, points: &[(i64, f64)], pos: Pos2) -> Option<i64> {
        let (begin, end) = self.bounds();
        points
            .iter()
            .filter(|(time, _)| (begin..=end).contains(time))
            .map(|&point| (point.0, self.point(point).distance_sq(pos)))
            .filter(|(_, distance)| *distance <= 64.0)
            .min_by(|left, right| left.1.total_cmp(&right.1))
            .map(|(time, _)| time)
    }
}
