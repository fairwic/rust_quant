from __future__ import annotations

import csv
import io
import os
import shutil
import subprocess
from pathlib import Path


DEFAULT_DATABASE_URL = "postgres://postgres:postgres123@localhost:5432/quant_core"
DEFAULT_POSTGRES_CONTAINER = "postgres"
REPO_ROOT = Path(__file__).resolve().parents[1]


def repo_root() -> Path:
    return REPO_ROOT


def quant_core_database_url() -> str:
    return (
        os.environ.get("QUANT_CORE_DATABASE_URL")
        or os.environ.get("DATABASE_URL")
        or DEFAULT_DATABASE_URL
    )


def binary_database_env() -> dict[str, str]:
    database_url = quant_core_database_url()
    return {
        "QUANT_CORE_DATABASE_URL": database_url,
        "DATABASE_URL": database_url,
    }


def quote_identifier(identifier: str) -> str:
    return '"' + identifier.replace('"', '""') + '"'


def sql_quote(text: str) -> str:
    return "'" + text.replace("'", "''") + "'"


def _psql_cmd(*extra_args: str) -> list[str]:
    database_url = quant_core_database_url()
    if shutil.which("psql"):
        return ["psql", database_url, "-v", "ON_ERROR_STOP=1", "-X", *extra_args]

    postgres_container = os.environ.get("POSTGRES_CONTAINER", DEFAULT_POSTGRES_CONTAINER)
    return [
        "podman",
        "exec",
        "-i",
        postgres_container,
        "psql",
        database_url,
        "-v",
        "ON_ERROR_STOP=1",
        "-X",
        *extra_args,
    ]


def run_sql(sql: str) -> None:
    subprocess.run(
        _psql_cmd("-c", sql),
        cwd=REPO_ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=True,
    )


def query_rows(sql: str) -> list[dict[str, str]]:
    copy_sql = f"COPY ({sql}) TO STDOUT WITH CSV HEADER"
    result = subprocess.run(
        _psql_cmd("-c", copy_sql),
        cwd=REPO_ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=True,
    )
    return list(csv.DictReader(io.StringIO(result.stdout)))


def query_scalar(sql: str) -> str:
    rows = query_rows(sql)
    if not rows:
        raise RuntimeError(f"query returned no rows: {sql}")
    first_row = rows[0]
    if not first_row:
        raise RuntimeError(f"query returned an empty row: {sql}")
    return next(iter(first_row.values()))
