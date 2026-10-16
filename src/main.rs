mod agent;
mod desktop;
mod http;
mod i18n;
mod paths;
mod session;
mod update;
mod ui;
mod vault;

use agent::{cli, mcp};
use i18n::tr;
use ui::app;
use vault::config::Config;

fn main() {
    // Read before any text is shown, so the CLI and MCP speak the same language.
    i18n::init(Config::load().language.as_deref());
    // std::env::args() aborts on an argument that is not UTF-8.
    let Ok(args) = std::env::args_os().skip(1).map(|a| a.into_string()).collect::<Result<Vec<String>, _>>() else {
        println!("{{\n  \"error\": \"{}\",\n  \"ok\": false\n}}", tr("visor no admite rutas con caracteres que no son UTF-8.", "visor does not accept paths that are not valid UTF-8."));
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
