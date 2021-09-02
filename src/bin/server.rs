use ss::server::Server;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
struct ServerOpt {
    #[structopt(default_value = "127.0.0.1:8080", long)]
    listen_addr: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let opt = ServerOpt::from_args();
    println!("server listen on {}", opt.listen_addr);
    let client = Server::new(opt.listen_addr);
    client.run().await
}
