//! `LAAB mcp`: what an AI app runs to reach LAAB. It pipes stdin and stdout
//! to the socket the running app listens on (`mcp.rs`), opening LAAB in the
//! background first if it isn't running.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const WAIT: Duration = Duration::from_secs(30);

pub fn run(args: &[String]) -> i32 {
    let demo = args.iter().any(|a| a == "--demo") || std::env::var_os("LAAB_DEMO").is_some();
    let socket = match crate::paths::Paths::root_for(demo) {
        Ok(root) => crate::paths::mcp_socket(&root),
        Err(e) => {
            eprintln!("LAAB: {e}");
            return 1;
        }
    };
    let mut input = BufReader::new(std::io::stdin());
    // Connect on the first message, so an error can answer it.
    let mut first = String::new();
    match input.read_line(&mut first) {
        Ok(0) | Err(_) => return 0,
        Ok(_) => {}
    }
    let stream = match connect(&socket) {
        Ok(s) => s,
        Err(message) => {
            eprintln!("LAAB: {message}");
            let id = serde_json::from_str::<serde_json::Value>(&first)
                .ok()
                .and_then(|v| v.get("id").cloned())
                .unwrap_or(serde_json::Value::Null);
            let reply = serde_json::json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32000, "message": message}});
            let mut out = std::io::stdout();
            let _ = writeln!(out, "{reply}");
            let _ = out.flush();
            return 1;
        }
    };
    let mut to_app = match stream.try_clone() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("LAAB: {e}");
            return 1;
        }
    };
    std::thread::spawn(move || {
        if to_app.write_all(first.as_bytes()).is_ok() {
            let _ = std::io::copy(&mut input, &mut to_app);
        }
        let _ = to_app.shutdown(std::net::Shutdown::Write);
    });
    let mut from_app = stream;
    let mut out = std::io::stdout();
    let _ = std::io::copy(&mut from_app, &mut out);
    let _ = out.flush();
    0
}

fn connect(socket: &Path) -> Result<UnixStream, String> {
    if let Ok(s) = UnixStream::connect(socket) {
        return Ok(s);
    }
    let launched = bundle().is_some_and(|app| {
        std::process::Command::new("/usr/bin/open")
            .arg("-g")
            .arg(&app)
            .status()
            .is_ok_and(|s| s.success())
    });
    if launched {
        let start = Instant::now();
        while start.elapsed() < WAIT {
            std::thread::sleep(Duration::from_millis(300));
            if let Ok(s) = UnixStream::connect(socket) {
                return Ok(s);
            }
        }
    }
    Err("เชื่อมต่อ LAAB ไม่ได้: เปิด LAAB แล้วเปิด MCP ที่เมนู ⋯ > MCP (Can't reach LAAB: open it and turn on MCP in ⋯ > MCP.)".into())
}

/// `…/LAAB.app` when running from inside an app bundle.
fn bundle() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let macos = exe.parent()?;
    let contents = macos.parent()?;
    let app = contents.parent()?;
    (macos.ends_with("Contents/MacOS") && app.extension().is_some_and(|e| e == "app")).then(|| app.to_path_buf())
}
