use std::path::Path;

use crate::ui::{icons::Icon, theme::*};
use eframe::egui::*;

/// Artwork is cached by egui's file and image loaders.
pub(super) fn paint(ui: &Ui, rect: Rect, path: Option<&Path>) {
    if !ui.is_rect_visible(rect) {
        return;
    }
    ui.painter()
        .rect_filled(rect, 4, Color32::from_rgb(53, 57, 55));
    if let Some(path) = path {
        Image::from_uri(file_uri(path))
            .fit_to_exact_size(rect.size())
            .corner_radius(4)
            .paint_at(ui, rect);
    } else {
        Icon::Microphone.paint(ui, rect.shrink(rect.width() * 0.26), MUTED);
    }
}

fn file_uri(path: &Path) -> String {
    #[cfg(windows)]
    {
        let path = path.to_string_lossy().replace('\\', "/");
        if path.starts_with("//") {
            format!("file:{path}")
        } else {
            format!("file:///{path}")
        }
    }
    #[cfg(not(windows))]
    {
        // Unix absolute paths already supply the third slash. Backslashes are
        // valid filename characters, and egui's file loader consumes raw paths.
        format!("file://{}", path.to_string_lossy())
    }
}
