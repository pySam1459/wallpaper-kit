use serde_json::{Value, json};

pub fn distribution(values: &[f64]) -> Value {
    if values.is_empty() {
        return Value::Null;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let percentile = |p: f64| sorted[(p * sorted.len() as f64).ceil().max(1.) as usize - 1];
    json!({"p50":percentile(0.5),"p95":percentile(0.95),"p99":percentile(0.99),"max":sorted.last(),
        "mean":values.iter().sum::<f64>()/values.len() as f64})
}

#[cfg(test)]
mod tests {
    #[test]
    fn percentiles_retain_a_stall_and_empty_is_unavailable() {
        let result = super::distribution(&[1., 2., 3., 120.]);
        assert_eq!(result["p50"], 2.);
        assert_eq!(result["p95"], 120.);
        assert_eq!(result["mean"], 31.5);
        assert!(super::distribution(&[]).is_null());
    }
}
