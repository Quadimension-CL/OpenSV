use std::{path::PathBuf,process::ExitCode};
use serde_json::{Value,json};
use rynthv::{model::Automation,synthesis::{plan::Planner,score::{self,LyricFrontend},voice::VoiceInventory}};
#[path="project_check/storage.rs"]
mod project_storage;

fn main()->ExitCode {
    match run(){Ok(value)=>{println!("{value}");ExitCode::SUCCESS},Err(error)=>{eprintln!("{error}");ExitCode::FAILURE}}
}
fn run()->Result<Value,Box<dyn std::error::Error>> {
    let args:Vec<_>=std::env::args_os().skip(1).collect();
    if args.len()==3 && args[0]=="--bindings" {
        return project_storage::bindings(&PathBuf::from(&args[1]),&PathBuf::from(&args[2]));
    }
    if (3..=4).contains(&args.len()) && args[0]=="--import" {
        let target=PathBuf::from(&args[2]);if target.exists(){return Err("Import output already exists".into())}
        let mut imported=rynthv::project_import::convert(&PathBuf::from(&args[1]))?;
        if !imported.assets.is_empty(){
            let directory=args.get(3).ok_or("Embedded audio requires an explicit extraction directory")?;
            imported.extract(&PathBuf::from(directory))?;
        }
        rynthv::storage::save(&target,&imported.project)?;
        return Ok(json!({"imported":args[1].to_string_lossy(),"output":target,"tracks":project_storage::tracks(&imported.project),
            "notes":imported.project.tracks.iter().map(|t|t.main_group.notes.len()).sum::<usize>()+imported.project.library.iter().map(|g|g.notes.len()).sum::<usize>()}));
    }
    if args.len()==3 && args[0]=="--roundtrip" {
        return project_storage::roundtrip(&PathBuf::from(&args[1]),&PathBuf::from(&args[2]));
    }
    if args.len()>=2 && args[0]=="--spectra" {
        use std::io::{BufRead,BufReader};
        let mut comparisons=Vec::new();
        for path in &args[1..] {
            for line in BufReader::new(std::fs::File::open(path)?).lines() {
                let line=line?;if !line.contains("vocoder_spectral_probe") {continue;}
                let row:Value=serde_json::from_str(&line)?;
                let primary=row["function"]=="2b9e50";
                let mut actual=row["input"].as_array().ok_or("Missing spectral input")?.iter()
                    .map(|v|v.as_f64().map(|v|v as f32).ok_or("Invalid spectrum")).collect::<Result<Vec<_>,_>>()?;
                rynthv::synthesis::stages::vocoder::effects::response(&mut actual,
                    row["f0"].as_f64().ok_or("Missing f0")? as f32,
                    if primary {row["gender"].as_f64()}else{row["cached"][3].as_f64()}.ok_or("Missing gender")? as f32,
                    if primary {Some([row["shape"].as_f64().ok_or("shape")? as f32,row["changed_shape"].as_f64().ok_or("changed shape")? as f32])}else{None},
                    if primary {None}else{Some([row["noise_ratio"].as_f64().ok_or("ratio")? as f32,row["gain"].as_f64().ok_or("gain")? as f32])})?;
                let expected=row["output"].as_array().ok_or("Missing spectral output")?;
                if actual.len()!=expected.len() {return Err("Spectrum length differs".into());}
                let mut max=0.0_f32;let mut exact=0;
                for (a,b) in actual.iter().zip(expected) {
                    let b=b.as_f64().ok_or("Invalid spectrum output")? as f32;
                    max=max.max((a-b).abs());exact+=usize::from(a.to_bits()==b.to_bits());
                }
                comparisons.push(json!({"path":path.to_string_lossy(),"branch":row["flags"],"frame":row["relative"],"count":actual.len(),"bit_exact":exact,"max_abs":max}));
            }
        }
        return Ok(json!({"spectral_arithmetic_comparisons":comparisons}));
    }
    if args.len()==2 && args[0]=="--curves" {
        let native:Value=serde_json::from_slice(&std::fs::read(&args[1])?)?;
        let mut results=Vec::new();
        for group in native["groups"].as_array().ok_or("Missing native groups")? {
            for source in group["curves"].as_array().ok_or("Missing native curves")? {
                if source.get("error").is_some(){continue;}
                let points=source["points"].as_array().ok_or("Missing points")?.iter()
                    .flat_map(|p|p.as_array().into_iter().flatten().cloned()).collect::<Vec<_>>();
                let curve:Automation=serde_json::from_value(json!({"mode":source["mode"],"points":points}))?;
                let default=source["definition"]["defaultValue"].as_f64().ok_or("Missing default")? as f32;
                let mut max_abs=0.0_f32;let mut exact=0;let mut count=0;
                for pair in source["values"].as_array().ok_or("Missing native samples")? {
                    let time=pair[0].as_i64().ok_or("Invalid sample time")?;
                    let expected=pair[1].as_f64().ok_or("Invalid sample value")? as f32;
                    let actual=curve.sample(time,default);
                    max_abs=max_abs.max((actual-expected).abs());exact+=usize::from(actual.to_bits()==expected.to_bits());count+=1;
                }
                results.push(json!({"name":source["name"],"points":curve.points.len(),"samples":count,"bit_exact":exact,"max_abs":max_abs}));
            }
        }
        return Ok(json!({"native_curve_comparison":results}));
    }
    if args.len()<4 || args[0]!="--project" {
        return Err("Usage: rynthv-project-check --project VOICE CLF_DATA PROJECT... | --curves NATIVE_API.json".into());
    }
    let inventory=VoiceInventory::load(&PathBuf::from(&args[1]))?;
    let voice=inventory.prepare()?;
    let config=inventory.configuration.as_ref().ok_or("Missing voice metadata")?;
    let lexicon=LyricFrontend::new(&PathBuf::from(&args[2]),&config.default_language,&config.supported_languages)?;
    let mut results=Vec::new();
    for path in &args[3..] {
        let project=rynthv::storage::load(&PathBuf::from(path))?;
        let mut planner=Planner::default();
        let plan=planner.update(&project,0,|_|Some(inventory.identity().to_owned()));
        let mut owners=Vec::new();
        for request in plan.requests.values() {
            let phrase=score::prepare_owner(&request.input,&lexicon,voice.model("duration")?,voice.family)?;

            rynthv::synthesis::vocal_modes::validate_for_voice(&voice,phrase.vocal_modes.as_ref())?;
            owners.push(json!({"notes":request.input.notes.len(),"records":phrase.records.len(),
                "owner":request.owner,"note_errors":phrase.note_errors,
                "phones":phrase.records.iter().map(|r|r.phones.len()).sum::<usize>(),
                "automation_keys":request.input.automation.iter().filter(|(_,c)|!c.points.is_empty()).map(|(key,_)|key).collect::<Vec<_>>()}));
        }
        results.push(json!({"project":path.to_string_lossy(),"version":project.version,"owners":owners,
            "tracks":project_storage::tracks(&project)}));
    }
    Ok(json!({"prepared_projects":results,"neural_execution":false,"project_files_unchanged":true}))
}
