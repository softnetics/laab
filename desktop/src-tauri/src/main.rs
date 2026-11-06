// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("mcp") {
        // An AI app's MCP connection, piped to the running LAAB.
        std::process::exit(laab_lib::mcp_bridge(&args[1..]));
    }
    laab_lib::run()
}
