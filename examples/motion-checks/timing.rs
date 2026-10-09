use serde_json::{Value, json};

pub struct Sample {
    pub slot: u32,
    pub frame_id: u64,
    pub playback_ms: f64,
    pub requested_ms: f64,
    pub ready_ms: f64,
    pub acquisition_ms: f64,
}

impl Sample {
    pub fn json(&self) -> Value {
        json!({"slot": self.slot, "frame_id": self.frame_id, "playback_ms": self.playback_ms,
            "requested_ms": self.requested_ms, "ready_ms": self.ready_ms,
            "acquisition_ms": self.acquisition_ms})
    }
}

pub fn summary(samples: &[Sample], period_ms: f64, slots: u32) -> Value {
    let mut latency: Vec<_> = samples
        .iter()
        .map(|s| s.ready_ms - s.requested_ms)
        .collect();
    latency.sort_by(f64::total_cmp);
    let percentile = |p: f64| {
        latency
            .get((latency.len() as f64 * p).ceil().max(1.0) as usize - 1)
            .copied()
    };
    let missed = samples
        .iter()
        .filter(|s| s.ready_ms > f64::from(s.slot + 1) * period_ms)
        .count();
    let max_gap = samples
        .windows(2)
        .map(|pair| pair[1].ready_ms - pair[0].ready_ms)
        .reduce(f64::max);
    json!({"target_period_ms":period_ms, "scheduled_slots":slots, "completed_samples":samples.len(),
        "skipped_slots":slots as usize - samples.len(), "missed_deadlines":missed,
        "request_to_ready_ms":{"p50":percentile(0.5),"p95":percentile(0.95),"p99":percentile(0.99),"max":latency.last()},
        "longest_ready_gap_ms":max_gap,
        "max_try_frame_call_ms":samples.iter().map(|s|s.acquisition_ms).reduce(f64::max)})
}

pub fn stall_self_test() -> bool {
    let samples = [
        Sample {
            slot: 0,
            frame_id: 1,
            playback_ms: 0.0,
            requested_ms: 0.0,
            ready_ms: 2.0,
            acquisition_ms: 0.1,
        },
        Sample {
            slot: 1,
            frame_id: 2,
            playback_ms: 33.0,
            requested_ms: 33.0,
            ready_ms: 153.0,
            acquisition_ms: 0.1,
        },
    ];
    let result = summary(&samples, 1000.0 / 30.0, 5);
    result["missed_deadlines"] == 1
        && result["skipped_slots"] == 3
        && result["request_to_ready_ms"]["max"] == 120.0
        && result["longest_ready_gap_ms"] == 151.0
}

#[cfg(test)]
mod tests {
    #[test]
    fn rejects_an_injected_timing_stall() {
        assert!(super::stall_self_test());
    }
}
