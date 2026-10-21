use crate::{
    model::{GroupRef, beats, ticks},
    synthesis::{
        plan::{GlobalBlick, OwnerInput},
        rendered::RenderedPhoneme,
    },
    ui::{state::Viewport, timeline},
};
use eframe::egui::*;
use std::sync::Arc;

#[derive(Clone)]
pub(super) struct Mapping {
    pub canvas: Rect,
    pub input: Arc<OwnerInput>,
    pub viewport: Viewport,
}

impl Mapping {
    pub fn content(&self) -> Rect {
        Rect::from_min_max(
            pos2(self.canvas.left(), self.canvas.top() + 32.0),
            self.canvas.max,
        )
    }

    pub fn x(&self, seconds: f64) -> f32 {
        timeline::x_at(
            self.canvas,
            &self.viewport,
            beats(self.input.tempo.position_at(seconds).0),
        )
    }

    pub fn seconds(&self, x: f32) -> f64 {
        self.input
            .tempo
            .seconds_at(GlobalBlick(ticks(timeline::beat_at(
                self.canvas,
                &self.viewport,
                x,
            ))))
    }

    pub fn clip(&self) -> Rect {
        let left = timeline::x_at(
            self.canvas,
            &self.viewport,
            beats(self.input.visible_begin.0),
        );
        let right = self.input.visible_end.map_or(self.canvas.right(), |end| {
            timeline::x_at(self.canvas, &self.viewport, beats(end.0))
        });
        Rect::from_min_max(
            pos2(left, self.canvas.top()),
            pos2(right, self.canvas.bottom()),
        )
        .intersect(self.canvas)
    }

    pub fn phone_rect(&self, phone: &RenderedPhoneme) -> Rect {
        Rect::from_min_max(
            pos2(self.x(phone.begin_seconds), self.content().top()),
            pos2(self.x(phone.end_seconds), self.content().bottom()),
        )
    }

    pub fn strength_y(&self, strength: f64) -> f32 {
        let top = self.content().top();
        let bottom = self.content().bottom();
        bottom - ((strength + 1.0) * 0.5) as f32 * (bottom - top)
    }

    pub fn strength_at(&self, y: f32) -> f64 {
        let top = self.content().top();
        let bottom = self.content().bottom();
        ((f64::from(bottom - y) / f64::from((bottom - top).max(1.0))).clamp(0.0, 1.0) * 2.0 - 1.0)
            as f32 as f64
    }

    pub fn impulse(&self, phone: &RenderedPhoneme) -> Option<Pos2> {
        if !phone.supports_impulse {
            return None;
        }
        let (position, activity) = (phone.position?, phone.activity?);
        let rect = self.phone_rect(phone);
        Some(pos2(
            rect.left() + rect.width() * position as f32,
            rect.bottom() - rect.height() * (activity * activity) as f32,
        ))
    }

    pub fn same_reference(&self, reference: &GroupRef) -> bool {
        self.input.group_id == reference.group_id
            && self.input.content_offset.0 == reference.blick_offset
            && self.input.visible_begin.0 == reference.blick_absolute_begin
            && self.input.visible_end.map(|end| end.0)
                == (reference.blick_absolute_end >= 0).then_some(reference.blick_absolute_end)
    }
}
