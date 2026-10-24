use super::{pitch_y,seconds_at_x,x_at_seconds};
use crate::{app::App,synthesis::{plan::TempoMapping,rendered::PitchTrack}};
use eframe::egui::*;

pub(super) fn draw(app:&App,painter:&Painter,canvas:Rect,pitch:&PitchTrack,tempo:&TempoMapping,active:bool) {
    let Some([lower,upper])=&pitch.unison_range else{return};
    if lower.len()!=pitch.values.len()||upper.len()!=pitch.values.len(){return}
    let clip=painter.clip_rect();
    let first=((seconds_at_x(app,tempo,canvas,clip.left())-pitch.start_seconds)/pitch.frame_seconds).floor().max(0.0) as usize;
    let last=((seconds_at_x(app,tempo,canvas,clip.right())-pitch.start_seconds)/pitch.frame_seconds).ceil().max(0.0) as usize;
    let color=Color32::from_rgba_unmultiplied(213,210,213,if active{26}else{14});
    let mut mesh=Mesh::default();
    for segment in &pitch.segments {
        let from=first.saturating_sub(1).max(segment.start);let to=last.saturating_add(2).min(segment.end);
        let mut previous=None;
        for index in from..to {
            if !lower[index].is_finite()||!upper[index].is_finite(){previous=None;continue}
            let x=x_at_seconds(app,tempo,canvas,pitch.start_seconds+index as f64*pitch.frame_seconds);
            let vertex=mesh.vertices.len() as u32;
            mesh.colored_vertex(pos2(x,pitch_y(app,canvas,lower[index])),color);
            mesh.colored_vertex(pos2(x,pitch_y(app,canvas,upper[index])),color);
            if let Some(old)=previous {mesh.add_triangle(old,old+1,vertex);mesh.add_triangle(old+1,vertex+1,vertex);}
            previous=Some(vertex);
        }
    }
    if !mesh.indices.is_empty(){painter.add(Shape::mesh(mesh));}
}
