use super::{Result, VoiceError};

pub(super) fn identity(payload: &[u8], voice_name: &str, model_version: u8) -> Result<u64> {
    super::super::derive_local_identity(descriptor(payload, voice_name)?, voice_name, model_version)
}

pub(super) fn is_session(payload: &[u8]) -> bool {
    std::str::from_utf8(payload).ok().is_some_and(|text| {
        let lines: Vec<_> = text.trim_start_matches('\u{feff}').lines().take(7).collect();
        lines.get(6).is_some_and(|line| line.contains("K1=") || line.contains("K3="))
            || (lines.get(4).is_some_and(|s| uuid::Uuid::parse_str(s).is_ok())
                && lines.get(5).is_some_and(|s| uuid::Uuid::parse_str(s).is_ok()))
    })
}

pub(super) fn is_plaintext(payload: &[u8]) -> bool {
    std::str::from_utf8(payload).ok().is_some_and(|text| {
        let text = text.trim_end_matches('\0');
        !text.is_empty() && text.chars().all(|c| !c.is_control() || matches!(c, '\r' | '\n' | '\t'))
    })
}

pub(super) fn descriptor<'a>(payload: &'a [u8], voice_name: &str) -> Result<&'a [u8]> {
    let text = std::str::from_utf8(payload).map_err(|_| invalid_session())?;
    let mut lines = text.trim_start_matches('\u{feff}').trim_end_matches('\0').lines();

    for _ in 0..4 {
        if lines.next().is_none() {
            return Err(invalid_session());
        }
    }
    let mut descriptor = "";
    for _ in 0..2 {
        let line = lines.next().ok_or_else(invalid_session)?;
        if uuid::Uuid::parse_str(line).is_err() {
            return Err(invalid_session());
        }
        descriptor = line;
    }

    let mut found = false;
    let mut records = 0;
    for line in lines {
        let mut fields = [None; 9];
        for field in line.split(';') {
            let (key, value) = field.split_once('=').ok_or_else(invalid_session)?;
            let index = match key.as_bytes() {
                [b'K', digit @ b'1'..=b'9'] => usize::from(*digit - b'1'),
                _ => return Err(invalid_session()),
            };
            if value.is_empty() || fields[index].replace(value).is_some() {
                return Err(invalid_session());
            }
        }
        let values = fields.map(|field| field.unwrap_or_default());
        if values.iter().any(|value| value.is_empty())
            || uuid::Uuid::parse_str(values[0]).is_err()
            || uuid::Uuid::parse_str(values[1]).is_err()
        {
            return Err(invalid_session());
        }
        records += 1;
        if values[2] == voice_name && matches!(values[4], "Voice Databases" | "Voice Databases 2") {
            if found {
                return Err(VoiceError::new("试用会话包含重复的声库记录。"));
            }
            found = true;
        }
    }
    if records == 0 {
        return Err(invalid_session());
    }
    if !found {
        return Err(VoiceError::new(format!(
            "所选试用会话未列出声库 {voice_name}。请选择包含此声库的原始试用会话文件。"
        )));
    }
    Ok(descriptor.as_bytes())
}

fn invalid_session() -> VoiceError {
    VoiceError::new(
        "无法读取所选 session，文件格式无效。",
    )
}

#[cfg(test)]
mod tests {
    use super::descriptor;

    fn payload(voice: &str) -> String {
        format!(
            "\n\n\n\n00000000-0000-0000-0000-000000000001\n\
             00000000-0000-0000-0000-000000000002\n\
             K1=00000000-0000-0000-0000-000000000003;\
             K2=00000000-0000-0000-0000-000000000004;\
             K3={voice};K4=fixture;K5=Voice Databases 2;\
             K6=synthetic;K7=synthetic;K8=synthetic;K9=synthetic"
        )
    }

    #[test]
    fn valid_record() {
        assert_eq!(descriptor(payload("Fixture Voice").as_bytes(), "Fixture Voice").unwrap(),
            b"00000000-0000-0000-0000-000000000002");
    }

    #[test]
    fn evaluation_absent_voice_still_reports_missing_record() {
        let error = descriptor(payload("Fixture Voice").as_bytes(), "Other Voice")
            .unwrap_err()
            .to_string();
        assert!(error.contains("未列出声库"));
        assert!(!error.contains("已验证"));
    }

    #[test]
    fn evaluation_malformed_record_still_reports_invalid_session() {
        for data in [
            vec![0xff],
            b"not an evaluation header".to_vec(),
            payload("Fixture Voice").replace(";K9=synthetic", "").into_bytes(),
            payload("Fixture Voice").replace("K9=synthetic", "K8=synthetic").into_bytes(),
        ] {
            let error = descriptor(&data, "Fixture Voice").unwrap_err().to_string();
            assert!(error.contains("格式无效"));
            assert!(!error.contains("已验证"));
        }
    }

    #[test]
    fn evaluation_duplicate_voice_still_reports_duplicate_record() {
        let mut data = payload("Fixture Voice");
        let record = data.lines().last().unwrap().to_owned();
        data.push('\n');
        data.push_str(&record);
        let error = descriptor(data.as_bytes(), "Fixture Voice").unwrap_err().to_string();
        assert!(error.contains("重复的声库记录"));
        assert!(!error.contains("已验证"));
    }

    #[test]
    fn evaluation_fix_preserves_explicit_manual_zero() {
        assert_eq!(crate::synthesis::voice::session_identity::parse_identity("0").unwrap(), 0);
        assert_eq!(crate::synthesis::voice::session_identity::parse_identity("0x0").unwrap(), 0);
    }
}
