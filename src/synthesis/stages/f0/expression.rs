#[derive(Clone, Copy, Debug)]
pub struct ExpressionControls {
    pub expression: f32,
    pub position_c: f32,
    pub position_b: f32,
    pub accent: f32,
}

pub fn expression_controls(kind: u32, x: f64, y: f64) -> Option<ExpressionControls> {
    if kind != 0 {
        return None;
    }

    let points: [(f64, f64); 9] = [
        (0.0, 0.0),
        (1.0, 0.0),
        (0.8, 0.8),
        (0.0, 1.0),
        (-0.8, 0.8),
        (-1.0, 0.0),
        (-0.8, -0.8),
        (0.0, -1.0),
        (0.8, -0.8),
    ];
    let weights = points.map(|(point_x, point_y)| (-4.0 * (x - point_x).hypot(y - point_y)).exp());
    let total: f64 = weights.iter().sum();
    let [
        center,
        right,
        upper_right,
        up,
        upper_left,
        left,
        lower_left,
        down,
        lower_right,
    ] = weights;
    let center_half = center * 0.5;
    let upper_right_half = upper_right * 0.5;
    let expression = (((((((right * 0.5 + center_half + upper_right_half) + up * 0.5)
        + upper_left * 0.5)
        + left * 0.3)
        + lower_left * 0.2)
        + down * 0.3)
        + lower_right * 0.5)
        / total;
    let position_c = (right + center_half + upper_right + up) / total;
    let position_b =
        (center_half + upper_right_half + up + upper_left + left + lower_left * 0.5) / total;
    Some(ExpressionControls {
        expression: expression as f32,
        position_c: position_c as f32,
        position_b: position_b as f32,
        accent: (1.0 - 2.0 * (-1.0 - x).hypot(-1.0 - y)).max(0.0) as f32,
    })
}
