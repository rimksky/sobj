use anyhow::{anyhow, Context, Result};
use clap::{Parser, Subcommand};
use futures_util::{StreamExt, TryStreamExt};
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::header::{AUTHORIZATION, CONTENT_LENGTH, CONTENT_TYPE};
use reqwest::Certificate;
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, time::Duration};
use tokio::{fs, io::AsyncWriteExt};
use tokio_util::io::ReaderStream;

#[derive(Parser)]
#[command(name = "sobj", version, about = "Simple Object Storage CLI (v0.3)")]
struct Cli {
    /// Config path (default: sobj.json next to this executable)
    #[arg(long)]
    config: Option<PathBuf>,

    /// Root CA certificate PEM file (overrides config)
    #[arg(long)]
    ca_cert: Option<PathBuf>,

    /// Disable TLS certificate verification (overrides config)
    #[arg(long)]
    insecure: bool,

    #[command(subcommand)]
    cmd: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Put {
        local: PathBuf,
        key: String,
        #[arg(long)]
        content_type: Option<String>,
    },
    Get {
        key: String,
        local: PathBuf,
    },
    Ls {
        #[arg(long)]
        prefix: Option<String>,
        #[arg(long)]
        delimiter: Option<String>,
        #[arg(long)]
        limit: Option<usize>,
        #[arg(long)]
        cursor: Option<String>,
        #[arg(long)]
        json: bool,
    },
    Head { key: String },
    Rm { key: String },

    /// Copy object: src -> dst
    Cp {
        src: String,
        dst: String,
        #[arg(long)]
        no_overwrite: bool,
    },

    /// Move object: src -> dst
    Mv {
        src: String,
        dst: String,
        #[arg(long)]
        no_overwrite: bool,
    },

    /// Health check
    Health,
}

#[derive(Deserialize, Clone)]
struct CliConfig {
    endpoint: Option<String>,
    token: Option<String>,
    timeout_secs: Option<u64>,
    tls_ca_cert_pem_path: Option<String>,
    tls_insecure_skip_verify: Option<bool>,
}

fn default_config_path() -> Result<PathBuf> {
    let exe = std::env::current_exe().context("current_exe")?;
    let dir = exe.parent().ok_or_else(|| anyhow!("failed to get exe dir"))?;
    Ok(dir.join("sobj.json"))
}

fn load_config(path: Option<PathBuf>) -> Result<CliConfig> {
    let path = path.unwrap_or(default_config_path()?);
    let txt = std::fs::read_to_string(&path).with_context(|| format!("read config {:?}", path))?;
    let cfg: CliConfig = serde_json::from_str(&txt).context("parse json")?;
    Ok(cfg)
}

#[derive(Deserialize)]
struct ListResp {
    #[allow(dead_code)]
    prefix: String,
    #[allow(dead_code)]
    delimiter: Option<String>,
    items: Vec<ListItem>,
    common_prefixes: Vec<String>,
    next_cursor: Option<String>,
}
#[derive(Deserialize)]
struct ListItem {
    key: String,
    size: u64,
    last_modified: Option<String>,
}

#[derive(Serialize)]
struct CopyMoveReq {
    src: String,
    dst: String,
    overwrite: bool,
}
#[derive(Deserialize)]
struct CopyMoveResp {
    src: String,
    dst: String,
    size: u64,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let cfg = load_config(cli.config.clone())?;

    let endpoint = cfg
        .endpoint
        .unwrap_or_else(|| "http://127.0.0.1:9999".to_string())
        .trim_end_matches('/')
        .to_string();

    let token = cfg.token.unwrap_or_default();
    let timeout = Duration::from_secs(cfg.timeout_secs.unwrap_or(3600));

    let mut builder = reqwest::Client::builder().timeout(timeout);

    let insecure = if cli.insecure { true } else { cfg.tls_insecure_skip_verify.unwrap_or(false) };
    if insecure {
        builder = builder.danger_accept_invalid_certs(true);
    }

    let ca_path = cli.ca_cert.clone().or_else(|| cfg.tls_ca_cert_pem_path.clone().map(PathBuf::from));
    if let Some(p) = ca_path {
        let pem = std::fs::read(&p).with_context(|| format!("read ca cert {:?}", p))?;
        let cert = Certificate::from_pem(&pem).context("invalid PEM cert")?;
        builder = builder.add_root_certificate(cert);
    }

    let client = builder.build()?;

    match cli.cmd {
        Commands::Put { local, key, content_type } => {
            put_cmd(&client, &endpoint, &token, local, key, content_type).await?;
        }
        Commands::Get { key, local } => {
            get_cmd(&client, &endpoint, &token, key, local).await?;
        }
        Commands::Ls { prefix, delimiter, limit, cursor, json } => {
            ls_cmd(&client, &endpoint, &token, prefix, delimiter, limit, cursor, json).await?;
        }
        Commands::Head { key } => head_cmd(&client, &endpoint, &token, key).await?,
        Commands::Rm { key } => rm_cmd(&client, &endpoint, &token, key).await?,
        Commands::Cp { src, dst, no_overwrite } => {
            copy_move_cmd(&client, &endpoint, &token, true, src, dst, !no_overwrite).await?;
        }
        Commands::Mv { src, dst, no_overwrite } => {
            copy_move_cmd(&client, &endpoint, &token, false, src, dst, !no_overwrite).await?;
        }
        Commands::Health => health_cmd(&client, &endpoint).await?,
    }

    Ok(())
}

fn maybe_auth(req: reqwest::RequestBuilder, token: &str) -> reqwest::RequestBuilder {
    if token.trim().is_empty() { req } else { req.header(AUTHORIZATION, token) }
}

fn progress_bar(total: u64, label: &str) -> ProgressBar {
    let pb = ProgressBar::new(total);
    pb.set_style(
        ProgressStyle::with_template("{msg:9} [{bar:40}] {bytes}/{total_bytes} ({bytes_per_sec}, {eta})")
            .unwrap(),
    );
    pb.set_message(label.to_string());
    pb
}

fn spinner(label: &str) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_style(ProgressStyle::with_template("{msg:9} {spinner} {bytes}").unwrap());
    pb.set_message(label.to_string());
    pb.enable_steady_tick(Duration::from_millis(80));
    pb
}

fn encode_key(key: &str) -> String {
    let k = key.trim_start_matches('/');
    k.split('/')
        .map(|seg| urlencoding::encode(seg).into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

async fn put_cmd(
    client: &reqwest::Client,
    endpoint: &str,
    token: &str,
    local: PathBuf,
    key: String,
    content_type: Option<String>,
) -> Result<()> {
    let url = format!("{}/{}", endpoint, encode_key(&key));
    let meta = fs::metadata(&local).await.with_context(|| format!("metadata {:?}", local))?;
    let total = meta.len();

    let ct = content_type.unwrap_or_else(|| {
        mime_guess::from_path(&local).first_or_octet_stream().to_string()
    });

    let pb = progress_bar(total, "upload");
    let pb2 = pb.clone();

    let file = fs::File::open(&local).await.with_context(|| format!("open {:?}", local))?;
    let stream = ReaderStream::new(file).map_ok(move |bytes| {
        pb2.inc(bytes.len() as u64);
        bytes
    });
    let body = reqwest::Body::wrap_stream(stream);

    let res = maybe_auth(client.put(url), token)
        .header(CONTENT_TYPE, ct)
        .header(CONTENT_LENGTH, total)
        .body(body)
        .send()
        .await
        .context("request failed")?;

    let status = res.status();
    let txt = res.text().await.unwrap_or_default();
    pb.finish_and_clear();

    if !status.is_success() {
        return Err(anyhow!("PUT failed: {} {}", status, txt));
    }

    println!("{}", txt);
    Ok(())
}

async fn get_cmd(
    client: &reqwest::Client,
    endpoint: &str,
    token: &str,
    key: String,
    local: PathBuf,
) -> Result<()> {
    let url = format!("{}/{}", endpoint, encode_key(&key));
    let res = maybe_auth(client.get(url), token)
        .send()
        .await
        .context("request failed")?;

    let status = res.status();
    if !status.is_success() {
        let txt = res.text().await.unwrap_or_default();
        return Err(anyhow!("GET failed: {} {}", status, txt));
    }

    let total = res
        .headers()
        .get(CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);

    let pb = if total > 0 { progress_bar(total, "download") } else { spinner("download") };

    let mut file = fs::File::create(&local).await.with_context(|| format!("create {:?}", local))?;
    let mut stream = res.bytes_stream();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("download chunk")?;
        pb.inc(chunk.len() as u64);
        file.write_all(&chunk).await.context("write file")?;
    }
    let _ = file.flush().await;
    pb.finish_and_clear();
    eprintln!("saved to {:?}", local);
    Ok(())
}

async fn ls_cmd(
    client: &reqwest::Client,
    endpoint: &str,
    token: &str,
    prefix: Option<String>,
    delimiter: Option<String>,
    limit: Option<usize>,
    cursor: Option<String>,
    json_out: bool,
) -> Result<()> {
    let mut url = format!("{}/", endpoint);
    let mut qp: Vec<(String, String)> = vec![];
    if let Some(p) = prefix { qp.push(("prefix".into(), p)); }
    if let Some(d) = delimiter { qp.push(("delimiter".into(), d)); }
    if let Some(l) = limit { qp.push(("limit".into(), l.to_string())); }
    if let Some(c) = cursor { qp.push(("cursor".into(), c)); }

    if !qp.is_empty() {
        url.push('?');
        url.push_str(
            &qp.iter()
                .map(|(k, v)| format!("{}={}", k, urlencoding::encode(v)))
                .collect::<Vec<_>>()
                .join("&"),
        );
    }

    let res = maybe_auth(client.get(url), token)
        .send()
        .await
        .context("request failed")?;

    let status = res.status();
    let txt = res.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(anyhow!("LS failed: {} {}", status, txt));
    }

    if json_out {
        println!("{}", txt);
        return Ok(());
    }

    let parsed: ListResp = serde_json::from_str(&txt).context("parse json")?;

    if !parsed.common_prefixes.is_empty() {
        println!("CommonPrefixes:");
        for p in &parsed.common_prefixes {
            println!("  {}", p);
        }
        println!();
    }

    if !parsed.items.is_empty() {
        println!("Items:");
        for it in &parsed.items {
            let lm = it.last_modified.as_deref().unwrap_or("-");
            println!("  {:>10}  {}  {}", it.size, lm, it.key);
        }
    }

    if let Some(next) = parsed.next_cursor {
        println!();
        println!("next_cursor={}", next);
    }

    Ok(())
}

async fn head_cmd(client: &reqwest::Client, endpoint: &str, token: &str, key: String) -> Result<()> {
    let url = format!("{}/{}", endpoint, encode_key(&key));
    let res = maybe_auth(client.head(url), token).send().await?;

    if res.status() == reqwest::StatusCode::NOT_FOUND {
        println!("NOT FOUND");
        return Ok(());
    }

    let status = res.status();
    if !status.is_success() {
        let txt = res.text().await.unwrap_or_default();
        return Err(anyhow!("HEAD failed: {} {}", status, txt));
    }

    println!("OK");
    for (k, v) in res.headers().iter() {
        println!("{}: {}", k, v.to_str().unwrap_or("<binary>"));
    }
    Ok(())
}

async fn rm_cmd(client: &reqwest::Client, endpoint: &str, token: &str, key: String) -> Result<()> {
    let url = format!("{}/{}", endpoint, encode_key(&key));
    let res = maybe_auth(client.delete(url), token).send().await?;
    let status = res.status();
    let txt = res.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(anyhow!("RM failed: {} {}", status, txt));
    }
    if !txt.trim().is_empty() {
        println!("{}", txt);
    } else {
        println!("deleted {}", key);
    }
    Ok(())
}

async fn copy_move_cmd(
    client: &reqwest::Client,
    endpoint: &str,
    token: &str,
    is_copy: bool,
    src: String,
    dst: String,
    overwrite: bool,
) -> Result<()> {
    let url = if is_copy { format!("{}/_copy", endpoint) } else { format!("{}/_move", endpoint) };

    let req = CopyMoveReq { src, dst, overwrite };

    let res = maybe_auth(client.post(url), token)
        .json(&req)
        .send()
        .await
        .context("request failed")?;

    let status = res.status();
    let txt = res.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(anyhow!("{} failed: {} {}", if is_copy { "CP" } else { "MV" }, status, txt));
    }

    let parsed: CopyMoveResp = serde_json::from_str(&txt).context("parse json")?;
    println!("{} -> {} ({} bytes)", parsed.src, parsed.dst, parsed.size);
    Ok(())
}

async fn health_cmd(client: &reqwest::Client, endpoint: &str) -> Result<()> {
    let url = format!("{}/healthz", endpoint);
    let res = client.get(url).send().await.context("request failed")?;
    let status = res.status();
    let txt = res.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(anyhow!("HEALTH failed: {} {}", status, txt));
    }
    println!("{}", txt);
    Ok(())
}
