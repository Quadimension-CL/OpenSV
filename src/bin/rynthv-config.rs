use rynthv::synthesis::voice::VoiceInventory;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path=std::env::args_os().nth(1).ok_or("Usage: rynthv-config VOICE")?;
    let inventory=VoiceInventory::load(std::path::Path::new(&path))?;
    let config=inventory.configuration.as_ref().ok_or("No voice configuration")?;
    println!("{}",serde_json::to_string(&serde_json::json!({"name":config.name,"version":config.version,"additional":config.additional_metadata()}))?);
    Ok(())
}
