use std::{path::PathBuf,process::ExitCode};
fn main()->ExitCode {
    let Some(root)=std::env::args_os().nth(1).map(PathBuf::from) else {
        eprintln!("Usage: rynthv-catalog DATABASE_ROOT");return ExitCode::FAILURE;
    };
    match rynthv::synthesis::voice::VoiceCatalog::scan(&root)
        .and_then(|catalog|serde_json::to_string(&catalog).map_err(|e|rynthv::synthesis::voice::VoiceError::from(std::io::Error::other(e)))) {
        Ok(json)=>{println!("{json}");ExitCode::SUCCESS},
        Err(error)=>{eprintln!("{error}");ExitCode::FAILURE},
    }
}
