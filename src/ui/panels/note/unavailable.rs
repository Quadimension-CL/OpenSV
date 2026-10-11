use crate::ui::{icons::Icon, theme::*};
use eframe::egui::*;

const REASON: &str = "此功能尚未接入合成";

pub(super) fn mode(ui: &mut Ui) {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::hover());
    ui.painter().rect_filled(rect, 3, BACKGROUND);
    let active = Rect::from_min_max(
        rect.min + vec2(3.0, 3.0),
        pos2(rect.center().x, rect.bottom() - 3.0),
    );
    ui.painter().rect_filled(active, 3, BUTTON);
    ui.painter().text(
        active.center(),
        Align2::CENTER_CENTER,
        "歌唱",
        FontId::proportional(12.0),
        TEXT,
    );
    ui.painter().text(
        pos2(rect.left() + rect.width() * 0.75, rect.center().y),
        Align2::CENTER_CENTER,
        "说唱",
        FontId::proportional(12.0),
        MUTED,
    );
    response.on_hover_text("当前支持歌唱；说唱尚未接入合成");
}

pub(super) fn expression(ui: &mut Ui) {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::splat(ui.available_width()), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 4, Color32::from_rgb(31, 30, 34));
    painter.hline(rect.x_range(), rect.center().y, Stroke::new(1.0, BUTTON));
    painter.vline(rect.center().x, rect.y_range(), Stroke::new(1.0, BUTTON));
    for (position, anchor, text) in [
        (rect.min + vec2(8.0, 8.0), Align2::LEFT_TOP, "鲜活"),
        (
            rect.right_top() + vec2(-8.0, 8.0),
            Align2::RIGHT_TOP,
            "精修",
        ),
        (
            rect.left_bottom() + vec2(8.0, -8.0),
            Align2::LEFT_BOTTOM,
            "机械",
        ),
        (rect.max - vec2(8.0, 8.0), Align2::RIGHT_BOTTOM, "原生"),
    ] {
        painter.text(position, anchor, text, FontId::proportional(12.0), MUTED);
    }
    painter.circle_stroke(rect.center(), 8.0, Stroke::new(2.0, TEXT));
    response
        .on_hover_cursor(CursorIcon::NotAllowed)
        .on_hover_text(REASON);
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 48.0), Sense::hover());
    ui.painter().text(
        rect.min + vec2(0.0, 4.0),
        Align2::LEFT_TOP,
        "颤音调制",
        FontId::proportional(12.0),
        MUTED,
    );
    let center = pos2(rect.center().x, rect.top() + 31.0);
    ui.painter()
        .hline(rect.x_range(), center.y, Stroke::new(2.0, EDGE));
    ui.painter()
        .hline(rect.left()..=center.x, center.y, Stroke::new(2.0, MUTED));
    ui.painter()
        .circle(center, 9.0, BUTTON, Stroke::new(0.7, BUTTON_ACTIVE));
    ui.painter().circle_filled(center, 3.0, TEXT);
    response
        .on_hover_cursor(CursorIcon::NotAllowed)
        .on_hover_text(REASON);
}

pub(super) fn retakes(ui: &mut Ui) {
    let width = ((ui.available_width() - 4.0) * 0.5).max(1.0);
    ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
    for row in [
        [("全部", None), ("时值", Some(Icon::Phonemes))],
        [("音高", Some(Icon::Notes)), ("音色", None)],
    ] {
        ui.horizontal(|ui| {
            for (label, icon) in row {
                let (rect, response) = ui.allocate_exact_size(vec2(width, 88.0), Sense::hover());
                ui.painter().rect(
                    rect,
                    3,
                    BORDER,
                    Stroke::new(0.6, BUTTON),
                    StrokeKind::Inside,
                );
                let center = pos2(rect.center().x, rect.top() + 34.0);
                if let Some(icon) = icon {
                    icon.paint(ui, Rect::from_center_size(center, Vec2::splat(25.0)), MUTED);
                } else if label == "全部" {
                    sparkles(ui, center);
                } else {
                    lips(ui, center);
                }
                ui.painter().text(
                    pos2(center.x, rect.bottom() - 12.0),
                    Align2::CENTER_BOTTOM,
                    label,
                    FontId::proportional(12.0),
                    MUTED,
                );
                response
                    .on_hover_cursor(CursorIcon::NotAllowed)
                    .on_hover_text(REASON);
            }
        });
    }
    ui.add_space(8.0);
    ui.add_enabled_ui(false, |ui| {
        ui.horizontal(|ui| {
            ComboBox::from_id_salt("retake-choice")
                .width((ui.available_width() - 56.0).max(30.0))
                .selected_text("默认采样")
                .show_ui(ui, |_| {});
            ui.add_sized(vec2(22.0, 28.0), Button::new("‹"));
            ui.add_sized(vec2(22.0, 28.0), Button::new("›"));
        });
    })
    .response
    .on_disabled_hover_text(REASON);
}

fn lips(ui: &Ui, center: Pos2) {
    let contour = [
        (-11.0, -1.0),
        (-5.0, -6.0),
        (0.0, -4.0),
        (5.0, -6.0),
        (11.0, -1.0),
        (7.0, 5.0),
        (0.0, 8.0),
        (-7.0, 5.0),
        (-11.0, -1.0),
    ];
    ui.painter().add(Shape::line(
        contour
            .into_iter()
            .map(|(x, y)| center + vec2(x, y))
            .collect(),
        Stroke::new(1.3, MUTED),
    ));
    ui.painter().add(Shape::line(
        [
            (-10.0, -1.0),
            (-4.0, 1.0),
            (0.0, 0.0),
            (4.0, 1.0),
            (10.0, -1.0),
        ]
        .into_iter()
        .map(|(x, y)| center + vec2(x, y))
        .collect(),
        Stroke::new(1.2, MUTED),
    ));
}

fn sparkles(ui: &Ui, center: Pos2) {
    for (offset, radius) in [
        (vec2(-4.0, 0.0), 9.0),
        (vec2(8.0, -8.0), 3.5),
        (vec2(9.0, 8.0), 3.0),
    ] {
        let center = center + offset;
        let outline = [
            (0.0, -1.0),
            (0.3, -0.3),
            (1.0, 0.0),
            (0.3, 0.3),
            (0.0, 1.0),
            (-0.3, 0.3),
            (-1.0, 0.0),
            (-0.3, -0.3),
            (0.0, -1.0),
        ];
        ui.painter().add(Shape::line(
            outline
                .into_iter()
                .map(|(x, y)| center + vec2(x, y) * radius)
                .collect(),
            Stroke::new(1.3, MUTED),
        ));
    }
}
