use std::time::{Duration, Instant};

fn main() {
    println!("Trying to spin for 10s.");
    let start_time = Instant::now();
    while start_time.elapsed() < Duration::from_secs(10) {}
}
