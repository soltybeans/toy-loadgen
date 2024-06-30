use std::fmt::{Error};
use std::sync::{Arc};
use std::time::{Duration, Instant};
use clap::Parser;
use http_body_util::{BodyExt, Full};

use hyper::{Method, Request};
use hyper::body::{Bytes};
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::{TokioExecutor, TokioTimer};
use tokio::sync::{mpsc, Mutex};
use tokio::sync::mpsc::{Sender, UnboundedSender};
use tokio::task::JoinHandle;
use tokio::time;
use tokio::time::Interval;


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

struct Workload {
    rate: u32,
    address: &'static Arc<String>,
    client: Client<HttpConnector, Full<Bytes>>,
    total_calls: &'static Arc<Mutex<u32>>,
    tx: Sender<()>,
    tx_result_status_codes: UnboundedSender<u16>,
    tx_result_duration_micros: UnboundedSender<u128>,
    time_interval: &'static mut Interval,
}


#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {

    // CLI check
    let args = TestParams::parse();
    println!("Rate is {}", args.rate);
    println!("Total is {}", args.total);

    // Validation
    let address = args.address;
    validate_address(address.as_str().trim())?;
    let address = Arc::new(address);

    let client = Client::builder(TokioExecutor::new())
        .pool_idle_timeout(Duration::from_secs(30))
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
    let mut time_interval = time::interval(Duration::from_micros(1_000_000));
    time_interval.tick().await; // the first tick is immediate.
    while rx.is_empty() {
        sustain_call_rate(args.rate, &address, client.clone(), &total_calls, tx.clone(), tx_result_status_codes.clone(), tx_result_duration_micros.clone(), &mut time_interval).await.unwrap();
    }

    // Result processing
    while !rx_status_codes.is_empty() {
        result_errors.push(rx_status_codes.recv().await.unwrap());
    }
    while !rx_durations.is_empty() {
        result_durations.push(rx_durations.recv().await.unwrap());
    }

    process_results(result_durations, result_errors).await;

    Ok(())
}

async fn process_results(mut result_durations: Vec<u128>, mut result_errors: Vec<u16>) {
    result_durations.sort_unstable();
    result_errors.sort_unstable();

    let mut total_5xx_responses = 0;
    result_errors.iter().for_each(|value| {
        if value > &499_u16 && value < &599_u16 {
            total_5xx_responses += 1;
        }
    });
    let success_rate: f32 = ((1 - (total_5xx_responses / result_errors.len())) * 100) as f32;
    let median = result_durations.len() / 2;
    println!("success: {} %", success_rate);
    println!("median (microseconds): {}", result_durations[median]);
}


async fn sustain_call_rate(
    rate: u32,
    address: &Arc<String>,
    client: Client<HttpConnector, Full<Bytes>>,
    total_calls: &Arc<Mutex<u32>>,
    tx: Sender<()>,
    tx_result_status_codes: UnboundedSender<u16>,
    tx_result_duration_micros: UnboundedSender<u128>,
    time_interval: &mut Interval) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut join_handles: Vec<JoinHandle<()>> = vec![];
    for _ in 0..rate {
        let calls = Arc::clone(total_calls);
        let client_conn = client.clone();
        let addr = Arc::clone(address);
        let tx_end_check = tx.clone();
        let tx_status = tx_result_status_codes.clone();
        let tx_duration = tx_result_duration_micros.clone();
        join_handles.push(tokio::spawn(async move {
            // Make sure we're within `total` limit - strong consistency needed here hence Mutex
            let mut job_number = calls.lock().await;
            if *job_number > 0 {
                *job_number -= 1;
            } else {
                if (tx_end_check.send(()).await).is_ok() {
                    println!("Total call limit reached...");
                }
                return;
            }

            let request: Request<Full<Bytes>> = Request::builder()
                .method(Method::GET)
                .uri(format!("http://{}", addr))
                .body(Full::from(" "))
                .expect("error constructing request!");

            let start_time = Instant::now();
            let future = client_conn.request(request);
            let res = future.await.unwrap();
            let (parts, body) = res.into_parts();
            // Data is not needed, but body.collect() is useful to stream the FULL response.
            let _data = body.collect().await.unwrap();
            let elapsed_time_micros = start_time.elapsed().as_micros();

            tx_status.send(parts.status.as_u16()).expect("cannot send status_code!");
            tx_duration.send(elapsed_time_micros).expect("cannot send duration!");
        }));
    }

    // Use the pre-determined interval and tick preserve the call rate.
    time_interval.tick().await;

    // Technically - we may choose to wait for all spawned tasks to complete even if total call limit is reached.
    // However, tasks are supposed to be cheap - so we let them drive to completion in the background even when
    // there is no more work to do. We choose not to wait so that we can proceed to analyzing results asap.
    //join_all(join_handles).await;
    Ok(())
}

fn validate_address(address: &str) -> Result<(), Error> {
    let parts: Vec<&str> = address.split(':').collect();
    if parts[1].parse::<u32>().is_err() {
        println!("Specified port is invalid!");
    }
    Ok(())
}
