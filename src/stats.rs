use crate::adc::ADC_BUFFER_LEN;
use crate::adc::ADC_MIN_THRESHOLD;
use crate::adc::THRESHOLD_BUFFER;

const TRIGGER: f64 = 2.5;

pub fn stats(buf: &[f64; ADC_BUFFER_LEN]) -> (f64, f64) {
    let mean = buf.iter().sum::<f64>() / ADC_BUFFER_LEN as f64;
    let var = buf
        .iter()
        .map(|v| {
            let diff = mean - *v;
            diff * diff
        })
        .sum::<f64>();
    let var = var / ADC_BUFFER_LEN as f64;
    (mean, var.sqrt())
}

pub fn check_ring(mean: f64, stddev: f64, prev: &mut [f64; THRESHOLD_BUFFER]) -> (bool, f64) {
    let threshold = prev.iter().sum::<f64>() / prev.len() as f64;
    let ring = stddev > threshold * TRIGGER;
    if mean > ADC_MIN_THRESHOLD && !ring {
        // Update threshold buffer if above ADC_MIN_THRESHOLD and ring not deteced
        for i in 0..(THRESHOLD_BUFFER - 1) {
            prev[i] = prev[i + 1];
        }
        prev[THRESHOLD_BUFFER - 1] = stddev;
    }
    (ring, threshold)
}
