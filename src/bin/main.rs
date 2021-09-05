#[macro_use]
extern crate log;

use std::str::FromStr;

use ss::{client::Client, server::Server};
use structopt::StructOpt;

// any error type implementing Display is acceptable.
type ParseError = &'static str;

impl FromStr for Mode {
    type Err = ParseError;
    fn from_str(day: &str) -> Result<Self, Self::Err> {
        match day {
            "server" => Ok(Mode::Server),
            "client" => Ok(Mode::Client),
            _ => Err("Could not parse a day"),
        }
    }
}

#[derive(Debug)]
enum Mode {
    Server,
    Client,
}

#[derive(Debug, StructOpt)]
struct Opt {
    #[structopt(short = "l", long = "listen_addr", default_value = "127.0.0.1:8080")]
    listen_addr: String,
    #[structopt(short = "f", long = "fullchain", default_value = "fullchain.pem")]
    fullchain_path: String,
    #[structopt(short = "k", long = "private_key", default_value = "private.pem")]
    private_key_path: String,
    #[structopt(short = "p", long = "proxy_addr", default_value = "proxy.com")]
    proxy_addr: String,
    #[structopt(short = "m", long = "mode", default_value = "server")]
    mode: Mode,
    #[structopt(short = "t", long = "authorization", default_value = "")]
    authorization: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    let opt = Opt::from_args();
    match opt.mode {
        Mode::Server => {
            info!("server listen on {}", opt.listen_addr);
            let server = Server::new(opt.listen_addr, opt.fullchain_path, opt.private_key_path, opt.authorization)?;
            server.run().await
        }
        Mode::Client => {
            info!("client listen on {}", opt.listen_addr);
            let client = Client::new(opt.listen_addr, opt.proxy_addr, opt.authorization)?;
            client.run().await
        }
    }
}
