from __future__ import annotations

import json
import os
import subprocess
import urllib.request
from typing import Any

USER_AGENT = "rust-quant-backtest/1.0"


def curl_json(
    url: str, proxy: str | None = None, headers: dict[str, str] | None = None
) -> dict[str, Any]:
    cmd = ["curl", "-4", "-L", "--connect-timeout", "8", "--max-time", "20", "-sS"]
    if proxy:
        cmd.extend(["--proxy", proxy])
    for key, value in (headers or {}).items():
        cmd.extend(["-H", f"{key}: {value}"])
    cmd.append(url)
    try:
        raw = subprocess.check_output(cmd, stderr=subprocess.STDOUT, timeout=25)
    except subprocess.CalledProcessError as exc:
        detail = exc.output.decode(errors="ignore")[:120]
        raise RuntimeError(f"CURL_REQUEST_FAILED:{detail}") from exc
    except subprocess.TimeoutExpired as exc:
        raise RuntimeError("CURL_REQUEST_TIMEOUT") from exc
    return json.loads(raw.decode("utf-8"))


def curl_json_rpc(rpc_url: str, body: str, proxy: str) -> dict[str, Any]:
    cmd = [
        "curl",
        "-4",
        "-L",
        "--connect-timeout",
        "8",
        "--max-time",
        "30",
        "-sS",
        "--proxy",
        proxy,
        "-H",
        "Content-Type: application/json",
        "--data",
        body,
        rpc_url,
    ]
    try:
        raw = subprocess.check_output(cmd, stderr=subprocess.STDOUT, timeout=35)
    except subprocess.CalledProcessError as exc:
        detail = exc.output.decode(errors="ignore")[:120]
        raise RuntimeError(f"CURL_RPC_FAILED:{detail}") from exc
    except subprocess.TimeoutExpired as exc:
        raise RuntimeError("CURL_RPC_TIMEOUT") from exc
    return json.loads(raw.decode("utf-8"))


def curl_proxy() -> str | None:
    proxy = (
        os.environ.get("RUST_QUANT_HTTP_PROXY")
        or os.environ.get("HTTPS_PROXY")
        or os.environ.get("https_proxy")
        or os.environ.get("ALL_PROXY")
        or os.environ.get("all_proxy")
    )
    if not proxy:
        return None
    if proxy.startswith("socks5://"):
        return "socks5h://" + proxy[len("socks5://") :]
    return proxy


def urllib_json_rpc(rpc_url: str, body: str) -> dict[str, Any]:
    req = urllib.request.Request(
        rpc_url,
        data=body.encode("utf-8"),
        headers={"Content-Type": "application/json", "User-Agent": USER_AGENT},
    )
    with urllib.request.urlopen(req, timeout=20) as resp:
        return json.loads(resp.read().decode("utf-8"))
