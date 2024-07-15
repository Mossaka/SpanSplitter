use std::time::Duration;

use otel::{traces_enabled, Config};
use tokio::time::interval;
use tracing::{instrument, span, Instrument, Level, Span};

mod otel;

#[instrument()]
async fn func_a() {
    // sleep 0.1 seconds
    std::thread::sleep(std::time::Duration::from_millis(100));

    let parent_span = Span::current();
    
    tokio::spawn(async move {
        let start_time = std::time::Instant::now();
        let mut interval = interval(Duration::from_secs(2));
        let parent_span = Span::current();

        let mut blocking_task = tokio::task::spawn_blocking(|| func_b(10));
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    let current_span = span!(parent: &parent_span, Level::INFO, "interval_span");
                    let _enter = current_span.enter();
                    println!("Operation in progress for {} seconds", start_time.elapsed().as_secs());
                }
                // long running function
                _ = &mut blocking_task => {
                    println!("func_b completed");
                    break;
                }
            }
        }
    }.instrument(parent_span)).await.unwrap();
}

#[instrument()]
fn func_b(i: u64) {
    std::thread::sleep(std::time::Duration::from_secs(i));
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    if traces_enabled() {
        let otel_config = Config::build_from_env()?;
        let _guard = otel_config.init()?;
        func_a().await;
    }
    Ok(())
}