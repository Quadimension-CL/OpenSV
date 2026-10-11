use crate::model::TimeAxis;
use std::time::Instant;

pub struct Transport {
    pub position: i64,
    pub playing: bool,
    pub looping: bool,
    pub return_on_pause: bool,
    start_position: i64,
    last: Option<Instant>,
}
impl Default for Transport {
    fn default() -> Self {
        Self {
            position: 0,
            playing: false,
            looping: false,
            return_on_pause: true,
            start_position: 0,
            last: None,
        }
    }
}
impl Transport {
    pub fn toggle(&mut self) {
        if self.playing {
            self.playing = false;
            if self.return_on_pause {
                self.position = self.start_position;
            }
        } else {
            self.start_position = self.position;
            self.playing = true;
        }
        self.last = Some(Instant::now());
    }
    pub fn stop(&mut self) {
        self.playing = false;
        self.position = 0;
        self.start_position = 0;
        self.last = None;
    }
    /// Returns true when a loop needs an explicit device seek.
    pub fn update(
        &mut self,
        time: &TimeAxis,
        end: i64,
        selection: Option<(i64, i64)>,
        audio_seconds: Option<f64>,
    ) -> bool {
        let now = Instant::now();
        let mut looped = false;
        if self.playing
            && let Some(last) = self.last
        {
            self.position = time.position_at(audio_seconds.unwrap_or_else(|| {
                time.seconds_at(self.position) + now.duration_since(last).as_secs_f64()
            }));
            let (start, end) = if self.looping {
                selection.unwrap_or((0, end))
            } else {
                (0, end)
            };
            if self.position >= end {
                if self.looping && end > start {
                    self.position = start;
                    looped = true;
                } else {
                    self.position = end;
                    self.playing = false;
                }
            }
        }
        self.last = Some(now);
        looped
    }
}
