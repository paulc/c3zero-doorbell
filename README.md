
ESP32C3-Zero Doorbell Ring Detector (using Hall effect current sensor)


To plot raw samples (set ADC_BUFFER_LEN & SAMPLE_RATE in examples/sample_timer.rs)

```
cargo run --release --example sample_timer | sed -e '1,/===/d' | uv run --with matplotlib python plot.py
```
