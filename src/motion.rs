use crate::Size;
use std::time::Duration;

// Fixture contract: a 1/8-width orange bar spans the middle half of the image.
// Forty 50-ms steps move from the left edge to the right edge, then wrap.
pub(crate) fn rectangle(size: Size, time: Duration) -> [i32; 4] {
    let width = (size.width() / 8).max(1);
    let step = (time.as_millis() % 2_000) as u32 / 50;
    let left = (size.width() - width) * step / 39;
    let top = size.height() / 4;
    let bottom = (size.height() - top).max(top + 1);
    [
        left as i32,
        top as i32,
        (left + width) as i32,
        bottom as i32,
    ]
}
