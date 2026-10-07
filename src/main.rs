mod annot;
mod app;
mod bar;
mod cli;
mod files;
mod http;
mod links;
mod markdown;
mod panes;
mod paths;
mod server;
mod sidebar;
mod theme;
mod vault;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = match args.first().map(String::as_str) {
        None => app::launch(),
        Some("help" | "--help" | "-h") => {
            print!("{}", include_str!("help.txt"));
            0
        }
        Some(_) => cli::run(&args),
    };
    std::process::exit(code);
}
