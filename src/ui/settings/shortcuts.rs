use super::widgets::section;
use crate::ui::theme::*;
use eframe::egui::*;

pub(super) fn show(ui: &mut Ui) {
    ui.weak("当前快捷键；自定义绑定尚未接入。");
    for (title, entries) in [
        (
            "工程",
            &[
                ("新建", "Ctrl+N"),
                ("打开", "Ctrl+O"),
                ("保存", "Ctrl+S"),
                ("另存为", "Ctrl+Shift+S"),
            ][..],
        ),
        (
            "编辑",
            &[
                ("撤销", "Ctrl+Z"),
                ("重做", "Ctrl+Y / Ctrl+Shift+Z"),
                ("全选音符", "Ctrl+A"),
                ("复制 / 剪切 / 粘贴", "Ctrl+C / X / V"),
                ("重复音符", "Ctrl+D"),
                ("删除音符", "Delete"),
                ("取消选择", "Esc"),
                ("填入歌词", "Ctrl+L"),
                ("静音音符", "Ctrl+M"),
                ("重录新的采样", "Alt+T"),
                ("裁剪保留使用采样", "Shift+Alt+T"),
                ("升高 / 降低八度", "Ctrl+↑ / ↓"),
                ("音符面板", "Ctrl+B"),
            ][..],
        ),
        (
            "走带控制",
            &[
                ("播放 / 暂停", "Space"),
                ("停止", "Ctrl+Space"),
                ("跳到结尾", "Ctrl+End"),
                ("按网格移动走带", "← / →"),
            ][..],
        ),
        (
            "工具与视野",
            &[
                ("选择 / 铅笔工具", "Alt+1 / Alt+2"),
                ("创建音符 / 编辑歌词", "双击"),
                ("平移视野", "中键拖动"),
                ("水平滚动", "Shift+滚轮"),
                ("缩放", "Ctrl+滚轮"),
            ][..],
        ),
    ] {
        section(ui, title);
        for (label, key) in entries {
            ui.horizontal(|ui| {
                ui.set_min_height(28.0);
                ui.label(*label);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    Frame::new()
                        .fill(BUTTON)
                        .corner_radius(3)
                        .inner_margin(Margin::symmetric(8, 4))
                        .show(ui, |ui| {
                            ui.label(RichText::new(*key).color(TEXT));
                        });
                });
            });
        }
    }
}
