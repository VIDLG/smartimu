use embassy_time::{Duration, Timer};

pub async fn delay_ms(ms: u64) {
    Timer::after(Duration::from_millis(ms)).await;
}
