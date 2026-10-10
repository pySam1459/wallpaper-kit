//! Redistributable combined-playback fixture and independent pixel oracle.

use super::texture_checks as texture;
use serde_json::json;
use wallpaper_kit::Pixels;

pub fn package() -> Vec<u8> {
    let scene = json!({"version":5,"general":{"orthogonalprojection":{"width":8,"height":4}},"objects":[
        {"id":1,"image":"models/background.json","origin":"4 2 0","size":"8 4"},
        {"id":2,"image":"models/birds.json","origin":"1 2.125 0","size":"2 2"},
        {"id":3,"image":"models/birds.json","origin":"7 2.125 0","size":"2 2"},
        {"id":4,"image":"models/character.json","origin":"4 2.125 0","size":"4 2"},
        {"id":5,"shape":"unsupported"}
    ]});
    let mut entries = vec![(
        "scene.json".to_string(),
        serde_json::to_vec(&scene).unwrap(),
    )];
    for (name, sprite, pixels, durations) in [
        ("background", false, vec![30, 40, 50, 255], [0.125, 0.25]),
        (
            "birds",
            true,
            vec![
                255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 0, 255,
            ],
            [0.125, 0.25],
        ),
        (
            "character",
            true,
            vec![
                255, 255, 255, 255, 0, 0, 0, 255, 128, 128, 128, 255, 240, 160, 80, 255,
            ],
            [0.25, 0.25],
        ),
    ] {
        entries.push((
            format!("models/{name}.json"),
            serde_json::to_vec(
                &json!({"autosize":true,"material":format!("materials/{name}.json"),"cropoffset":if sprite {"0 0"} else {"2 -3"}}),
            )
            .unwrap(),
        ));
        entries.push((format!("materials/{name}.json"),serde_json::to_vec(&json!({"passes":[{"shader":"genericimage4","textures":[name],"combos":if sprite {json!({"SPRITESHEET":1})} else {json!({})},"blending":"translucent","cullmode":"nocull","depthtest":"disabled","depthwrite":"disabled"}]})).unwrap()));
        let extent = if sprite { [2, 2] } else { [1, 1] };
        let png = texture::png(extent, png::ColorType::Rgba, &pixels);
        let tex = texture::tex(0, extent, &png, true, true);
        let tex = if sprite {
            texture::sprite(
                tex,
                &[
                    (0, durations[0], [0., 0., 1., 0., 0., 2.]),
                    (0, durations[1], [1., 0., 1., 0., 0., 2.]),
                ],
            )
        } else {
            tex
        };
        entries.push((format!("materials/{name}.tex"), tex));
    }
    let references: Vec<_> = entries
        .iter()
        .map(|(name, bytes)| (name.as_str(), bytes.as_slice()))
        .collect();
    texture::package(&references)
}

pub fn indices(time_ms: u64) -> [usize; 2] {
    [
        usize::from(time_ms % 375 >= 125),
        usize::from(time_ms % 500 >= 250),
    ]
}

pub fn check(pixels: &Pixels, time_ms: u64) -> Result<(), String> {
    let [bird, character] = indices(time_ms);
    let width = pixels.size.width();
    let height = pixels.size.height();
    for y in 0..height {
        for x in 0..width {
            let canvas_x = (f64::from(x) + 0.5) * 8. / f64::from(width);
            let canvas_y = (f64::from(y) + 0.5) * 4. / f64::from(height);
            // Offset fixture placement avoids exact point-sampling ties at
            // the tested 180/360 and odd 193-pixel heights on WARP/hardware.
            let expected = if !(0.875..2.875).contains(&canvas_y) {
                [30, 40, 50, 255]
            } else if !(2. ..6.).contains(&canvas_x) {
                match (bird, canvas_y >= 1.875) {
                    (0, false) => [255, 0, 0, 255],
                    (0, true) => [0, 0, 255, 255],
                    (1, false) => [0, 255, 0, 255],
                    _ => [255, 255, 0, 255],
                }
            } else {
                match (character, canvas_y >= 1.875) {
                    (0, false) => [255, 255, 255, 255],
                    (0, true) => [128, 128, 128, 255],
                    (1, false) => [0, 0, 0, 255],
                    _ => [240, 160, 80, 255],
                }
            };
            let offset = ((y * width + x) * 4) as usize;
            if pixels.rgba[offset..offset + 4] != expected {
                return Err(format!(
                    "combined fixture differs at ({x},{y}) for {time_ms} ms"
                ));
            }
        }
    }
    Ok(())
}
