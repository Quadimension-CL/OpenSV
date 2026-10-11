use super::widgets::*;
use crate::{app::App, ui::state::Column};
use eframe::egui::*;

pub(super) fn general(app: &mut App, ui: &mut Ui) {
    fixed(ui, "界面语言", "简体中文", "当前界面仅支持简体中文");
    row(ui, "界面缩放", |ui| {
        let id = ui.make_persistent_id("zoom-draft");
        let mut percent = ui
            .ctx()
            .data_mut(|data| data.get_temp::<f32>(id))
            .unwrap_or(app.preferences.zoom * 100.0);
        ui.label(format!("{percent:.0}%"));
        ui.spacing_mut().slider_width = ui.available_width().max(40.0);
        let response = ui.add(
            Slider::new(&mut percent, 75.0..=200.0)
                .step_by(5.0)
                .show_value(false),
        );
        if response.drag_stopped() || response.changed() && !response.dragged() {
            app.preferences.zoom = percent / 100.0;
        }
        if response.dragged() {
            ui.ctx().data_mut(|data| data.insert_temp(id, percent));
        } else {
            ui.ctx().data_mut(|data| data.remove::<f32>(id));
        }
    });
    fixed(
        ui,
        "图形渲染",
        "Direct3D 12",
        "当前使用 GPU 渲染，暂不支持切换渲染后端",
    );
    section(ui, "帮助和指导");
    toggle(ui, "显示状态栏", &mut app.preferences.show_status_bar);
    toggle(ui, "显示控件操作提示", &mut app.preferences.control_hints);
    section(ui, "工作区");
    row(ui, "工具列宽度", |ui| {
        ui.spacing_mut().slider_width = (ui.available_width() - 55.0).max(40.0);
        ui.add(Slider::new(&mut app.view.panel_width, 140.0..=420.0).suffix(" px"));
    });
    row(ui, "布局", |ui| {
        if ui.button("重置工作区布局").clicked() {
            app.view = Default::default();
        }
    });
}

pub(super) fn audio(app: &mut App, ui: &mut Ui) {
    section(ui, "音频输出");
    fixed(
        ui,
        "输出设备",
        "系统默认输出设备",
        "当前跟随系统默认设备，设备切换尚未接入",
    );
    fixed(
        ui,
        "播放格式",
        "设备默认格式",
        "预览音频自动适配输出设备的采样率和声道",
    );
    if let Some(error) = app.synthesis.playback_error() {
        ui.colored_label(Color32::from_rgb(218, 142, 126), error);
    }
    section(ui, "MIDI 输入");
    fixed(ui, "输入设备", "未启用", "MIDI 输入尚未接入");
}

pub(super) fn editing(app: &mut App, ui: &mut Ui) {
    section(ui, "钢琴卷帘");
    row(ui, "半音行高", |ui| {
        ui.spacing_mut().slider_width = (ui.available_width() - 55.0).max(40.0);
        ui.add(Slider::new(&mut app.view.row_height, 12.0..=36.0).suffix(" px"));
    });
    toggle(ui, "音符对齐网格", &mut app.view.snap);
    row(ui, "网格精度", |ui| {
        ComboBox::from_id_salt("settings-grid")
            .width(ui.available_width())
            .selected_text(format!("{} 分音符", app.view.grid))
            .show_ui(ui, |ui| {
                for value in [4, 8, 16, 32, 64, 128] {
                    ui.selectable_value(&mut app.view.grid, value, format!("{value} 分音符"));
                }
            });
    });
    toggle(ui, "三连音网格", &mut app.view.triplet);
    section(ui, "显示");
    toggle(ui, "显示音高线", &mut app.show_pitch);
    toggle(ui, "显示波形", &mut app.show_waveform);
    toggle(ui, "编排位于上方", &mut app.view.arrangement_first);
    toggle(ui, "显示曲速和拍号轨", &mut app.show_time_tracks);
    ui.add_space(10.0);
    ui.weak("拖动标题可调整工具列顺序，拖动分隔线可调整区域尺寸。");
}

pub(super) fn recording(ui: &mut Ui) {
    fixed(ui, "录音输入设备", "未启用", "录音功能尚未接入");
    fixed(ui, "录音声道", "未启用", "录音功能尚未接入");
    section(ui, "录制");
    ui.weak("录制功能尚未接入，相关设置暂不可用。");
}

pub(super) fn rendering(app: &mut App, ui: &mut Ui) {
    crate::ui::voice_library::settings(app, ui);
    row(ui, "歌声数据库", |ui| {
        if ui.button("管理歌声…").clicked() {
            app.settings = false;
            app.voice_manager = true;
        }
    });
    section(ui, "音频导出");
    row(ui, "导出设置", |ui| {
        if ui.button("打开渲染面板").clicked() {
            if !app.view.columns.contains(&Column::Render) {
                app.view.toggle(Column::Render);
            }
            app.settings = false;
        }
    });
    ui.weak("目标文件夹、音轨、声道、位深度和采样率在渲染面板中设置。");
}
