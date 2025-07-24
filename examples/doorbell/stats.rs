use crate::adc::ADC_BUFFER_LEN;
use crate::adc::ADC_MIN_THRESHOLD;
use crate::adc::THRESHOLD_BUFFER;

const TRIGGER: f32 = 2.5;

pub fn stats(buf: &[f32; ADC_BUFFER_LEN]) -> (f32, f32) {
    let mean = buf.iter().sum::<f32>() / ADC_BUFFER_LEN as f32;
    let var = buf
        .iter()
        .map(|v| {
            let diff = mean - *v;
            diff * diff
        })
        .sum::<f32>();
    let var = var / ADC_BUFFER_LEN as f32;
    (mean, var.sqrt())
}

pub fn check_ring(mean: f32, stddev: f32, prev: &mut [f32; THRESHOLD_BUFFER]) -> (bool, f32) {
    let threshold = prev.iter().sum::<f32>() / prev.len() as f32;
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
