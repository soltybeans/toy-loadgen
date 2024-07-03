use std::sync::Arc;
use std::time::Instant;
use http_body_util::{BodyExt, Full};
use hyper::{Method, Request};
use hyper::body::Bytes;
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use tokio::sync::mpsc::{Sender, UnboundedSender};
use tokio::sync::Mutex;
use tokio::time::Interval;

pub async fn sustain_call_rate(
    rate: u32,
    address: &Arc<String>,
    client: Client<HttpConnector, Full<Bytes>>,
    total_calls: &Arc<Mutex<u32>>,
    tx: Sender<()>,
    tx_result_status_codes: UnboundedSender<u16>,
    tx_result_duration_micros: UnboundedSender<u128>,
    time_interval: &mut Interval) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    for _ in 0..rate {
        let calls = Arc::clone(total_calls);
        let client_conn = client.clone();
        let addr = Arc::clone(address);
        let tx_end_check = tx.clone();
        let tx_status = tx_result_status_codes.clone();
        let tx_duration = tx_result_duration_micros.clone();
        tokio::spawn(async move {
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
                .expect("errors constructing request!");

            let start_time = Instant::now();
            let response_future = client_conn.request(request);
            let res = response_future.await.unwrap();
            let (parts, body) = res.into_parts();
            // Data itself is not as important how long it takes to be fully streamed back to us.
            // We need all the data to stop timing.
            let _data = body.collect().await.unwrap();
            let elapsed_time_micros = start_time.elapsed().as_millis();

            tx_status.send(parts.status.as_u16()).expect("cannot send status_code!");
            tx_duration.send(elapsed_time_micros).expect("cannot send duration!");
        });
    }

    // Use the pre-determined interval and tick preserve the call rate.
    time_interval.tick().await;

    Ok(())
}
