use crate::model::QUARTER;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LoopRegion {
    start: i64,
    end: i64,
}

impl Default for LoopRegion {
    fn default() -> Self {
        Self {
            start: 0,
            end: QUARTER,
        }
    }
}

impl LoopRegion {
    pub fn from_saved(start:i64,end:i64)->Option<Self> {
        (start>=0 && end>start).then_some(Self{start,end})
    }
    pub fn start(self) -> i64 {
        self.start
    }

    pub fn end(self) -> i64 {
        self.end
    }

    pub fn contains(self, position: i64) -> bool {
        (self.start..self.end).contains(&position)
    }

    pub fn set_start(&mut self, position: i64) {
        self.start = position.clamp(0, self.end - QUARTER);
    }

    pub fn set_end(&mut self, position: i64) {
        self.end = position.max(self.start + QUARTER);
    }

    pub fn move_to(&mut self, position: i64) {
        let length = self.end - self.start;
        self.start = position.clamp(0, i64::MAX - length);
        self.end = self.start + length;
    }

    pub fn place_start(&mut self, position: i64) {
        self.start = position.clamp(0, i64::MAX - QUARTER);
        self.end = self.end.max(self.start + QUARTER);
    }

    pub fn place_end(&mut self, position: i64) {
        self.end = position.max(QUARTER);
        self.start = self.start.min(self.end - QUARTER);
    }
}
