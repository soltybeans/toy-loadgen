use std::fmt::{Error};
use std::time::{Duration, Instant};
use clap::Parser;
use http_body_util::{BodyExt, Full};

use hyper::{Method, Request};
use hyper::body::{Bytes};
use hyper_util::client::legacy::Client;
use hyper_util::rt::{TokioExecutor, TokioTimer};


#[derive(Parser, Debug)]
#[command(about, long_about = None)]
struct TestParams {
    /// Fixed call rate (per second)
    #[arg(short, long)]
    rate: u32,

    /// Maximum number of requests
    #[arg(short, long)]
    total: u32,

    /// Address of the form <endpoint>:<port>. Example: nghttp2.org:80
    address: String,
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

    // Hyper-Client - _SINGLE_ connection opening
    // ASSUMPTION - we will use ONE TCP connection to execute N concurrent requests for the host:port.
    // ASSUMPTION - the application supports h2c i.e. ALPN not required (TLS with H2 not used).
    // i.e. nghttp2.org:80 or podinfo with h2c.enabled = true
    let client = Client::builder(TokioExecutor::new())
        .pool_idle_timeout(Duration::from_secs(30))
        .pool_timer(TokioTimer::new())
        .http2_only(true)
        .build_http();

    let req: Request<Full<Bytes>> = Request::builder()
        .method(Method::GET)
        .uri(format!("http://{}", address))
        .body(Full::from(" "))
        .expect("establishing connection failed!");

    // This part needs to be concurrent
    let start_time = Instant::now();
    let future = client.request(req);
    let res = future.await.unwrap();
    let (parts, body) = res.into_parts();
    println!("Response status is: {:?}", parts.status);
    let data = body.collect().await.unwrap();
    let elapsed_time = start_time.elapsed().as_micros();
    println!("Response is : {:?}", data);
    println!("[elapsed time] : {}", elapsed_time);

    // TODO: Time the request-response (stream end) and make sure to capture results.
    // TODO: Conditionally update error rate

    Ok(())
}

fn validate_address(address: &str) -> Result<(), Error> {
    let parts: Vec<&str> = address.split(':').collect();
    if parts[1].parse::<u32>().is_err() {
        println!("Specified port is invalid!");
    }
    Ok(())
}
