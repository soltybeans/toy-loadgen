## HTTP/2 Load Generator

This is currently work in progress

### TODO
* refactoring into different files/modules
* error handling


### Assumptions
* The service-under-test is http2 enabled and can accept HTTP2 over HTTP i.e. _without_ TLS  
  * HTTP/2 without TLS is uncommon but possible and is used for the scope of this exercise
* One TCP Connection can be re-used for multiple requests to the same `host` and `port`. 
  * As such, this load generator does not simulate concurrent, but different users.
* When measuring performance (duration of requests), the _end_ is after the full response body has been streamed.
  * The reason for this decision is that we don't want to prematurely declare a service-under-test as fast when streaming may not be.
* Using an [Unbounded mpsc](https://docs.rs/tokio/latest/tokio/sync/mpsc/index.html) is acceptable because the total calls is known.
* An error is anything with the status_code in the 5XX error range. 

### How to set up a local HTTP/2 enabled service.
#### HTTP/2 target service
The load-generator is of interest but while developing the load generator, it is useful to have a service to test against. There are two useful endpoints that can serve http2
* `nghttpbin.org:80` is the http/2 equivalent of `httpbin.org`
* `podinfo` is a very useful Golang scaffold service with various bells and whistles that can run in a Docker/Kubernetes runtime. It supports the `h2c` setting (which means HTTP2 without TLS) and is used as a test service for this exercise. Minimal setup:
  ```bash
  ### Install the working service directly in a local `kind` cluster:
  kind create cluster
  kubectl create ns test --dry-run 
  helm install podinfo-h2c-enabled -n test --set h2c.enabled=true podinfo/podinfo
  kubectl -n test port-forward deploy/podinfo-h2c-enabled 8080:9898
  ```
  * Following the podinfo service setup above - sanity check with basic `curl` or browser
    ```bash
    curl --http2 http://localhost:8080/
    ```
  * Should return something similar:
      ```bash
      {
         "hostname": "podinfo-h2c-enabled-564b976766-rgwvk",
         "version": "6.7.0",
         ...
       }%     
      ``` 
#### HTTP/2 Load generator
* This binary can accept command line args necessary for the tests:
  ```bash
   # From the target/debug directory, using the binary
   # While developing, at the root directory can replace with cargo run  --rate <rate> --total <total> <address>
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
  
### Design Overview
* TODO
* Define scope of task, result aggregation (only generate results at end). Locking counter

### Future considerations
* TODO
