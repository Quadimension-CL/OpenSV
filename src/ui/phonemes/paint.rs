use super::{Handle, geometry::Mapping};
use crate::{
    app::App,
    synthesis::rendered::RenderedPhoneme,
    ui::{theme::*, timeline::grid, track_color::TrackColors},
};
use eframe::egui::*;

const PHONE_BACKGROUND: Color32 = Color32::from_rgb(58, 57, 62);
const PHONE_FILL: Color32 = Color32::from_rgb(71, 70, 76);
const PHONE_LINE: Color32 = Color32::from_rgb(91, 89, 97);
const PHONE_BOUNDARY: Color32 = Color32::from_rgb(77, 75, 82);

pub(super) fn show(
    app: &App,
    ui: &Ui,
    mapping: &Mapping,
    phones: &[RenderedPhoneme],
    hovered: Option<(usize, Handle)>,
) {
    let clip = mapping.clip().intersect(ui.clip_rect());
    if !clip.is_positive() {
        return;
    }
    let colors = TrackColors::new(app.editor.current_track());
    let painter = ui.painter().with_clip_rect(clip);
    for phone in phones {
        let Some(rect) = visible_phone(mapping, phone, clip) else {
            continue;
        };
        painter.rect_filled(
            Rect::from_min_max(pos2(rect.left(), mapping.canvas.top()), rect.max),
            0,
            PHONE_BACKGROUND,
        );
    }
    grid(
        ui,
        mapping.canvas,
        &mapping.viewport,
        &app.editor.project.time,
        app.view.step(),
    );
    for (index, phone) in phones.iter().enumerate() {
        let Some(rect) = visible_phone(mapping, phone, clip) else {
            continue;
        };
        let handle = hovered
            .filter(|(phone_index, _)| *phone_index == index)
            .map(|(_, handle)| handle);
        let body_hovered = matches!(handle, Some(Handle::Strength | Handle::Impulse));
        if phone.supports_impulse {
            impulse(&painter, mapping, phone, rect, body_hovered, colors.accent);
        } else {
            strength(&painter, mapping, phone, rect, body_hovered, colors.accent);
        }
        boundary(&painter, rect.left(), mapping.canvas, false, colors.accent);
        boundary(&painter, rect.right(), mapping.canvas, false, colors.accent);
        token(&painter, mapping, phone, rect);
    }
    if let Some((index, handle)) = hovered
        && let Some(phone) = phones.get(index)
        && let Some(rect) = visible_phone(mapping, phone, clip)
    {
        match handle {
            Handle::Left => boundary(&painter, rect.left(), mapping.canvas, true, colors.accent),
            Handle::Right => boundary(&painter, rect.right(), mapping.canvas, true, colors.accent),
            Handle::Strength | Handle::Impulse => {}
        }
    }
    painter.hline(
        mapping.canvas.x_range(),
        mapping.content().top(),
        Stroke::new(1.0, PHONE_BOUNDARY),
    );
}

fn visible_phone(mapping: &Mapping, phone: &RenderedPhoneme, clip: Rect) -> Option<Rect> {
    if !phone.begin_seconds.is_finite()
        || !phone.end_seconds.is_finite()
        || phone.end_seconds <= phone.begin_seconds
    {
        return None;
    }
    let rect = mapping.phone_rect(phone);
    (rect.intersects(clip) && rect.is_positive()).then_some(rect)
}

fn token(painter: &Painter, mapping: &Mapping, phone: &RenderedPhoneme, rect: Rect) {
    let label = Rect::from_x_y_ranges(rect.x_range(), mapping.canvas.top()..=rect.top());
    let painter = painter.with_clip_rect(label.intersect(painter.clip_rect()));
    painter.text(
        label.center(),
        Align2::CENTER_CENTER,
        &phone.token,
        FontId::proportional(12.0),
        TEXT,
    );
}

fn boundary(painter: &Painter, x: f32, rect: Rect, hovered: bool, color: Color32) {
    painter.vline(
        x,
        rect.y_range(),
        Stroke::new(
            if hovered { 1.5 } else { 1.0 },
            if hovered { color } else { PHONE_BOUNDARY },
        ),
    );
}

fn impulse(
    painter: &Painter,
    mapping: &Mapping,
    phone: &RenderedPhoneme,
    rect: Rect,
    hovered: bool,
    color: Color32,
) {
    let Some(tip) = mapping.impulse(phone) else {
        return;
    };
    let stroke = Stroke::new(1.0, if hovered { color } else { PHONE_LINE });
    let width = rect.right() - tip.x;
    if width <= f32::EPSILON {
        painter.line_segment([pos2(tip.x, rect.bottom()), tip], stroke);
        return;
    }
    let left = tip.x.max(painter.clip_rect().left());
    let right = rect.right().min(painter.clip_rect().right());
    if right <= left {
        return;
    }
    let fill = if hovered {
        PHONE_FILL.lerp_to_gamma(color, 0.35)
    } else {
        PHONE_FILL
    };
    let height = rect.bottom() - tip.y;
    let samples = (right - left).ceil().clamp(1.0, 4096.0) as usize;
    let mut mesh = Mesh::default();
    let mut outline = Vec::with_capacity(samples + 3);
    outline.push(pos2(left, rect.bottom()));
    for sample in 0..=samples {
        let x = left + (right - left) * sample as f32 / samples as f32;
        let remaining = (rect.right() - x) / width;
        let point = pos2(x, rect.bottom() - height * remaining * remaining);
        outline.push(point);
        mesh.colored_vertex(point, fill);
        mesh.colored_vertex(pos2(x, rect.bottom()), fill);
        if sample > 0 {
            let index = (sample * 2) as u32;
            mesh.add_triangle(index - 2, index - 1, index);
            mesh.add_triangle(index - 1, index + 1, index);
        }
    }
    outline.push(pos2(right, rect.bottom()));
    painter.add(Shape::mesh(mesh));
    painter.add(Shape::line(outline, stroke));
}

fn strength(
    painter: &Painter,
    mapping: &Mapping,
    phone: &RenderedPhoneme,
    rect: Rect,
    hovered: bool,
    color: Color32,
) {
    let Some(value) = phone.strength.filter(|value| value.is_finite()) else {
        return;
    };
    let y = mapping.strength_y(value);
    painter.rect_filled(
        Rect::from_min_max(pos2(rect.left(), y), rect.max),
        0,
        if hovered {
            PHONE_FILL.lerp_to_gamma(color, 0.35)
        } else {
            PHONE_FILL
        },
    );
    painter.hline(
        rect.x_range(),
        y,
        Stroke::new(1.0, if hovered { color } else { PHONE_LINE }),
    );
}
