from __future__ import annotations

import os
from typing import Any

from .http_json import curl_json, curl_proxy

CHAINSTACK_NODES_URL = "https://api.chainstack.com/v1/nodes/"


def resolve_bsc_rpc_url(
    explicit_url: str | None,
    rpc_env_name: str = "BSC_RPC_URL",
    chainstack_key_env_name: str = "CHAINSTACK_API_KEY",
) -> str | None:
    if explicit_url:
        return explicit_url
    env_url = os.environ.get(rpc_env_name)
    if env_url:
        return env_url
    api_key = os.environ.get(chainstack_key_env_name)
    if not api_key:
        return None
    return resolve_chainstack_rpc_url(api_key)


def resolve_chainstack_rpc_url(api_key: str) -> str:
    payload = curl_json(
        CHAINSTACK_NODES_URL,
        curl_proxy(),
        {"Authorization": f"Bearer {api_key}"},
    )
    return resolve_chainstack_rpc_url_from_nodes(payload)


def resolve_chainstack_rpc_url_from_nodes(payload: dict[str, Any]) -> str:
    for node in payload.get("results") or []:
        details = node.get("details") or {}
        endpoint = str(details.get("https_endpoint") or "")
        auth_key = str(details.get("auth_key") or "")
        namespaces = details.get("api_namespaces") or []
        if node.get("status") != "running" or not endpoint or not auth_key:
            continue
        if namespaces and "eth" not in namespaces:
            continue
        if not is_bsc_node(node, endpoint):
            continue
        return endpoint.rstrip("/") + "/" + auth_key.strip("/")
    raise RuntimeError("CHAINSTACK_BSC_NODE_NOT_FOUND")


def is_bsc_node(node: dict[str, Any], endpoint: str) -> bool:
    protocol = str(node.get("protocol") or "").lower()
    network = str(node.get("network") or "").lower()
    endpoint_lower = endpoint.lower()
    return (
        protocol == "bsc"
        or network in {"bsc-mainnet", "bnb-mainnet", "bnb-smart-chain-mainnet"}
        or "bsc-mainnet" in endpoint_lower
        or "bnb-mainnet" in endpoint_lower
    )
