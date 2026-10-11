//! Independent corner light shafts with projective placement and animated noise.
use super::{ImageLayer, InspectionError, RawObject, snapshot};
use crate::{Size, TextureDecoder, TexturePage, TextureRegion};
use std::{collections::BTreeMap, sync::Arc, time::Duration};

#[derive(Debug)]
pub(crate) struct Rays {
    pub homography: [f32; 8],
    pub start_color: [f32; 3],
    pub end_color: [f32; 3],
    pub exponent: f32,
    pub intensity: f32,
    pub feather: [f32; 2],
    pub radius: f32,
    pub scale: [f32; 2],
    pub smoothness: f32,
    pub speed: f32,
    pub noise_amount: f32,
    pub noise_scale: f32,
    pub noise: Arc<TextureRegion>,
    pub gradient: Arc<TextureRegion>,
}

pub(super) struct RayLayer {
    pub id: u64,
    center: [f32; 2],
    size: [f32; 2],
    angle: f32,
    source: Arc<TextureRegion>,
    rays: Arc<Rays>,
}

fn homography(p: [[f32; 2]; 4], asset: &str) -> Result<[f32; 8], InspectionError> {
    let dx1 = p[1][0] - p[2][0];
    let dx2 = p[3][0] - p[2][0];
    let dx3 = p[0][0] - p[1][0] + p[2][0] - p[3][0];
    let dy1 = p[1][1] - p[2][1];
    let dy2 = p[3][1] - p[2][1];
    let dy3 = p[0][1] - p[1][1] + p[2][1] - p[3][1];
    let det = dx1 * dy2 - dx2 * dy1;
    if det.abs() < 0.000001 {
        return Err(InspectionError::invalid(
            asset,
            "degenerate light-shaft quadrilateral",
        ));
    }
    let g = (dx3 * dy2 - dx2 * dy3) / det;
    let h = (dx1 * dy3 - dx3 * dy1) / det;
    let result = [
        p[1][0] - p[0][0] + g * p[1][0],
        p[3][0] - p[0][0] + h * p[3][0],
        p[0][0],
        p[1][1] - p[0][1] + g * p[1][1],
        p[3][1] - p[0][1] + h * p[3][1],
        p[0][1],
        g,
        h,
    ];
    if result.iter().any(|v| !v.is_finite() || v.abs() > 64.)
        || [1. + g, 1. + h, 1. + g + h].iter().any(|v| *v <= 0.00001)
    {
        return Err(InspectionError::invalid(
            asset,
            "unbounded light-shaft perspective",
        ));
    }
    Ok(result)
}

pub(super) fn prepare(
    decoder: &TextureDecoder,
    objects: &[RawObject],
    canvas: Size,
    pages: &mut BTreeMap<(String, u32), Arc<TexturePage>>,
    bytes: &mut u64,
) -> Result<Vec<RayLayer>, InspectionError> {
    let mut result = Vec::new();
    for object in objects {
        let asset = format!("scene.json object {}", object.id);
        let invalid =
            || InspectionError::invalid(&asset, "unsupported corner light-shafts profile");
        if object.shape.as_deref() != Some("quad")
            || object.effects.len() != 1
            || object.properties.keys().any(|k| {
                ![
                    "origin",
                    "size",
                    "scale",
                    "angles",
                    "visible",
                    "castshadow",
                    "locktransforms",
                ]
                .contains(&k.as_str())
            })
        {
            return Err(invalid());
        }
        let effect = &object.effects[0];
        let definition = snapshot::json(&decoder.package, &effect.file)?;
        if definition["replacementkey"] != "lightshafts"
            || definition["version"] != 1
            || effect.passes.len() != 1
        {
            return Err(invalid());
        }
        let pass = &effect.passes[0];
        let combos = &pass["combos"];
        if combos.as_object().is_none_or(|m| m.len() != 4)
            || combos["DIRECTDRAW"] != 1
            || combos["RAYMODE"] != 2
            || combos["RAYCORNER"] != 1
            || combos["RENDERING"] != 1
            || pass.get("textures").is_some_and(|v| !v.is_null())
        {
            return Err(invalid());
        }
        let c = &pass["constantshadervalues"];
        if c.as_object().is_none_or(|m| {
            m.keys().any(|k| {
                ![
                    "point0",
                    "point1",
                    "point2",
                    "point3",
                    "colorastart",
                    "colorend",
                    "colorwexponent",
                    "colorwintensity",
                    "rayfeather",
                    "rayradius",
                    "rayscale",
                    "raysmoothness",
                    "rayspeed",
                    "noiseamount",
                    "noisescale",
                ]
                .contains(&k.as_str())
            })
        }) {
            return Err(invalid());
        }
        let scalar = |name, default, maximum| {
            let n = snapshot::scalar(c.get(name), default, &asset)?;
            if !(0. ..=maximum).contains(&n) {
                return Err(invalid());
            }
            Ok(n)
        };
        let mut points = [[0.; 2]; 4];
        for (index, p) in points.iter_mut().enumerate() {
            *p = snapshot::vector(
                c.get(format!("point{index}").as_str()),
                [[0., 0.], [1., 0.], [1., 1.], [0., 1.]][index],
                &asset,
            )?;
        }
        let mut region = |name: &str| {
            let page = snapshot::page(decoder, name, 0, pages, bytes)?;
            let extent = page.extent();
            Ok(Arc::new(TextureRegion::new(page, [0, 0], extent).map_err(
                |e| InspectionError::invalid(name, e.to_string()),
            )?))
        };
        let rays = Arc::new(Rays {
            homography: homography(points, &asset)?,
            start_color: snapshot::vector(c.get("colorastart"), [1.; 3], &asset)?,
            end_color: snapshot::vector(c.get("colorend"), [1.; 3], &asset)?,
            exponent: scalar("colorwexponent", 1., 16.)?,
            intensity: scalar("colorwintensity", 0.5, 16.)?,
            feather: snapshot::vector(c.get("rayfeather"), [0.; 2], &asset)?,
            radius: scalar("rayradius", 0.12, 4.)?,
            scale: snapshot::vector(c.get("rayscale"), [0.17, 0.43], &asset)?,
            smoothness: scalar("raysmoothness", 1., 4.)?,
            speed: scalar("rayspeed", 0.12, 4.)?,
            noise_amount: scalar("noiseamount", 0., 4.)?,
            noise_scale: scalar("noisescale", 0., 16.)?,
            noise: region("materials/util/noise.tex")?,
            gradient: region("materials/gradient/gradient_iridescent.tex")?,
        });
        if rays.scale.iter().any(|v| *v <= 0. || *v > 16.)
            || rays.feather.iter().any(|v| !(0. ..=1.).contains(v))
            || rays
                .start_color
                .iter()
                .chain(&rays.end_color)
                .any(|v| !(0. ..=16.).contains(v))
        {
            return Err(invalid());
        }
        let origin = snapshot::vector(
            object.properties.get("origin"),
            [
                canvas.width() as f32 * 0.5,
                canvas.height() as f32 * 0.5,
                0.,
            ],
            &asset,
        )?;
        let scale = snapshot::vector(object.properties.get("scale"), [1.; 3], &asset)?;
        let size = snapshot::vector(
            object.properties.get("size"),
            [canvas.width() as f32, canvas.height() as f32],
            &asset,
        )?;
        let angles = snapshot::vector(object.properties.get("angles"), [0.; 3], &asset)?;
        if origin[2] != 0.
            || angles[..2] != [0., 0.]
            || object
                .properties
                .get("castshadow")
                .is_some_and(|v| v != false)
        {
            return Err(invalid());
        }
        let page = Arc::new(TexturePage::rgba([1, 1], vec![255; 4])?);
        let source = Arc::new(
            TextureRegion::new(page, [0, 0], [1, 1])
                .map_err(|e| InspectionError::invalid(&asset, e.to_string()))?,
        );
        let layer = RayLayer {
            id: object.id,
            center: [origin[0], origin[1]],
            size: [size[0] * scale[0], size[1] * scale[1]],
            angle: angles[2],
            source,
            rays,
        };
        layer.layer(Duration::ZERO)?;
        result.push(layer);
    }
    Ok(result)
}

impl RayLayer {
    pub(super) fn layer(&self, time: Duration) -> Result<ImageLayer, InspectionError> {
        let mut layer = ImageLayer::new(
            self.source.clone(),
            self.center,
            self.size,
            self.angle,
            1.,
            1.,
        )
        .map_err(|e| InspectionError::invalid("light shafts", e.to_string()))?;
        layer.rays = Some((self.rays.clone(), time));
        Ok(layer)
    }
}
