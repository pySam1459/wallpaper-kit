# Example scene inventory

Inspected read-only on 2026-10-09. This note contains metadata, not extracted artwork or proprietary shader code. It grounds the [renderer specification](../spec/wallpaper-kit.md).

## Input identity

| Field | Observed value |
| --- | --- |
| Workshop item | [3780699842](https://steamcommunity.com/sharedfiles/filedetails/?id=3780699842) |
| Title | Yuta Okkotsu \| Jujutsu Kaisen |
| Local folder | `C:/Program Files (x86)/Steam/steamapps/workshop/content/431960/3780699842` |
| Engine asset root | `C:/Program Files (x86)/Steam/steamapps/common/wallpaper_engine/assets` |
| Package bytes | 335,905,371 |
| Package SHA-256 | `551F74EECA12B2F760BFF5A9C79F9F28C623C1E1D31C3642719B67633FF91F14` |
| Package signature | `PKGV0024` |
| Entries | 42 |
| Package payload starts at | Byte 1,913 |
| Project type / scene version | Scene / 5 |
| Scene JSON size | 8,718 bytes |

The folder also contains project metadata, a GIF preview, and cached shader blobs. Those blobs are not an implementation input. Package contents were inspected through indexed reads without writing to the originals.

## Scene features

The camera is orthographic at 3840 × 2160. HDR, bloom, and camera shake are disabled. Camera parallax is enabled, with amount 0.2, delay 0.5, and mouse influence 0.1.

The twelve objects comprise seven image layers, one particle layer, one light-shaft shape, one accumulated-scene composition layer, and two sound layers. The seven images use six distinct materials because the birds appear twice.

| Feature | Observed requirement |
| --- | --- |
| Image animation | Two sprite assets, each with 480 frames |
| Cloud motion | Two instances; one uses a texture mask |
| Gaussian blur | Two instances; horizontal and vertical passes |
| Light shafts | Procedural corner rays with authored material combinations |
| Chromatic aberration | Composition-layer effect, before foreground and character |
| Particles | Hexagonal bokeh sprite emitter with movement, alpha fade, random initial values and additive blending |
| Audio | Two MP3 wind tracks; MVP is explicitly muted |

The bokeh definition has a maximum count of 500 and a spherical emitter rate of 5, with radius bounds 64–1024. Initializers include lifetime 3–5 seconds, size 512–1025, randomized velocity, color, rotation and alpha. Instance overrides include alpha 0.64, brightness 1.95, count 0.53, rate 1.51, size 1.23 and speed 0.1. These are observed fields; their precise scaling semantics must be confirmed against the reference.

No SceneScript or mesh entries were found in the package, and no script, timeline or puppet bindings were found in the inspected scene JSON. This is evidence for this package, not a claim about the whole scene format.

## Texture and animation metadata

Textures use `TEXV0005` and `TEXI0001`. Static images use `TEXB0004`; bokeh uses `TEXB0003`. Both sprite assets use `TEXB0004` and `TEXS0003`. All inspected `TEXB0004` video flags are zero.

| Asset | Storage and dimensions | Animation |
| --- | --- | --- |
| Character | Format 4, DXT5/BC3; 30 pages, first page 8004 × 6904; first decoded page 55,259,616 bytes; package entry 259,987,854 bytes | 480 frames, 2001 × 1726; 0.06 seconds each; about 28.8 seconds per loop |
| Birds | Format 0, RGBA8888; 27 pages, first page 7680 × 8100; first decoded page 248,832,000 bytes; package entry 58,230,136 bytes | 480 frames, 3840 × 900; 0.06 seconds each; about 28.8 seconds per loop |
| Background/clouds | PNG payloads, 3840 × 2160, five mip levels | Static |
| Fils | PNG payloads, 3840 × 687, five mip levels; model has crop offset | Static |
| Foreground | PNG payloads, 3147 × 795, four mip levels | Static |
| Bokeh sprite | RGBA8888, 512 × 512, four mip levels, LZ4 payload | Particle texture |
| Cloud mask | Format 9, R8, 1920 × 1080, one mip level, LZ4 payload | Effect mask |

Both sprite parsers consumed exactly their package entry lengths when skipping payloads and reading all frame records. Frame records contain page identity, duration, and atlas rectangle basis. The measured duration sum is approximately 28.799999 seconds due to floating-point storage.

First-page sizes illustrate the residency risk; pages near the end may differ. Actual totals and peak residency must be computed from every page in the implementation inventory rather than multiplying the first-page size by the page count. Two decoded bird pages alone occupy about 474.6 MiB. BC3 frame cropping also needs care because a 2001-pixel frame width is not aligned to its four-pixel blocks.

## External dependency closure

The package includes the four effect definitions, their shaders and materials, but references engine-provided assets. An initial inspection identified:

- Built-in generic image and particle shaders.
- The project-layer model and composition material. The inspected model marks itself as passthrough, autosized and a project layer.
- Common shader headers for blur, perspective, fragments and blending.
- Utility noise, perlin and gradient textures referenced by effect defaults.

Conditional shader combinations determine which defaults are actually needed. Completing the transitive dependency closure and recording hashes remains the first implementation gate. This note does not establish that every possible material combination has been inspected or rendered.

## Format references and limits of evidence

[RePKG](https://github.com/notscuffed/repkg) is an MIT-licensed PKG/TEX reference. Its [texture format enum](https://github.com/notscuffed/repkg/blob/master/RePKG.Core/Texture/Enums/TexFormat.cs), [image-container reader](https://github.com/notscuffed/repkg/blob/master/RePKG.Application/Texture/TexImageContainerReader.cs), and [frame-container reader](https://github.com/notscuffed/repkg/blob/master/RePKG.Application/Texture/TexFrameInfoContainerReader.cs) informed the binary interpretation. The local metadata was read independently; no renderer behavior was established by running that project.

The [official Wallpaper Engine shader documentation](https://docs.wallpaperengine.io/en/scene/shader/syntax.html) explains custom preprocessing and cross-language shader conventions. Source availability in an asset package does not grant redistribution rights.

Texture payloads were not fully decompressed, images were not visually reviewed, and the scene was not rendered during this specification task. Blend conventions, effect fidelity, complete dependencies, cold-load cost, hardware behavior and streaming performance remain to be verified. The package hash scopes these observations if Workshop content is later updated.
