## HTTP/2 Load Generator

### Assumptions
* The service-under-test is http2 enabled and can accept HTTP2 _without_ TLS  
* We do not need to tweak lower level settings like `max concurrent streams` or use `h2` directly. Whatever opaque [defaults](https://github.com/hyperium/hyper/commit/dd638b5b34225d2c5ad0bd01de0ecf738f9a0e12) come with are acceptable for this exercise.
* We leverage the [hyper-util](https://github.com/hyperium/hyper-util/blob/master/src/client/legacy/client.rs) crate and delegate connection pooling to it so that we don't have to manage connection re-use for requests to the same `host` and `port`.
* When measuring performance (response time), the _end_ is after the full response body has been streamed.
  * The reason for this decision is that we don't want to prematurely declare a service-under-test as fast when streaming may not be.
* Using an [Unbounded mpsc](https://docs.rs/tokio/latest/tokio/sync/mpsc/index.html) is acceptable because the total calls is known.
* An error is anything with the status_code in the 5XX error range.
* The `GET` HTTP verb is used exclusively.

### Running the HTTP/2 Load generator
* Run `cargo build`
* This binary can accept command line args necessary for the tests:
  ```bash
   # From the target/debug directory, using the binary
   ./toy-loadgen -h
    Usage: toy-loadgen --rate <RATE> --total <TOTAL> <ADDRESS>

    Arguments:
    <ADDRESS>  Address of the form <endpoint>:<port>. Example: nghttp2.org:80

    Options:
    -r, --rate <RATE>    Fixed call rate (per second)
    -t, --total <TOTAL>  Maximum number of requests
    -h, --help           Print help
  ```

* Example command with params:
  ```bash
   # From the target/debug directory, using the binary
   # While developing, at the root directory can replace with cargo run  --rate <rate> --total <total> <address>
  ./toy-loadgen --rate 10 --total 100 localhost:8080
  ```

### How to set up a local HTTP/2 enabled service.
The load-generator is of interest but while developing the load generator, it is useful to have a service to test against.

[podinfo](https://github.com/stefanprodan/podinfo/blob/master/charts/podinfo/values.yaml) is a very useful Golang scaffold service with various bells and whistles that can run in a Docker/Kubernetes runtime. It supports the `h2c` setting (which means HTTP2 without TLS) and is used as a test service for this exercise. Minimal setup:
  ```bash
  ### Install the working service directly in a local `kind` cluster:
  kind create cluster
  kubectl create ns test
  helm install podinfo-h2c-enabled -n test --set h2c.enabled=true podinfo/podinfo
  kubectl -n test port-forward deploy/podinfo-h2c-enabled 8080:9898
  ```
  * Following the podinfo service setup above - sanity check with basic `curl` or browser
    ```bash
    curl --http2 http://localhost:8080/
    ```
  * Should return something similar to:
      ```bash
      {
         "hostname": "podinfo-h2c-enabled-564b976766-rgwvk",
         "version": "6.7.0",
         ...
       }%     
      ``` 
  * Metrics are also available at `http://localhost:8080/metrics`

### Design Overview
#### CLI
* The [clap](https://crates.io/crates/clap) crate does the heavy lifting of setting up the CLI boilerplate.
* The default `rate` and `total` is 1 rps and 1 call.

#### Load testing using Hyper
* We rely on the `hyper-util` crate to help set up the underlying TCP connection and manage connection pooling, using whatever defaults it has. However, we do set idle timeouts.
* Each batch of tasks are spawned at 1-second intervals. The batch size, is equivalent to our user-specified `rate`.
* Tokio's [tick](https://docs.rs/tokio/latest/tokio/time/struct.Interval.html#method.tick) capabilities help set the 1-second pace.
* A mutex wraps our `total` limit of allowed calls that each task decrements.
* Signaling the end of load gen (i.e. `total` calls made) is done using a tokio mpsc channel, and results themselves are _also_ sent to their distinct channels.

#### Results processing
* For two types of results (errors and durations), distinct, unbounded mpsc channels are sufficient. Senders do not block.
* However, result capturing and processing only happens _after all_ calls are made. This is because messages are buffered until the end of the run.
* Data is stored in vectors.
#### Future considerations
* For timeouts that stall the entire test run, we need to bound how long we're willing to wait for results as our program indefinitely waits for results in these scenarios.
* TLS support would be a good addition.
* The loadgen tool needs testing itself and better logging and display of progress.
* Errors from the hyper-util crate need to be carefully wrapped as they currently clobber the output.
* It would be more useful if more percentiles are displayed to showcase the latency profile.
* An upper bound of total calls and a re-work of the result processing would also be necessary improvements.
* The ability to configure (tokio) runtime workers may also help with tuning the loadgenerator for different load profiles i.e. (rps of 10K, 100K)

### References
* [Hyper_Util docs](https://docs.rs/hyper-util/0.1.5/hyper_util/client/legacy/struct.Client.html#method.request)
* [Rust lang forum](https://users.rust-lang.org/t/http2-client-with-hyper/109617/4) as many features of hyper-1.x for HTTP2 are not documented [here](https://hyper.rs/guides/1/)
* [docs rs for tokio and hyper](https://docs.rs/)
* [hyper examples](https://github.com/hyperium/hyper/blob/master/examples/client.rs) - however, these cater for HTTP1.1

### Stretch goal - how to measure average in-flight requests
* I did not have time to implement this.
* On the client side, I interpret an `in flight` request to mean
  * We're either in between streaming over our request, or waiting for the response to be fully streamed back.
  * That means an HTTP/2 stream is _still_ open with data transfer (potentially) occurring.
* I am not sure if it's possible to hook a tracing library to such events to expose.
* Theoretically, one could also take the _average_ response time (so not p50) and multiply by the rate but not sure if that's acceptable :) Curious to hear other ideas.