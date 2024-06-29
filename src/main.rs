use clap::Parser;

#[derive(Parser, Debug)]
#[command( about, long_about = None)]
struct TestParams {
    /// Fixed call rate (per second)
    #[arg(short, long)]
    rate: u32,

    /// Maximum number of requests
    #[arg(short, long)]
    total: u32,

    /// Address of the form <endpoint>:<port>
    address: String
}

fn main() {
    let args = TestParams::parse();
    println!("Rate is {}", args.rate);
    println!("Total is {}", args.total);
    println!("Address is {}", args.address);
}
