pub mod dump;

use std::{env, net::TcpStream, sync::Mutex};

use clap::Parser;
use tracing::{error, info, Level};

#[derive(Debug, Parser)]
pub struct Arguments {
    #[arg(short, long, default_value = "0.0")]
    pub delay: f32,
}

#[ctor::ctor]
fn ctor() {
    let Ok(stream) = TcpStream::connect("127.0.0.1:8996") else {
        return;
    };

    tracing_subscriber::fmt().with_max_level(Level::DEBUG).with_writer(Mutex::new(stream)).init();

    info!("Connected to injector process");

    if let Err(err) = fallible() {
        error!("Encountered error in execution: {err}");
    }
}

fn fallible() -> anyhow::Result<()> {
    let tiny_pinch_arguments = env::var("TINY_PINCH_ARGUMENTS")?;

    let words = shell_words::split(&tiny_pinch_arguments)?;

    let arguments = Arguments::parse_from(words);

    dump::operate(arguments)
}
