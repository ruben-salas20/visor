mod agent;
mod http;
mod paths;
mod ui;
mod vault;

use agent::{cli, mcp};
use ui::app;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = match args.first().map(String::as_str) {
        None => app::launch(),
        Some("help" | "--help" | "-h") => {
            print!("{}", include_str!("agent/help.txt"));
            0
        }
        Some("mcp") => mcp::run(),
        Some(_) => cli::run(&args),
    };
    std::process::exit(code);
}
