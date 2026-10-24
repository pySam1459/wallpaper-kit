//! Independent smooth force field for tolerant playback, not engine noise.

use super::{Particle, Random, Range};

#[derive(Clone, Copy)]
pub(super) struct Turbulence {
    pub mask: [f32; 2],
    pub scale: f32,
    pub timescale: f32,
    pub speed: Range,
    pub phase: Range,
}

impl Turbulence {
    pub fn offset(self, p: &Particle, age: f32) -> ([f32; 3], [f32; 3]) {
        let mut rng = Random(p.seed ^ 0xbb67ae8584caa73b);
        let speed = f64::from(self.speed.sample(&mut rng));
        let phase = f64::from(self.phase.sample(&mut rng));
        let mut position = [0.; 3];
        let mut velocity = [0.; 3];
        // Two spatial waves give a coherent, seeded field. Evaluate along the
        // initial trajectory so integration is analytic and independent of the
        // seek distance, sample rate and previous samples. Feedback of displaced
        // positions into the field and exact engine noise remain unverified.
        for axis in 0..2 {
            if self.mask[axis] == 0. {
                continue;
            }
            for direction in [[1., 0.73], [-0.41, 1.37]] {
                let direction = if axis == 0 {
                    direction
                } else {
                    [-direction[1], direction[0]]
                };
                let spatial = |v: [f32; 3]| {
                    f64::from(self.scale)
                        * (f64::from(v[0]) * direction[0] + f64::from(v[1]) * direction[1])
                };
                let omega = f64::from(self.timescale) + spatial(p.velocity);
                let phase = phase + spatial(p.origin) + p.born * f64::from(self.timescale);
                let (displacement, change) = response(phase, omega, f64::from(age));
                let amplitude = speed * f64::from(self.mask[axis]) * 0.5;
                position[axis] += (amplitude * displacement) as f32;
                velocity[axis] += (amplitude * change) as f32;
            }
        }
        (position, velocity)
    }
}

// Integrals of sin(phase + omega*t). Series at zero preserve continuity for
// zero/reversed noise speed and cancellation with the spatial wave speed.
fn response(phase: f64, omega: f64, age: f64) -> (f64, f64) {
    let x = omega * age;
    let (sinc, cosc, integral) = if x.abs() < 0.01 {
        let square = x * x;
        (
            1. - square / 6. + square * square / 120.,
            0.5 - square / 24. + square * square / 720.,
            1. / 6. - square / 120. + square * square / 5040.,
        )
    } else {
        (
            x.sin() / x,
            (1. - x.cos()) / (x * x),
            (x - x.sin()) / (x * x * x),
        )
    };
    let (sin, cos) = phase.sin_cos();
    (
        age * age * (sin * cosc + cos * x * integral),
        age * (sin * sinc + cos * x * cosc),
    )
}
