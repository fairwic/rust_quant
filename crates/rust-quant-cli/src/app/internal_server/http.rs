use anyhow::{Context, Result};
use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use super::auth::parse_headers;

const MAX_HEADER_BYTES: usize = 16 * 1024;
const MAX_BODY_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone)]
pub struct InternalHttpJsonResponse {
    pub status_code: u16,
    pub body: Value,
}

pub(super) fn json_response(status_code: u16, body: Value) -> InternalHttpJsonResponse {
    InternalHttpJsonResponse { status_code, body }
}

pub(super) fn route_path(path: &str) -> &str {
    path.split_once('?').map(|(path, _)| path).unwrap_or(path)
}

pub(super) fn required_query_param(query: &str, names: &[&str]) -> Result<String, String> {
    let value = query_param(query, names)
        .ok_or_else(|| format!("{} is required", names.first().copied().unwrap_or("param")))?;
    if value.trim().is_empty() {
        return Err(format!(
            "{} is required",
            names.first().copied().unwrap_or("param")
        ));
    }
    Ok(value)
}

pub(super) fn query_param(query: &str, names: &[&str]) -> Option<String> {
    query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find_map(|(raw_name, raw_value)| {
            let name = raw_name.trim();
            if names.iter().any(|candidate| name == *candidate) {
                Some(raw_value.trim().replace('+', " "))
            } else {
                None
            }
        })
}

pub(super) struct HttpRequest {
    pub(super) method: String,
    pub(super) path: String,
    pub(super) headers: Vec<(String, String)>,
    pub(super) body: Vec<u8>,
}

pub(super) async fn read_request(stream: &mut TcpStream) -> Result<HttpRequest> {
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 4096];
    let header_end = loop {
        let read = stream.read(&mut chunk).await?;
        if read == 0 {
            anyhow::bail!("连接提前关闭");
        }
        buffer.extend_from_slice(&chunk[..read]);
        if buffer.len() > MAX_HEADER_BYTES {
            anyhow::bail!("HTTP header too large");
        }
        if let Some(index) = find_header_end(&buffer) {
            break index;
        }
    };

    let header_bytes = &buffer[..header_end];
    let header = std::str::from_utf8(header_bytes).context("HTTP header不是UTF-8")?;
    let mut lines = header.lines();
    let request_line = lines.next().context("缺少HTTP request line")?;
    let mut request_parts = request_line.split_whitespace();
    let method = request_parts.next().context("缺少HTTP method")?.to_string();
    let path = request_parts.next().context("缺少HTTP path")?.to_string();
    let headers = parse_headers(header);
    let content_length = parse_content_length(header)?;
    if content_length > MAX_BODY_BYTES {
        anyhow::bail!("HTTP body too large");
    }

    let body_start = header_end + 4;
    while buffer.len() < body_start + content_length {
        let read = stream.read(&mut chunk).await?;
        if read == 0 {
            anyhow::bail!("HTTP body读取不完整");
        }
        buffer.extend_from_slice(&chunk[..read]);
    }
    let body = buffer[body_start..body_start + content_length].to_vec();

    Ok(HttpRequest {
        method,
        path,
        headers,
        body,
    })
}

fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}

fn parse_content_length(header: &str) -> Result<usize> {
    for line in header.lines().skip(1) {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        if name.trim().eq_ignore_ascii_case("content-length") {
            return value
                .trim()
                .parse::<usize>()
                .context("Content-Length格式错误");
        }
    }
    Ok(0)
}

pub(super) async fn write_response(
    stream: &mut TcpStream,
    response: InternalHttpJsonResponse,
) -> Result<()> {
    let body = serde_json::to_vec(&response.body)?;
    let reason = reason_phrase(response.status_code);
    let header = format!(
        "HTTP/1.1 {} {}\r\ncontent-type: application/json; charset=utf-8\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
        response.status_code,
        reason,
        body.len()
    );
    stream.write_all(header.as_bytes()).await?;
    stream.write_all(&body).await?;
    stream.shutdown().await?;
    Ok(())
}

fn reason_phrase(status_code: u16) -> &'static str {
    match status_code {
        200 => "OK",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        500 => "Internal Server Error",
        _ => "OK",
    }
}
