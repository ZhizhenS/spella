//! Render the real offline dictionary results through the production presenter.
use retype_candidate_ui::surface::Surface;
use retype_types::{CandidateAppearance, CandidateTheme, LearningStore, RenderState};
use std::path::PathBuf;
struct ReadOnly;
impl LearningStore for ReadOnly {
    fn record(&self, _: retype_types::LearningEvent) {}
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let directory = PathBuf::from(args.next().ok_or("pass an output directory")?);
    let dictionaries = PathBuf::from(args.next().ok_or("pass a dictionary directory")?);
    let vocab = std::fs::read_dir(&dictionaries)?
        .filter_map(Result::ok)
        .map(|x| x.path())
        .find(|x| {
            x.file_name()
                .and_then(|s| s.to_str())
                .is_some_and(|name| name.starts_with("english-e") && name.ends_with(".bin"))
        })
        .ok_or("English vocabulary not found")?;
    retype_english::install(&vocab).map_err(std::io::Error::other)?;
    std::fs::create_dir_all(&directory)?;
    for (query, theme, name) in [
        ("hel", CandidateTheme::Light, "英文释义候选.png"),
        ("pingguo", CandidateTheme::Light, "拼音查英文候选.png"),
        ("nihao", CandidateTheme::Dark, "深色候选.png"),
        ("iamt", CandidateTheme::Light, "连写短语候选.png"),
        ("ia", CandidateTheme::Dark, "连写歧义候选.png"),
    ] {
        let state = RenderState {
            composition: query.into(),
            page_size: 5,
            candidates: retype_english::current()
                .complete_input(query, &ReadOnly, 24, true, true, true),
            ..Default::default()
        };
        let mut surface = Surface::default();
        surface.set_appearance(CandidateAppearance {
            theme,
            ..Default::default()
        });
        let widths = surface.measure(&state, 472, 1.5);
        let bitmap = surface.render(&state, &widths[..state.visible().len()], 3, 720, 1.5);
        let file = std::fs::File::create(directory.join(name))?;
        let mut encoder = png::Encoder::new(
            std::io::BufWriter::new(file),
            bitmap.geometry.width as u32,
            bitmap.geometry.height as u32,
        );
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let pixels: Vec<u8> = bitmap
            .pixels
            .iter()
            .flat_map(|pixel| [(*pixel >> 16) as u8, (*pixel >> 8) as u8, *pixel as u8])
            .collect();
        encoder.write_header()?.write_image_data(&pixels)?;
        println!("{query}: {} candidates rendered", state.candidates.len());
    }
    Ok(())
}
