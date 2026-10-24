use super::*;
use blowfish::cipher::BlockCipherEncrypt;

struct Fixture(std::path::PathBuf);
impl Fixture {
    fn new(data: &[u8]) -> Self {
        let path = std::env::temp_dir().join(format!("rynthv-session-fixture-{}.bin",uuid::Uuid::new_v4()));
        std::fs::write(&path,data).unwrap(); Self(path)
    }
    fn read(&self,key:Option<u64>,generation:u8) -> Result<SessionCondition> {
        read_session_identity(&self.0,"Fixture Voice",generation,key)
    }
}
impl Drop for Fixture {fn drop(&mut self) {let _=std::fs::remove_file(&self.0);}}

fn payload() -> Vec<u8> {
    b"\n\n\n\n00000000-0000-0000-0000-000000000001\n00000000-0000-0000-0000-000000000002\nK1=00000000-0000-0000-0000-000000000003;K2=00000000-0000-0000-0000-000000000004;K3=Fixture Voice;K4=fixture;K5=Voice Databases 2;K6=201;K7=2;K8=0;K9=0".to_vec()
}
fn encrypted(input:&[u8], key:u64) -> Vec<u8> {
    let mut out=input.to_vec(); let padding=8-out.len()%8;
    out.extend(std::iter::repeat_n(padding as u8,padding));
    let cipher=BlowfishLE::new_from_slice(&key.to_le_bytes()).unwrap();
    for chunk in out.as_chunks_mut::<8>().0 {cipher.encrypt_block(chunk.into());}
    out
}

#[test]
fn manual_nonzero_uses_original_generation_derivation_not_key() {
    let bytes=payload();let key=uuid::Uuid::new_v4().as_u128() as u64;
    let file=Fixture::new(&encrypted(&bytes,key));
    assert!(file.read(None,2).unwrap_err().is_session_decryption_exhausted());
    for generation in [1,2] {
        let result=file.read(Some(key),generation).unwrap();
        let expected=super::super::derive_local_identity(evaluation::descriptor(&bytes,"Fixture Voice").unwrap(),"Fixture Voice",generation).unwrap();
        assert_eq!(result.identity,expected);
        assert_eq!(result.protection,SessionProtection::ManualKey);
        assert_ne!(result.identity,key);
    }
    assert!(file.read(Some(key^1),2).unwrap_err().is_session_decryption_exhausted());
}

#[test]
fn automatic_plaintext_zero_precede_manual_and_preserve_generation_derivation() {
    for bytes in [payload(),encrypted(&payload(),0)] {
        let file=Fixture::new(&bytes);
        for key in [None,Some(0),Some(u64::MAX)] {
            let result=file.read(key,2).unwrap();
            assert_eq!(result.identity,super::super::derive_local_identity(
                evaluation::descriptor(&payload(),"Fixture Voice").unwrap(),"Fixture Voice",2).unwrap());
            assert_ne!(result.protection,SessionProtection::ManualKey);
        }
    }
}

#[test]
fn io_length_schema_duplicate_absent_voice_never_request_a_key() {
    let missing=std::env::temp_dir().join(format!("rynthv-absent-fixture-{}",uuid::Uuid::new_v4()));
    assert!(!read_session_identity(&missing,"Fixture Voice",2,None).unwrap_err().is_session_decryption_exhausted());
    let original=String::from_utf8(payload()).unwrap();let record=original.lines().last().unwrap();
    let key=uuid::Uuid::new_v4().as_u128() as u64;
    for (text,message) in [
        ("not a session".to_owned(),"格式无效"),
        (original.replace(";K9=0",""),"格式无效"),
        (original.replace("K3=Fixture Voice","K3=Other Voice"),"未列出声库"),
        (format!("{original}\n{record}"),"重复的声库记录"),
    ] {
        for bytes in [text.as_bytes().to_vec(),encrypted(text.as_bytes(),0),encrypted(text.as_bytes(),key)] {
            let file=Fixture::new(&bytes);
            let error=file.read(Some(key),2).unwrap_err();
            assert!(!error.is_session_decryption_exhausted());
            assert!(error.to_string().contains(message));
        }
    }
    for bytes in [vec![],vec![0xff;3]] {
        assert!(!Fixture::new(&bytes).read(None,2).unwrap_err().is_session_decryption_exhausted());
    }
}

#[test]
fn actual_device_key_is_attempted_before_any_manual_retry() {
    let Ok(bytes) = smbios::session_key() else {return};
    let key = u64::from_le_bytes(bytes);
    if key == 0 {return;}
    let payload=payload();let file=Fixture::new(&encrypted(&payload,key));
    let condition=file.read(Some(key^1),2).unwrap();
    assert_eq!(condition.protection,SessionProtection::MachineKey);
    assert_eq!(condition.identity,super::super::derive_local_identity(
        evaluation::descriptor(&payload,"Fixture Voice").unwrap(),"Fixture Voice",2).unwrap());
}

#[test]
fn key_parser_is_exact_and_errors_do_not_echo_inputs() {
    use super::super::parse_session_key;
    assert_eq!(parse_session_key(&u64::MAX.to_string()).unwrap(),u64::MAX);
    let key=uuid::Uuid::new_v4().as_u128() as u64;
    assert_eq!(parse_session_key(&format!("0x{key:X}")).unwrap(),key);
    assert_eq!(parse_session_key("0").unwrap(),0);
    let overflow=u128::from(u64::MAX)+1;
    for bad in [overflow.to_string(),format!("0x{overflow:x}"),"-1".into(),"1.0".into(),"+1".into(),"0x".into(), "secret-test".into()] {
        let error = parse_session_key(&bad).unwrap_err().to_string();
        if bad.len()>10 {assert!(!error.contains(&bad));}
    }
    assert!(VoiceError::decryption_exhausted().with_context("fixture").is_session_decryption_exhausted());
    assert!(!VoiceError::new("DUR model error").is_session_decryption_exhausted());
}
