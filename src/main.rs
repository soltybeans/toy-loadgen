use std::fmt::{Debug};
use std::sync::Arc;
use std::time::Duration;

use clap::Parser;
use hyper_util::client::legacy::Client;
use hyper_util::rt::{TokioExecutor, TokioTimer};
use tokio::sync::{mpsc, Mutex};
use tokio::time::interval;

use core::sustain_call_rate;
use errors::LoadGenError;

use crate::results::process_results;

mod results;
mod errors;
mod core;

#[derive(Parser, Debug)]
#[command(about, long_about = None)]
struct TestParams {
    /// Fixed call rate (per second)
    #[arg(short, long, default_value_t = 1)]
    rate: u32,

    /// Maximum number of requests
    #[arg(short, long, default_value_t = 1)]
    total: u32,

    /// Address of the form <endpoint>:<port>. Example: nghttp2.org:80
    address: String,
}


#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {

    // CLI check
    let args = TestParams::parse();
    println!("Rate is {} rps", args.rate);
    println!("Total is {}", args.total);

    // Validation
    let address = args.address;
    if let Err(e) = validate_address(address.as_str().trim()) {
        println!("{}", e);
        return Err(e.into());
    }
    let address = Arc::new(address);
    let rate = args.rate;


    let client = Client::builder(TokioExecutor::new())
        .pool_idle_timeout(Duration::from_secs(5))
        .pool_timer(TokioTimer::new())
        .http2_only(true)
        .build_http();

    // The reason for wrapping in Arc and Mutex is to ensure strong consistency when counting down from the max total calls allowed.
    let total_calls = Arc::new(Mutex::new(args.total));

    // We use a channel and wait for a single message that signals we've reached our call limit.
    let (tx, rx) = mpsc::channel::<()>(1);

    // Another two channels will be used solely for capturing raw results (status_code and duration).
    // Their results will be collected into their respective vectors.
    let (tx_result_status_codes, mut rx_status_codes) = mpsc::unbounded_channel::<u16>();
    let (tx_result_duration_micros, mut rx_durations) = mpsc::unbounded_channel::<u128>();
    let mut result_errors: Vec<u16> = vec![];
    let mut result_durations: Vec<u128> = vec![];

    // We need to sustain the call rate, therefore we use tokio's interval.
    let mut time_interval = interval(Duration::from_secs(1));
    time_interval.tick().await; // the first tick is immediate.

    while rx.is_empty() {
        sustain_call_rate(rate, &address, client.clone(), &total_calls, tx.clone(), tx_result_status_codes.clone(), tx_result_duration_micros.clone(), &mut time_interval).await.unwrap();
    }

    // Result processing
    // This can be optimized further, we're doing full buffering of all the response codes and durations
    // in their respective channels until _after_ the total calls have been made. Our load generator
    // also has a flaw in that it will hang indefinitely if no requests can be made. This can be addressed by wrapping
    // the result collection in a tokio timeout itself.
    // We use try_recv to know when an Empty error occurs as a signal for no more results (even if they are delayed).
    while !rx_status_codes.try_recv().is_err() {
        result_errors.push(rx_status_codes.recv().await.unwrap());
    }
    while !rx_durations.try_recv().is_err() {
        result_durations.push(rx_durations.recv().await.unwrap());
    }

    if process_results(result_durations, result_errors).await.is_err() {
        return Err(LoadGenError::NoResultsError.into());
    }
    Ok(())
}


fn validate_address(address: &str) -> Result<(), LoadGenError> {
    let parts: Vec<&str> = address.split(':').collect();
    let port = parts[1];
    if port.parse::<u32>().is_err() {
        println!("Specified port is invalid!");
        return Err(LoadGenError::InvalidPortError(port.to_string()));
    }
    Ok(())
}