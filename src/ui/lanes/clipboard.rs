use serde::{Deserialize, Serialize};

const CLIPBOARD_KIND: &str = "RynthV parameter points";
const MAX_CLIPBOARD_BYTES: usize = 4 * 1024 * 1024;
const MAX_CLIPBOARD_POINTS: usize = 100_000;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ParameterClipboard {
    kind: String,
    pub(super) range: (f64, f64),
    pub(super) points: Vec<(i64, f64)>,
}

impl ParameterClipboard {
    pub(super) fn capture(
        mut points: Vec<(i64, f64)>,
        range: (f64, f64),
    ) -> Result<Option<Self>, String> {
        let Some(&(first, _)) = points.first() else {
            return Ok(None);
        };
        for point in &mut points {
            point.0 = point
                .0
                .checked_sub(first)
                .ok_or("所选参数控制点的时间跨度超出可用范围。")?;
        }
        let clipboard = Self {
            kind: CLIPBOARD_KIND.into(),
            range,
            points,
        };
        clipboard.validate()?;
        Ok(Some(clipboard))
    }

    fn validate(&self) -> Result<(), String> {
        if self.kind != CLIPBOARD_KIND {
            return Err("剪贴板中没有 RynthV 参数控制点，请先复制参数控制点。".into());
        }
        let (low, high) = self.range;
        if !low.is_finite() || !high.is_finite() || low >= high || !(high - low).is_finite() {
            return Err("参数控制点剪贴板的取值范围无效。".into());
        }
        if self.points.is_empty() || self.points.len() > MAX_CLIPBOARD_POINTS {
            return Err(format!(
                "参数剪贴板须包含 1 到 {MAX_CLIPBOARD_POINTS} 个控制点。"
            ));
        }
        if self.points[0].0 != 0
            || self.points.iter().any(|&(time, value)| {
                time < 0 || !value.is_finite() || !(low..=high).contains(&value)
            })
            || self.points.windows(2).any(|pair| pair[0].0 >= pair[1].0)
        {
            return Err("参数控制点剪贴板的时间顺序或数值无效。".into());
        }
        Ok(())
    }

    pub(super) fn to_text(&self) -> Result<String, String> {
        let text =
            serde_json::to_string(self).map_err(|error| format!("无法复制参数控制点：{error}"))?;
        if text.len() > MAX_CLIPBOARD_BYTES {
            return Err("所选参数控制点数据过大，无法复制。".into());
        }
        Ok(text)
    }

    pub(super) fn from_text(text: &str) -> Result<Self, String> {
        if text.len() > MAX_CLIPBOARD_BYTES {
            return Err("剪贴板数据过大，无法粘贴参数控制点。".into());
        }
        let clipboard: Self = serde_json::from_str(text)
            .map_err(|error| format!("剪贴板中没有有效的 RynthV 参数控制点：{error}"))?;
        clipboard.validate()?;
        Ok(clipboard)
    }
}
