use serde::{Deserialize, Serialize};

/// SVP stores track colors as eight hexadecimal digits in ARGB order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct TrackColor(u32);

impl TrackColor {
    pub const PALETTE: [(&'static str, Self); 12] = [
        ("红色", Self::from_rgb(207, 77, 91)),
        ("橙色", Self::from_rgb(191, 117, 85)),
        ("赭黄", Self::from_rgb(190, 160, 77)),
        ("黄色", Self::from_rgb(218, 185, 58)),
        ("绿色", Self::from_rgb(125, 178, 53)),
        ("翠绿", Self::from_rgb(47, 177, 106)),
        ("青色", Self::from_rgb(49, 174, 166)),
        ("天蓝", Self::from_rgb(62, 145, 192)),
        ("蓝色", Self::from_rgb(78, 115, 195)),
        ("靛蓝", Self::from_rgb(103, 81, 204)),
        ("紫色", Self::from_rgb(149, 77, 202)),
        ("品红", Self::from_rgb(173, 75, 175)),
    ];

    pub const fn from_rgb(r: u8, g: u8, b: u8) -> Self {
        Self(u32::from_be_bytes([255, r, g, b]))
    }

    pub const fn rgba(self) -> [u8; 4] {
        let [a, r, g, b] = self.0.to_be_bytes();
        [r, g, b, a]
    }
}

impl Default for TrackColor {
    fn default() -> Self {
        Self::PALETTE[4].1
    }
}

impl super::Project {
    pub fn next_track_color(&self) -> TrackColor {
        // Alternate hue families first; after exhausting unused colors, reuse
        // the least common one. Vocal and instrumental tracks share the palette.
        [4, 7, 0, 3, 9, 5, 11, 1, 6, 10, 2, 8]
            .into_iter()
            .map(|index| TrackColor::PALETTE[index].1)
            .min_by_key(|color| {
                self.tracks
                    .iter()
                    .filter(|t| t.disp_color == *color)
                    .count()
            })
            .expect("nonempty track palette")
    }
}

impl TryFrom<String> for TrackColor {
    type Error = &'static str;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.len() != 8 || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err("音轨颜色必须是八位 ARGB 十六进制数");
        }
        u32::from_str_radix(&value, 16)
            .map(Self)
            .map_err(|_| "音轨颜色无效")
    }
}

impl From<TrackColor> for String {
    fn from(color: TrackColor) -> Self {
        format!("{:08x}", color.0)
    }
}
