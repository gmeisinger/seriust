mod app;
mod serial;
mod ui;

use clap::Parser;
use std::io;

use app::App;

#[derive(Parser, Debug, Clone)]
#[command(name = "seriust", about = "TUI serial monitor")]
pub struct Args {
    /// Serial port path
    #[arg(short, long)]
    port: Option<String>,

    /// Baud rate
    #[arg(short, long, default_value_t = 115200)]
    baud: u32,

    /// Data bits
    #[arg(long, default_value_t = 8)]
    data_bits: u8,

    /// Stop bits
    #[arg(long, default_value_t = 1)]
    stop_bits: u8,

    /// Parity
    #[arg(long, default_value = "none")]
    parity: String,

    /// Flow control
    #[arg(long, default_value = "none")]
    flow_control: String,
}

fn main() -> io::Result<()> {
    let args: Args = Parser::parse();
    ratatui::run(|terminal| App::new(args.clone()).run(terminal))
}
