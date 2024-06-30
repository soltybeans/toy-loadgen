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
use tokio::sync::mpsc::Sender;
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

struct UpstreamResult {
    success_code: bool,
    p50_response_time_micros: f32,
}


#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {

    // CLI check
    let args = TestParams::parse();
    println!("Rate is {}", args.rate);
    println!("Total is {}", args.total);
    println!("Address is {}", args.address);

    // Validation
    let address = args.address;
    validate_address(address.as_str().trim())?;
    let address = Arc::new(address);
    if args.total < args.rate {
        println!("Please specify a `total` value that is >= `rate` ");
        return Ok(());
    }

    let client = Client::builder(TokioExecutor::new())
        .pool_idle_timeout(Duration::from_secs(30))
        .pool_timer(TokioTimer::new())
        .http2_only(true)
        .build_http();

    // The reason for wrapping in Arc and Mutex is to ensure strong consistency when counting down from the max total calls allowed.
    let total_calls = Arc::new(Mutex::new(args.total));

    // We use a channel and wait for a single message that signals we've reached our call limit.
    let (tx, rx) = mpsc::channel::<()>(1);

    // We need to sustain the call rate, therefore we use tokio's interval.
    let mut time_interval = time::interval(Duration::from_micros(1_000_000));
    time_interval.tick().await; // the first tick is immediate.
    while rx.is_empty() {
        sustain_call_rate(args.rate, &address, client.clone(), &total_calls, tx.clone(), &mut time_interval).await.unwrap();
    }

    Ok(())
}

async fn sustain_call_rate(rate: u32, address: &Arc<String>, client: Client<HttpConnector, Full<Bytes>>, total_calls: &Arc<Mutex<u32>>, tx: Sender<()>, time_interval: &mut Interval) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut join_handles: Vec<JoinHandle<()>> = vec![];
    // This is the _rate_ that needs to be sustained UNTIL the `total` calls have been made.
    // We use the rate to bound the number of concurrent tasks
    for _ in 0..rate {
        let calls = Arc::clone(total_calls);
        let client_conn = client.clone(); // this does _NOT_ need to be locked - we WANT multiple, concurrent requests.
        let addr = Arc::clone(address);
        let tx_for_task = tx.clone();
        join_handles.push(tokio::spawn(async move {
            // Make sure we're within `total` limit - strong consistency needed here hence Mutex
            let mut job_number = calls.lock().await;
            if *job_number > 0 {
                *job_number -= 1;
            } else {
                if (tx_for_task.send(()).await).is_ok() {
                    println!("Total call limit reached!");
                }
                return;
            }

            // Would have been nice to re-use the request object created from _outside_ this task
            let request: Request<Full<Bytes>> = Request::builder()
                .method(Method::GET)
                .uri(format!("http://{}", addr))
                .body(Full::from(" "))
                .expect("error constructing request!");

            // For the 1 RPS case - we need to make sure 1 second has passed before the next request.
            let mut time_interval = time::interval(Duration::from_micros(1_000_000));
            time_interval.tick().await; // the first tick is immediate.

            let start_time = Instant::now();
            let future = client_conn.request(request);
            let res = future.await.unwrap();
            let (parts, body) = res.into_parts();
            let data = body.collect().await.unwrap();
            let elapsed_time_micros = start_time.elapsed().as_micros();
            println!("Response status is: {:?}", parts.status);
            // TODO: Do data capture: Store error rate duration
        }));
    }
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
