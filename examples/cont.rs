    // Continuous ADC

    use esp_idf_svc::hal::adc::{AdcContConfig, AdcContDriver, AdcMeasurement, Attenuated};
    let mut config = AdcContConfig::default();
    config.sample_freq = esp_idf_hal::units::Hertz(5000);
    config.frame_measurements = 1000;
    //.sample_freq(esp_idf_hal::units::Hertz(5000));
    println!("ADC Config: {:?}", config);

    let adc_pin = Attenuated::db6(peripherals.pins.gpio2);
    let mut adc = AdcContDriver::new(peripherals.adc1, &config, adc_pin)?;

    adc.start()?;

    let mut samples = [AdcMeasurement::default(); ADC_BUFFER_LEN];

    loop {
        if let Ok(_n) = adc.read(&mut samples, 10) {
            for s in &samples {
                print!("{},", s.data());
            }
            println!("");
        }
    }
