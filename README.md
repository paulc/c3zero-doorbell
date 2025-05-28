
ESP32C3-Zero Doorbell Ring Detector (using Hall effect current sensor)


To plot raw samples (set SAMPLES & SAMPLE_RATE in examples/sample_timer.rs)

```
cargo run --release --example sample_timer | uv run --with matplotlib python plot.py
cargo run --release --example sample_continuous | uv run --with matplotlib python plot.py
```
