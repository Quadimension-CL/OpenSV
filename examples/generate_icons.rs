use eframe::egui::SizeHint;
use image::{ExtendedColorType, ImageEncoder, codecs::png::PngEncoder};
use std::{error::Error, fs, path::Path};

fn main() -> Result<(), Box<dyn Error>> {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/branding");
    let svg = fs::read(directory.join("app-icon.svg"))?;
    let sizes = [16_u32, 20, 24, 32, 40, 48, 64, 128, 256];
    let mut images = Vec::new();
    for size in sizes {
        let pixels = egui_extras::image::load_svg_bytes_with_size(
            &svg,
            SizeHint::Width(size),
            &Default::default(),
        )?;
        let rgba: Vec<_> = pixels
            .pixels
            .iter()
            .flat_map(|pixel| pixel.to_srgba_unmultiplied())
            .collect();
        let mut png = Vec::new();
        PngEncoder::new(&mut png).write_image(&rgba, size, size, ExtendedColorType::Rgba8)?;
        images.push(png);
    }
    fs::write(directory.join("app-icon.png"), images.last().unwrap())?;

    let mut icon = Vec::new();
    icon.extend_from_slice(&0_u16.to_le_bytes());
    icon.extend_from_slice(&1_u16.to_le_bytes());
    icon.extend_from_slice(&(sizes.len() as u16).to_le_bytes());
    let mut offset = 6 + sizes.len() as u32 * 16;
    for (&size, png) in sizes.iter().zip(&images) {

        icon.extend_from_slice(&[size as u8, size as u8, 0, 0]);
        icon.extend_from_slice(&1_u16.to_le_bytes());
        icon.extend_from_slice(&32_u16.to_le_bytes());
        icon.extend_from_slice(&(png.len() as u32).to_le_bytes());
        icon.extend_from_slice(&offset.to_le_bytes());
        offset += png.len() as u32;
    }
    for png in images {
        icon.extend_from_slice(&png);
    }
    fs::write(directory.join("app-icon.ico"), icon)?;
    println!("Generated app-icon.png and app-icon.ico (9 sizes).");
    Ok(())
}
