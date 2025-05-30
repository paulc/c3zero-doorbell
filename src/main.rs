use esp_idf_hal::delay::TickType;
use esp_idf_hal::gpio::{OutputPin, PinDriver};
use esp_idf_hal::peripherals::Peripherals;
use esp_idf_hal::timer::TimerDriver;
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::adc::{AdcContConfig, AdcContDriver, AdcMeasurement, Attenuated};
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::wifi::EspWifi;

use embedded_svc::http::client::Client as HttpClient;
use esp_idf_svc::http::client::{Configuration as HttpConfiguration, EspHttpConnection};

use doorbell::httpd;
use doorbell::nvs::APStore;
use doorbell::rgb::{self, RgbLayout};
use doorbell::wifi::{self, APConfig};
use doorbell::ws2812_rmt::Ws2812RmtSingle;

const ADC_SAMPLE_RATE: u32 = 1000; // 1kHz sample rate
const ADC_BUFFER_LEN: usize = 50; // 50ms sample buffer
const ADC_MIN_THRESHOLD: f64 = 0.1; // If Hall-Effect sensor is on we should see Vcc/2
                                    // when bell is off - if this is below threshold
                                    // we assume that sensor is powered off
const THRESHOLD_BUFFER: usize = 5;

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

fn main() -> anyhow::Result<()> {
    esp_idf_hal::sys::link_patches();

    // Bind the log crate to the ESP Logging facilities
    esp_idf_svc::log::EspLogger::initialize_default();
    log::info!("Started...");

    // Initialise peripherals
    let peripherals = Peripherals::take()?;
    let sys_loop = EspSystemEventLoop::take()?;
    let nvs_default_partition = EspDefaultNvsPartition::take()?;

    // Status display (C3-Zero onboard WS2812 LED pin = GPIO10)
    let ws2812 = peripherals.pins.gpio10.downgrade_output();
    let channel = peripherals.rmt.channel0;
    let mut status = Ws2812RmtSingle::new(ws2812, channel, RgbLayout::Rgb)?;
    status.set(rgb::OFF)?;

    // Ring led
    let mut ring_led = PinDriver::output(peripherals.pins.gpio6)?;
    ring_led.set_high()?;

    // Initialise WiFi
    let mut wifi: EspWifi<'_> = EspWifi::new(
        peripherals.modem,
        sys_loop.clone(),
        Some(nvs_default_partition.clone()),
    )?;
    wifi::wifi_init(&mut wifi)?;

    // Initial scan
    wifi::wifi_scan(&mut wifi)?;

    // Initislise NVS APStore
    APStore::init(nvs_default_partition.clone())?;

    let mut wifi_config: Option<APConfig> = None;

    for config in wifi::find_known_aps() {
        log::info!("Trying network: {}", config.ssid);
        match wifi::connect_wifi(&mut wifi, &config, 10000) {
            Ok(true) => {
                log::info!("Connected to Wifi: {}", config.ssid);
                wifi_config = Some(config);
                break;
            }
            Ok(false) => {
                log::info!("Failed to connect to Wifi: {}", config.ssid);
            }
            Err(e) => {
                log::info!("Wifi Error: {} [{}]", config.ssid, e);
            }
        }
    }

    log::info!("WiFi Config: {:?}", wifi_config);

    let mut _server = if let Some(config) = wifi_config {
        log::info!("Connected to SSID: {}", config.ssid);
        log::info!("IP: {}", wifi.sta_netif().get_ip_info()?.ip);
        httpd::start_http_server()?
    } else {
        log::info!("No valid config found - starting AP");
        wifi::start_access_point(&mut wifi)?;
        log::info!("AP Mode - {:?}", wifi.ap_netif());
        httpd::start_http_server()?
    };

    adc_cont(
        peripherals.timer00,
        peripherals.adc1,
        peripherals.pins.gpio4,
        &mut status,
        &mut ring_led,
    )
}

fn adc_cont(
    timer: esp_idf_hal::timer::TIMER00,
    adc: esp_idf_hal::adc::ADC1,
    adc_pin: esp_idf_hal::gpio::Gpio4,
    _status: &mut Ws2812RmtSingle<'_>,
    ring_led: &mut PinDriver<'_, esp_idf_hal::gpio::Gpio6, esp_idf_hal::gpio::Output>,
) -> anyhow::Result<()> {
    // Setup Timer
    let mut timer = TimerDriver::new(timer, &Default::default())?;
    timer.enable(true)?;
    println!("=== Timer: {} Hz", timer.tick_hz());

    // Setup ADC
    let mut config = AdcContConfig::default();
    config.sample_freq = esp_idf_hal::units::Hertz(ADC_SAMPLE_RATE);
    config.frame_measurements = ADC_BUFFER_LEN;
    config.frames_count = 2; // Need 2 buffers as frames can be unaligned (?)

    let adc_pin = Attenuated::db11(adc_pin);
    let mut adc = AdcContDriver::new(adc, &config, adc_pin)?;
    adc.start()?;
    println!(
        "=== ADC Samples - Sample Rate: {} / Samples: {} / ADC Config: {:?}",
        ADC_SAMPLE_RATE, ADC_BUFFER_LEN, config
    );

    // State variables
    let mut samples = [AdcMeasurement::default(); ADC_BUFFER_LEN];
    let mut samples_f64 = [0_f64; ADC_BUFFER_LEN];
    let mut ring_state = false;
    let mut debounce = [false; 2];
    let mut count = 0_usize;
    let mut frame = 0_usize;
    let mut prev = [1.0_f64; THRESHOLD_BUFFER];
    let mut ticks = timer.counter()?;

    loop {
        match adc.read(&mut samples, TickType::new_millis(200).ticks()) {
            Ok(n) => {
                let now = timer.counter()?;

                // We dont always get a full frame from the ADC so fill up
                // samples_f64 with the data we do have

                // Make sure we dont overrun samples_f64 array
                let n = if frame + n > ADC_BUFFER_LEN {
                    ADC_BUFFER_LEN - frame
                } else {
                    n
                };

                // Append frame to output
                for i in 0..n {
                    samples_f64[frame + i] = samples[i].data() as f64 / 4096_f64;
                }
                frame += n;

                // When it's full process
                if frame == ADC_BUFFER_LEN {
                    let ring = check_ring(count, now - ticks, &samples_f64, &mut prev);
                    debounce = [debounce[1], ring];
                    match ring_state {
                        true => {
                            if debounce == [false, false] {
                                ring_state = false;
                                ring_led.set_high()?;
                                println!(">> RING STOP",);
                            }
                        }
                        false => {
                            if debounce == [true, true] {
                                ring_state = true;
                                ring_led.set_low()?;
                                println!(">> RING START",);
                                // let _ = std::thread::spawn(|| {
                                //     log::info!(">> Calling start webhook");
                                send_pushover_alert()?;
                                //});
                            }
                        }
                    };
                    count += 1;
                    frame = 0;
                    ticks = now;
                }
            }
            Err(e) => println!("{:?}", e),
        }
    }
}

fn check_ring(
    count: usize,
    elapsed: u64,
    buf: &[f64; ADC_BUFFER_LEN],
    prev: &mut [f64; THRESHOLD_BUFFER],
) -> bool {
    let (mean, stddev) = stats(&buf);
    let threshold = prev.iter().sum::<f64>() / prev.len() as f64;

    // Trigger if stddev > 2.5 * threshold
    let ring = stddev > threshold * 2.5_f64;

    if mean < ADC_MIN_THRESHOLD {
        // Hall-effect sensor probably off - ignore readings
    } else {
        if ring {
        } else {
            // Update threshold buffer
            for i in 0..(THRESHOLD_BUFFER - 1) {
                prev[i] = prev[i + 1];
            }
            prev[THRESHOLD_BUFFER - 1] = stddev;
        }
    }
    println!("[{count}/{elapsed:06}] Mean: {mean:.4} :: Std Dev: {stddev:.4}/{threshold:.4} :: Ring: {ring}");
    ring
}

/// Send an HTTP POST request.
fn send_pushover_alert() -> anyhow::Result<()> {
    use embedded_svc::io::Write;

    // HTTP Client
    let config = &HttpConfiguration {
        crt_bundle_attach: Some(esp_idf_svc::sys::esp_crt_bundle_attach),
        ..Default::default()
    };
    let mut client = HttpClient::wrap(EspHttpConnection::new(&config)?);

    // Pushover API payload
    let app_token = "amfa9dzeck8bongtab3nrta3xux3hj";
    let user = "uomfetdtawqotwp3ii9jpf4buys3p4";
    let message = "DOORBELL";

    let payload =
        format!("{{\"token\":\"{app_token}\",\"user\":\"{user}\",\"message\":\"{message}\"}}");
    let payload = payload.as_bytes();

    // Prepare headers and URL
    let content_length_header = format!("{}", payload.len());
    let headers = [
        ("content-type", "application/json"),
        ("content-length", content_length_header.as_str()),
        ("accept", "application/json"),
    ];

    let url = "https://api.pushover.net/1/messages.json";

    let mut request = client.post(url, &headers)?;

    request.write_all(payload)?;
    request.flush()?;
    log::info!("-> POST {url}");

    let response = request.submit()?;
    log::info!("<- {}", response.status());

    Ok(())
}
