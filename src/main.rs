mod agent;
mod http;
mod paths;
mod session;
mod update;
mod ui;
mod vault;

use agent::{cli, mcp};
use ui::app;

fn main() {
    // std::env::args() aborts on an argument that is not UTF-8.
    let Ok(args) = std::env::args_os().skip(1).map(|a| a.into_string()).collect::<Result<Vec<String>, _>>() else {
        println!("{{\n  \"error\": \"visor no admite rutas con caracteres que no son UTF-8.\",\n  \"ok\": false\n}}");
        std::process::exit(1);
    };
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
