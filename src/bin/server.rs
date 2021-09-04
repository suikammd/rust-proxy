use ss::server::Server;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
struct ServerOpt {
    #[structopt(default_value = "127.0.0.1:8080")]
    listen_addr: String,
    #[structopt(default_value = "fullchain.pem")]
    cert_pem_path: String,
    #[structopt(default_value = "private.pem")]
    cert_key_path: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let opt = ServerOpt::from_args();
    println!("server listen on {}", opt.listen_addr);
    let server = Server::new(opt.listen_addr, opt.cert_pem_path, opt.cert_key_path)?;
    server.run().await
}
