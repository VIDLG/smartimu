use smartimu_hil::{HilConfig, run};

#[test]
#[ignore = "requires a connected ESP32-C3 SmartIMU board running JSON transport firmware"]
fn connected_board_probes_and_streams_all_enabled_imus() {
    let config = HilConfig::from_env().expect("invalid SmartIMU HIL configuration");
    let report = run(config).unwrap_or_else(|error| panic!("{error}"));

    println!("SmartIMU HIL passed: {report:#?}");
}
