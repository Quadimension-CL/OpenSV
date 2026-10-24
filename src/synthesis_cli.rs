#[path = "synthesis_cli/args.rs"]
mod args;
#[path = "synthesis_cli/context.rs"]
mod context;
#[path = "synthesis_cli/render.rs"]
mod render;

use args::{Args, Command};
use rynthv::{
    storage,
    synthesis::{plan::Planner, voice::VoiceInventory},
};
use serde_json::json;
use std::{
    fs,
    io::{self, Write},
    process::ExitCode,
    time::Instant,
};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let Some(args) = Args::parse()? else {
        print!("{}", args::HELP);
        return Ok(());
    };
    let start = Instant::now();
    let inventory = VoiceInventory::load(&args.voice).map_err(|e| e.to_string())?;
    let load_ms = start.elapsed().as_secs_f64() * 1000.0;
    if let Some(import) = args.context {
        return write_report(args.output, &import.run(&inventory)?);
    }
    let mut preparation_ms = None;
    let preparation = if matches!(
        args.command,
        Command::Prepare | Command::RunModule | Command::ResolvePhone | Command::RenderScore
    ) {
        let start = Instant::now();
        let model = inventory.prepare().map_err(|e| e.to_string())?;
        preparation_ms = Some(start.elapsed().as_secs_f64() * 1000.0);
        Some(model)
    } else {
        None
    };
    let plan = if let Some(path) = args.project {
        let project = storage::load(&path)?;
        let mut planner = Planner::default();
        planner.update(&project, 0, |_| Some(inventory.identity().to_owned()));
        Some(planner.summary())
    } else {
        None
    };
    let execution = if let Some(module) = args.module {
        let metadata = fs::metadata(&module.input).map_err(|e| e.to_string())?;
        if metadata.len() > 16 * 1024 * 1024 {
            return Err("Module input JSON exceeds 16 MiB".into());
        }
        let input: Vec<f32> = serde_json::from_slice(
            &fs::read(&module.input).map_err(|e| e.to_string())?,
        )
        .map_err(|e| format!("Input must be a flat JSON array of finite f32 values: {e}"))?;
        if input.is_empty() || input.iter().any(|value| !value.is_finite()) {
            return Err("Module input must be nonempty and finite".into());
        }
        let start = Instant::now();
        let output = preparation
            .as_ref()
            .expect("RunModule prepares a voice")
            .execute_module(module.file, module.tree, module.node, &input)
            .map_err(|e| e.to_string())?;
        if output.iter().any(|value| !value.is_finite()) {
            return Err(
                "Module returned nonfinite values; refusing JSON's lossy null conversion".into(),
            );
        }
        Some(json!({
            "file_index": module.file, "tree_index": module.tree, "node_index": module.node,
            "input_elements": input.len(), "output_elements": output.len(),
            "execution_ms": start.elapsed().as_secs_f64() * 1000.0,
            "values": output,
            "scope": "individual recovered CPU module; intermediate features, not audio",
            "original_engine_bit_exact_verified": false,
        }))
    } else {
        None
    };
    let phone_lookup = if let Some(phone) = args.phone {
        let model = preparation.as_ref().expect("ResolvePhone prepares a voice");
        let result = model
            .tree(phone.file, phone.tree)
            .and_then(|tree| tree.resolve_phone(&phone.language, &phone.token))
            .map_err(|e| e.to_string())?;
        Some(json!({ "language": phone.language, "token": phone.token, "resolved": result }))
    } else {
        None
    };
    let audio = if let Some(render_args) = &args.render {
        Some(render::run(
            render_args,
            &inventory,
            preparation.as_ref().expect("RenderScore prepares a voice"),
        )?)
    } else {
        None
    };
    let report = json!({
        "schema": 2,
        "voice_load_ms": load_ms,
        "preparation_ms": preparation_ms,
        "voice": inventory,
        "preparation": preparation.as_ref().map(|model| model.summary()),
        "plan": plan,
        "execution": execution,
        "phone_lookup": phone_lookup,
        "audio_generated": audio.is_some(),
        "audio": audio,
    });
    write_report(args.output, &report)
}

fn write_report(
    output: Option<std::path::PathBuf>,
    report: &serde_json::Value,
) -> Result<(), String> {
    let mut bytes = serde_json::to_vec_pretty(report).map_err(|e| e.to_string())?;
    bytes.push(b'\n');
    if let Some(path) = output {
        storage::atomic_write(&path, &bytes)?;
    } else {
        io::stdout()
            .lock()
            .write_all(&bytes)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
