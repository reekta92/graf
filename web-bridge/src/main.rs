use pulldown_cmark::{Options, Parser, html as cmark_html};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command;

const PAGE: &str = include_str!("index.html");

#[derive(Debug)]
struct Options_ {
    dir: PathBuf,
    bind: SocketAddr,
    open: bool,
    exclude: Vec<String>,
    with_arrows: bool,
    arrow_color: Option<String>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options = parse_args()?;
    let listener = bind_replacing_graf(options.bind)?;
    let address = listener.local_addr()?;
    let url = format!("http://{address}/");
    eprintln!("graf-web serving {} at {url}", options.dir.display());

    if options.open {
        open_browser(&url);
    }

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => match serve(stream, &options) {
                Ok(true) => break,
                Ok(false) => {}
                Err(error) => eprintln!("graf-web request error: {error}"),
            },
            Err(error) => eprintln!("graf-web connection error: {error}"),
        }
    }
    Ok(())
}

fn parse_args() -> Result<Options_, Box<dyn std::error::Error>> {
    let mut dir = std::env::current_dir()?;
    let mut bind = SocketAddr::from(([127, 0, 0, 1], 8765));
    let mut open = false;
    let mut exclude = Vec::new();
    let mut with_arrows = false;
    let mut arrow_color = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--dir" => dir = PathBuf::from(args.next().ok_or("--dir requires a path")?),
            "--bind" => bind = args.next().ok_or("--bind requires an address")?.parse()?,
            "--port" => {
                let port: u16 = args.next().ok_or("--port requires a port")?.parse()?;
                bind.set_port(port);
            }
            "--open" => open = true,
            "--exclude" | "--ignore-dir" => {
                exclude.push(args.next().ok_or("--exclude requires a glob")?)
            }
            "--with-arrows" => with_arrows = true,
            "--arrow-color" => {
                let value = args.next().ok_or("--arrow-color requires #RRGGBB")?;
                if value.len() != 7
                    || !value.starts_with('#')
                    || !value[1..].bytes().all(|b| b.is_ascii_hexdigit())
                {
                    return Err("--arrow-color must use #RRGGBB format".into());
                }
                arrow_color = Some(value);
            }
            "--help" | "-h" => {
                println!(
                    "graf-web [--dir PATH] [--exclude GLOB]... [--with-arrows] [--arrow-color #RRGGBB] [--bind IP:PORT] [--port PORT] [--open]\n\nStarts an optional local browser view. Default bind: 127.0.0.1:8765."
                );
                std::process::exit(0);
            }
            other => return Err(format!("unknown option: {other}").into()),
        }
    }
    dir = dir.canonicalize()?;
    if !dir.is_dir() {
        return Err("--dir must be a directory".into());
    }
    Ok(Options_ {
        dir,
        bind,
        open,
        exclude,
        with_arrows,
        arrow_color,
    })
}

fn open_browser(url: &str) {
    #[cfg(target_os = "linux")]
    let result = Command::new("xdg-open").arg(url).spawn();
    #[cfg(target_os = "macos")]
    let result = Command::new("open").arg(url).spawn();
    #[cfg(target_os = "windows")]
    let result = Command::new("cmd").args(["/C", "start", "", url]).spawn();
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    let result: Result<std::process::Child, std::io::Error> =
        Err(std::io::Error::other("browser launching is unsupported"));
    if let Err(error) = result {
        eprintln!("Could not open browser automatically: {error}; open {url}");
    }
}

fn bind_replacing_graf(address: SocketAddr) -> Result<TcpListener, Box<dyn std::error::Error>> {
    match TcpListener::bind(address) {
        Ok(listener) => Ok(listener),
        Err(error)
            if error.kind() == std::io::ErrorKind::AddrInUse && address.ip().is_loopback() =>
        {
            if !request_graf_web_shutdown(address) {
                return Err(error.into());
            }
            for _ in 0..30 {
                std::thread::sleep(std::time::Duration::from_millis(100));
                match TcpListener::bind(address) {
                    Ok(listener) => return Ok(listener),
                    Err(retry) if retry.kind() == std::io::ErrorKind::AddrInUse => {}
                    Err(retry) => return Err(retry.into()),
                }
            }
            Err(error.into())
        }
        Err(error) => Err(error.into()),
    }
}

fn request_graf_web_shutdown(address: SocketAddr) -> bool {
    // Confirm the occupant is a graf-web instance by checking for our sentinel header.
    let Ok(mut health) =
        TcpStream::connect_timeout(&address, std::time::Duration::from_millis(300))
    else {
        return false;
    };
    let _ = health.set_read_timeout(Some(std::time::Duration::from_millis(1500)));
    if health
        .write_all(b"GET /health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .is_err()
    {
        return false;
    }
    let mut response = Vec::new();
    if health.take(2048).read_to_end(&mut response).is_err()
        || !response
            .windows(b"X-Graf-Web: graf".len())
            .any(|w| w.eq_ignore_ascii_case(b"X-Graf-Web: graf"))
    {
        return false;
    }

    // Send shutdown and read just the HTTP status line.  Using read_line avoids
    // waiting for EOF (which requires the server to fully drop its socket) and
    // is therefore immune to the broken-pipe race that plagued read_to_end.
    let Ok(mut shutdown) =
        TcpStream::connect_timeout(&address, std::time::Duration::from_millis(300))
    else {
        return false;
    };
    let _ = shutdown.set_read_timeout(Some(std::time::Duration::from_secs(3)));
    if shutdown
        .write_all(b"POST /__graf_shutdown HTTP/1.1\r\nHost: localhost\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
        .is_err()
    {
        return false;
    }
    let mut status = String::new();
    matches!(
        BufReader::new(&mut shutdown).read_line(&mut status),
        Ok(n) if n > 0
    ) && status.starts_with("HTTP/1.1 200")
}

struct HttpRequest {
    method: String,
    path: String,
    query: String,
    body: Vec<u8>,
}

fn parse_http_request(stream: &TcpStream) -> Result<HttpRequest, Box<dyn std::error::Error>> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or("GET").to_string();
    let raw = parts.next().unwrap_or("/").to_string();
    let (path, query) = match raw.find('?') {
        Some(i) => (raw[..i].to_string(), raw[i + 1..].to_string()),
        None => (raw, String::new()),
    };
    let mut content_length = 0usize;
    loop {
        let mut h = String::new();
        reader.read_line(&mut h)?;
        if h.trim().is_empty() { break; }
        let hl = h.trim().to_ascii_lowercase();
        if hl.starts_with("content-length:") {
            content_length = hl[15..].trim().parse().unwrap_or(0);
        }
    }
    let mut body = vec![0u8; content_length.min(1 << 20)];
    if content_length > 0 {
        reader.read_exact(&mut body)?;
    }
    Ok(HttpRequest { method, path, query, body })
}

fn serve(mut stream: TcpStream, options: &Options_) -> Result<bool, Box<dyn std::error::Error>> {
    stream.set_read_timeout(Some(std::time::Duration::from_secs(3)))?;
    let req = parse_http_request(&stream)?;
    let method = req.method.as_str();
    let path = req.path.as_str();
    let query = req.query.as_str();

    if method == "POST" && path == "/__graf_shutdown" {
        if stream.peer_addr()?.ip().is_loopback() {
            respond(
                &mut stream,
                "200 OK",
                "text/plain; charset=utf-8",
                b"stopping",
            )?;
            return Ok(true);
        }
        respond(
            &mut stream,
            "403 Forbidden",
            "text/plain; charset=utf-8",
            b"loopback only",
        )?;
        return Ok(false);
    }

    // POST mutation routes (loopback only)
    if method == "POST" && path != "/__graf_shutdown" {
        if !stream.peer_addr()?.ip().is_loopback() {
            respond(&mut stream, "403 Forbidden", "text/plain; charset=utf-8", b"loopback only")?;
            return Ok(false);
        }
        let v: serde_json::Value = serde_json::from_slice(&req.body).unwrap_or(serde_json::Value::Null);
        let ok_json = b"{\"ok\":true}";
        match path {
            "/file/create" => {
                let rel = v["path"].as_str().unwrap_or("");
                match create_file(rel, options) {
                    Ok(()) => respond(&mut stream, "200 OK", "application/json; charset=utf-8", ok_json)?,
                    Err(e) => {
                        let msg = serde_json::json!({"ok":false,"error":e}).to_string();
                        respond(&mut stream, "400 Bad Request", "application/json; charset=utf-8", msg.as_bytes())?
                    }
                }
            }
            "/folder/create" => {
                let rel = v["path"].as_str().unwrap_or("");
                match create_folder(rel, options) {
                    Ok(()) => respond(&mut stream, "200 OK", "application/json; charset=utf-8", ok_json)?,
                    Err(e) => {
                        let msg = serde_json::json!({"ok":false,"error":e}).to_string();
                        respond(&mut stream, "400 Bad Request", "application/json; charset=utf-8", msg.as_bytes())?
                    }
                }
            }
            "/file/move" => {
                let from = v["from"].as_str().unwrap_or("");
                let to = v["to"].as_str().unwrap_or("");
                match move_file(from, to, options) {
                    Ok(()) => respond(&mut stream, "200 OK", "application/json; charset=utf-8", ok_json)?,
                    Err(e) => {
                        let msg = serde_json::json!({"ok":false,"error":e}).to_string();
                        respond(&mut stream, "400 Bad Request", "application/json; charset=utf-8", msg.as_bytes())?
                    }
                }
            }
            "/file/open" => {
                let rel = v["path"].as_str().unwrap_or("");
                match open_file_system(rel, options) {
                    Ok(()) => respond(&mut stream, "200 OK", "application/json; charset=utf-8", ok_json)?,
                    Err(e) => {
                        let msg = serde_json::json!({"ok":false,"error":e}).to_string();
                        respond(&mut stream, "400 Bad Request", "application/json; charset=utf-8", msg.as_bytes())?
                    }
                }
            }
            "/link/create" => {
                let from = v["from"].as_str().unwrap_or("");
                let to_title = v["to_title"].as_str().unwrap_or("");
                match link_modify("add", from, to_title, options) {
                    Ok(()) => respond(&mut stream, "200 OK", "application/json; charset=utf-8", ok_json)?,
                    Err(e) => {
                        let msg = serde_json::json!({"ok":false,"error":e}).to_string();
                        respond(&mut stream, "400 Bad Request", "application/json; charset=utf-8", msg.as_bytes())?
                    }
                }
            }
            "/link/delete" => {
                let from = v["from"].as_str().unwrap_or("");
                let to_title = v["to_title"].as_str().unwrap_or("");
                match link_modify("remove", from, to_title, options) {
                    Ok(()) => respond(&mut stream, "200 OK", "application/json; charset=utf-8", ok_json)?,
                    Err(e) => {
                        let msg = serde_json::json!({"ok":false,"error":e}).to_string();
                        respond(&mut stream, "400 Bad Request", "application/json; charset=utf-8", msg.as_bytes())?
                    }
                }
            }
            _ => respond(&mut stream, "404 Not Found", "text/plain; charset=utf-8", b"not found")?,
        }
        return Ok(false);
    }

    if method != "GET" {
        respond(
            &mut stream,
            "405 Method Not Allowed",
            "text/plain; charset=utf-8",
            b"GET only",
        )?;
        return Ok(false);
    }
    match path {
        "/" | "/index.html" => respond(
            &mut stream,
            "200 OK",
            "text/html; charset=utf-8",
            PAGE.as_bytes(),
        )?,
        "/graph.json" => {
            let body = graph_json(options)?;
            respond(
                &mut stream,
                "200 OK",
                "application/json; charset=utf-8",
                body.as_bytes(),
            )?
        }
        "/file" => {
            let params = parse_query(query);
            match params.get("path").and_then(|p| serve_file_json(p, options)) {
                Some(body) => respond(
                    &mut stream,
                    "200 OK",
                    "application/json; charset=utf-8",
                    body.as_bytes(),
                )?,
                None => respond(
                    &mut stream,
                    "404 Not Found",
                    "text/plain; charset=utf-8",
                    b"file not found",
                )?,
            }
        }
        "/tree" => {
            let body = tree_json(options)?;
            respond(&mut stream, "200 OK", "application/json; charset=utf-8", body.as_bytes())?
        }
        "/health" => respond(&mut stream, "200 OK", "text/plain; charset=utf-8", b"ok")?,
        _ => respond(
            &mut stream,
            "404 Not Found",
            "text/plain; charset=utf-8",
            b"not found",
        )?,
    }
    Ok(false)
}

fn parse_query(query: &str) -> HashMap<String, String> {
    query
        .split('&')
        .filter_map(|pair| {
            let mut it = pair.splitn(2, '=');
            let k = it.next()?;
            let v = it.next().unwrap_or("");
            if k.is_empty() {
                return None;
            }
            Some((url_decode(k), url_decode(v)))
        })
        .collect()
}

fn url_decode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut bytes = s.bytes();
    while let Some(b) = bytes.next() {
        if b == b'%' {
            let h1 = bytes.next().and_then(|c| (c as char).to_digit(16));
            let h2 = bytes.next().and_then(|c| (c as char).to_digit(16));
            if let (Some(h1), Some(h2)) = (h1, h2) {
                out.push((((h1 << 4) | h2) as u8) as char);
            }
        } else if b == b'+' {
            out.push(' ');
        } else {
            out.push(b as char);
        }
    }
    out
}

fn serve_file_json(relative: &str, options: &Options_) -> Option<String> {
    let canonical = safe_path(&options.dir, relative)?;
    let source = std::fs::read_to_string(&canonical).ok()?;
    let html = markdown_to_html(&source);
    Some(
        serde_json::json!({
            "path": relative,
            "source": source,
            "html": html,
        })
        .to_string(),
    )
}

fn safe_path(dir: &Path, relative: &str) -> Option<PathBuf> {
    // Strip leading slashes; reject absolute paths and plain ..
    let rel = relative.trim_start_matches('/');
    if rel.is_empty() {
        return None;
    }
    let candidate = dir.join(rel);
    let canonical = candidate.canonicalize().ok()?;
    if canonical.starts_with(dir) {
        Some(canonical)
    } else {
        None
    }
}

fn safe_new_path(dir: &Path, relative: &str) -> Option<PathBuf> {
    let rel = relative.trim_start_matches('/');
    if rel.is_empty() { return None; }
    if rel.split('/').any(|c| c == "..") { return None; }
    Some(dir.join(rel))
}

fn tree_json(options: &Options_) -> Result<String, Box<dyn std::error::Error>> {
    let root = build_tree_node(&options.dir, &options.dir, &options.exclude);
    let folders = collect_folders(&root);
    Ok(serde_json::json!({"root": root, "folders": folders}).to_string())
}

fn build_tree_node(dir: &Path, base: &Path, exclude: &[String]) -> serde_json::Value {
    let rel = dir.strip_prefix(base).unwrap_or(Path::new(""));
    let name = if rel.as_os_str().is_empty() {
        ".".to_string()
    } else {
        dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
    };
    let path_str = rel.to_string_lossy().replace('\\', "/");
    let mut sub_dirs = Vec::new();
    let mut files = Vec::new();
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .into_iter().flatten().filter_map(|e| e.ok()).collect();
    entries.sort_by_key(|e| e.file_name().to_string_lossy().to_lowercase());
    for entry in &entries {
        let p = entry.path();
        let fname = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        if fname.starts_with('.') { continue; }
        let rel_p = p.strip_prefix(base).unwrap_or(&p);
        let rel_str = rel_p.to_string_lossy().replace('\\', "/");
        if is_excluded_from_tree(&rel_str, exclude) { continue; }
        if p.is_dir() {
            sub_dirs.push(build_tree_node(&p, base, exclude));
        } else {
            let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
            if ext == "md" || ext == "mdx" {
                files.push(serde_json::json!({"name": fname, "path": rel_str, "is_dir": false}));
            }
        }
    }
    let mut children = sub_dirs;
    children.extend(files);
    serde_json::json!({"name": name, "path": path_str, "is_dir": true, "children": children})
}

fn collect_folders(node: &serde_json::Value) -> Vec<String> {
    let mut result = Vec::new();
    if node["is_dir"].as_bool().unwrap_or(false) {
        result.push(node["path"].as_str().unwrap_or("").to_string());
        if let Some(children) = node["children"].as_array() {
            for child in children {
                if child["is_dir"].as_bool().unwrap_or(false) {
                    result.extend(collect_folders(child));
                }
            }
        }
    }
    result
}

fn is_excluded_from_tree(rel: &str, patterns: &[String]) -> bool {
    patterns.iter().any(|p| {
        let stem = p.trim_end_matches("/**").trim_end_matches("/*");
        rel == stem || rel.starts_with(&format!("{stem}/"))
    })
}

fn create_file(rel_path: &str, options: &Options_) -> Result<(), String> {
    let rel_path = rel_path.trim_start_matches('/');
    let ext = Path::new(rel_path).extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if ext != "md" && ext != "mdx" {
        return Err("only .md and .mdx files are allowed".into());
    }
    let full = safe_new_path(&options.dir, rel_path)
        .ok_or_else(|| "invalid path".to_string())?;
    if full.exists() { return Err("file already exists".into()); }
    let stem = Path::new(rel_path).file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "New Note".to_string());
    std::fs::write(&full, format!("# {stem}\n")).map_err(|e| e.to_string())
}

fn create_folder(rel_path: &str, options: &Options_) -> Result<(), String> {
    let rel_path = rel_path.trim_start_matches('/');
    if rel_path.split('/').any(|c| c == "..") { return Err("invalid path".into()); }
    if rel_path.is_empty() { return Err("empty path".into()); }
    std::fs::create_dir_all(options.dir.join(rel_path)).map_err(|e| e.to_string())
}

fn move_file(from: &str, to: &str, options: &Options_) -> Result<(), String> {
    let src = safe_path(&options.dir, from).ok_or_else(|| "source not found".to_string())?;
    if !src.is_file() { return Err("source is not a file".into()); }
    let dst = safe_new_path(&options.dir, to).ok_or_else(|| "invalid destination path".to_string())?;
    if dst.exists() { return Err("destination already exists".into()); }
    std::fs::rename(&src, &dst).map_err(|e| e.to_string())?;

    // Rewrite interlinks in all other notes if stem changed.
    let old_stem = std::path::Path::new(from)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let new_stem = std::path::Path::new(to)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    if !old_stem.eq_ignore_ascii_case(&new_stem) {
        let all_files = graf::scan_markdown_files(&options.dir, &options.exclude, 0);
        for fd in &all_files {
            let abs = options.dir.join(&fd.relative_path);
            if abs == dst {
                continue; // skip the renamed file itself
            }
            if let Ok(content) = std::fs::read_to_string(&abs) {
                let updated = graf::rename_wikilinks(&content, &old_stem, &new_stem);
                if updated != content {
                    let _ = std::fs::write(&abs, updated);
                }
            }
        }
    }
    Ok(())
}

fn open_file_system(rel_path: &str, options: &Options_) -> Result<(), String> {
    let abs = safe_path(&options.dir, rel_path)
        .ok_or_else(|| "file not found".to_string())?;
    #[cfg(target_os = "linux")]
    { let _ = std::process::Command::new("xdg-open").arg(&abs).spawn(); }
    #[cfg(target_os = "macos")]
    { let _ = std::process::Command::new("open").arg(&abs).spawn(); }
    #[cfg(target_os = "windows")]
    { let _ = std::process::Command::new("cmd").args(["/C", "start", ""]).arg(&abs).spawn(); }
    Ok(())
}

fn link_modify(action: &str, from: &str, to_title: &str, options: &Options_) -> Result<(), String> {
    if from.is_empty() || to_title.is_empty() {
        return Err("missing parameters".into());
    }
    let abs = safe_path(&options.dir, from)
        .ok_or_else(|| "file not found".to_string())?;
    let content = std::fs::read_to_string(&abs).map_err(|e| e.to_string())?;
    let updated = match action {
        "add" => graf::add_wikilink(&content, to_title),
        "remove" => graf::remove_wikilink(&content, to_title),
        _ => return Err("unknown action".into()),
    };
    if updated != content {
        std::fs::write(&abs, updated).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn markdown_to_html(source: &str) -> String {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TASKLISTS);
    opts.insert(Options::ENABLE_WIKILINKS);
    let parser = Parser::new_ext(source, opts);
    let mut html_out = String::new();
    cmark_html::push_html(&mut html_out, parser);
    html_out
}

fn graph_json(options: &Options_) -> Result<String, Box<dyn std::error::Error>> {
    let files = graf::scan_markdown_files(&options.dir, &options.exclude, 0);
    let wiki = graf::resolve_wikilinks(&files, &[]);
    let markdown = graf::resolve_markdown_links(&files, &[]);
    let title_by_path: HashMap<String, String> = files
        .iter()
        .map(|file| (file.relative_path.to_lowercase(), file.title.clone()))
        .collect();
    let nodes: Vec<_> = files
        .iter()
        .map(|file| {
            let folder = std::path::Path::new(&file.relative_path)
                .parent()
                .and_then(|path| path.file_name())
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_default();
            serde_json::json!({"id":file.relative_path,"title":file.title,"folder":folder,"tags":file.tags})
        })
        .collect();
    let mut edges = Vec::new();
    for file in &files {
        for (kind, targets) in [
            ("wikilink", wiki.get(&file.relative_path)),
            ("markdown", markdown.get(&file.relative_path)),
        ] {
            if let Some(targets) = targets {
                for target in targets {
                    if let Some(title) = title_by_path.get(&target.to_lowercase()) {
                        edges.push(serde_json::json!({"source":file.relative_path,"target":target,"target_title":title,"kind":kind}));
                    }
                }
            }
        }
    }
    Ok(serde_json::json!({"nodes":nodes,"edges":edges,"with_arrows":options.with_arrows,"arrow_color":options.arrow_color}).to_string())
}

fn respond(
    stream: &mut TcpStream,
    status: &str,
    content_type: &str,
    body: &[u8],
) -> Result<(), Box<dyn std::error::Error>> {
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\nX-Graf-Web: graf\r\nX-Content-Type-Options: nosniff\r\n\r\n",
        body.len()
    )?;
    stream.write_all(body)?;
    stream.flush()?;
    Ok(())
}
