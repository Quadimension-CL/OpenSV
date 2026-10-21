use super::*;

pub(super) fn add_point(app: &mut App, lane: &Lane, mapping: &Mapping, pos: Pos2) {
    if app.gesture.is_some() || app.editor.is_editing() {
        return;
    }
    app.commit_lyric();
    focus(app, lane);
    let point = mapping.value_at(pos);
    let mut points: BTreeMap<_, _> = app
        .editor
        .automation(&lane.foreground)
        .into_iter()
        .flat_map(|curve| curve.points.iter().copied())
        .collect();
    points.insert(point.0, point.1);
    app.editor.begin("添加参数控制点");
    match app
        .editor
        .replace_automation_points(&lane.foreground, points.into_iter().collect())
    {
        Ok(_) => {
            app.editor.finish();
            app.parameter_lane.selected = BTreeSet::from([point.0]);
            app.parameter_lane.anchor = Some(point.0);
        }
        Err(error) => {
            app.editor.cancel();
            app.error = Some(error);
        }
    }
}
