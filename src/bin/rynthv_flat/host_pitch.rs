use rynthv::synthesis::{engine::DdsPitchClock, voice::{Result, VoiceError}};
use serde::Deserialize;
use serde_json::json;
use std::io::{self, BufRead, Read, Write};

const MAX_REPLY: usize = 32 * 1024 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Reply {
    schema: u32,
    event: String,
    request: u32,
    origin_seconds: f64,
    frame_seconds: f64,
    count: usize,
    values: Vec<Option<f32>>,
}

fn bad(message: impl Into<String>) -> VoiceError {
    io::Error::new(io::ErrorKind::InvalidData, message.into()).into()
}

pub(super) fn exchange(clock: DdsPitchClock) -> Result<Vec<Option<f32>>> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    exchange_io(clock, &mut stdin.lock(), &mut stdout.lock())
}

fn exchange_io(clock: DdsPitchClock, input: &mut impl BufRead, output: &mut impl Write) -> Result<Vec<Option<f32>>> {
    serde_json::to_writer(&mut *output, &json!({
        "schema":1,"event":"host_pitch","request":1,
        "origin_seconds":clock.origin_seconds,"frame_seconds":clock.frame_seconds,"count":clock.count
    })).map_err(|error| bad(error.to_string()))?;
    output.write_all(b"\n")?;
    output.flush()?;
    let mut line = Vec::new();
    input.take(MAX_REPLY as u64 + 1).read_until(b'\n', &mut line)?;
    if line.len() > MAX_REPLY || line.last() != Some(&b'\n') {
        return Err(bad("Host pitch reply is missing, unterminated or oversized"));
    }
    let reply: Reply = serde_json::from_slice(&line).map_err(|error| bad(format!("Invalid host pitch reply: {error}")))?;
    if reply.schema != 1 || reply.event != "host_pitch" || reply.request != 1
        || reply.origin_seconds.to_bits() != clock.origin_seconds.to_bits()
        || reply.frame_seconds.to_bits() != clock.frame_seconds.to_bits()
        || reply.count != clock.count || reply.values.len() != clock.count
        || reply.values.iter().flatten().any(|value| !value.is_finite())
    {
        return Err(bad("Host pitch reply identity, clock, count or values differ"));
    }
    Ok(reply.values)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn clock() -> DdsPitchClock { DdsPitchClock { origin_seconds: -0.5, frame_seconds: 0.005000000353902578, count: 2 } }
    fn reply() -> serde_json::Value { let c=clock(); json!({"schema":1,"event":"host_pitch","request":1,
        "origin_seconds":c.origin_seconds,"frame_seconds":c.frame_seconds,"count":c.count,"values":[null,60.25]}) }
    fn run(value: serde_json::Value) -> Result<Vec<Option<f32>>> {
        let mut line=serde_json::to_vec(&value).unwrap();line.push(b'\n');
        exchange_io(clock(),&mut io::Cursor::new(line),&mut Vec::new())
    }
    #[test]
    fn accepts_finite_and_unavailable_without_interpolation() {
        assert_eq!(run(reply()).unwrap(),vec![None,Some(60.25)]);
    }
    #[test]
    fn rejects_wrong_clock_order_count_value_and_missing_response() {
        for key in ["origin_seconds","frame_seconds","request","count"] {
            let mut value=reply();value[key]=json!(3);assert!(run(value).is_err());
        }
        let mut value=reply();value["values"]=json!([null]);assert!(run(value).is_err());
        let mut value=reply();value["values"]=json!([null,"NaN"]);assert!(run(value).is_err());
        assert!(exchange_io(clock(),&mut io::Cursor::new(Vec::<u8>::new()),&mut Vec::new()).is_err());
    }
}
