#!/usr/bin/env bash
set -euo pipefail

: "${SSH_HOST:?SSH_HOST is required}"

ssh_host_input="${SSH_HOST}"
ssh_port="${SSH_PORT:-}"
ssh_host="${ssh_host_input}"

if [[ "${ssh_host_input}" == *"@"* ]]; then
  echo "SSH_HOST must not include a username. Set SSH_USER separately." >&2
  exit 1
fi

if [[ "${ssh_host_input}" =~ ^\[([^]]+)\]:(.+)$ ]]; then
  if [[ -z "${ssh_port}" ]]; then
    ssh_port="${BASH_REMATCH[2]}"
  fi
  ssh_host="${BASH_REMATCH[1]}"
elif [[ "${ssh_host_input}" =~ ^([^:]+):([0-9]+)$ ]]; then
  if [[ -z "${ssh_port}" ]]; then
    ssh_port="${BASH_REMATCH[2]}"
  fi
  ssh_host="${BASH_REMATCH[1]}"
fi

ssh_port="${ssh_port:-22}"

mkdir -p ~/.ssh
chmod 700 ~/.ssh
touch ~/.ssh/known_hosts
chmod 600 ~/.ssh/known_hosts

if [[ -n "${SSH_KNOWN_HOSTS:-}" ]]; then
  printf '%s\n' "${SSH_KNOWN_HOSTS}" >> ~/.ssh/known_hosts
  exit 0
fi

if ! ssh-keyscan -p "${ssh_port}" -H "${ssh_host}" >> ~/.ssh/known_hosts 2>/dev/null; then
  echo "ssh-keyscan failed for ${ssh_host}:${ssh_port}. Check SSH_HOST/SSH_PORT or set SSH_KNOWN_HOSTS." >&2
  exit 1
fi
