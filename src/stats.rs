use crate::ADC_BUFFER_LEN;
use crate::ADC_MIN_THRESHOLD;
use crate::THRESHOLD_BUFFER;

fn stats(buf: &[f64; ADC_BUFFER_LEN]) -> (f64, f64) {
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

pub fn check_ring(
    count: usize,
    elapsed: u64,
    buf: &[f64; ADC_BUFFER_LEN],
    prev: &mut [f64; THRESHOLD_BUFFER],
) -> bool {
    let (mean, stddev) = stats(buf);
    let threshold = prev.iter().sum::<f64>() / prev.len() as f64;

    // Trigger if stddev > 2.5 * threshold
    let ring = stddev > threshold * 2.5_f64;

    if mean < ADC_MIN_THRESHOLD {
        // Hall-effect sensor probably off - ignore readings
    } else if ring {
    } else {
        // Update threshold buffer
        for i in 0..(THRESHOLD_BUFFER - 1) {
            prev[i] = prev[i + 1];
        }
        prev[THRESHOLD_BUFFER - 1] = stddev;
    }
    log::info!("[{count}/{elapsed:06}] Mean: {mean:.4} :: Std Dev: {stddev:.4}/{threshold:.4} :: Ring: {ring}");
    ring
}
