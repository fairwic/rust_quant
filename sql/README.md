# Quant Core Schema

`postgres_quant_core.sql` documents the target standalone `quant_core` Postgres database tables for `rust_quant`.
It is not a sqlx migration and does not replace existing migration history.

## Database Layout

Postgres is shared at the server/container level, but service data is split by independent databases:

- `rust_quant` uses database `quant_core`.
- `rust_quant_news` uses database `quant_news`.

Both databases use the default `public` schema. Do not model this as one database with separate
`quant_core`/`quant_news` schemas, and do not add schema-qualified table names to this DDL unless
the runtime code is changed at the same time.

## DDL Smoke

Run the idempotent smoke against the local Postgres container:

```bash
scripts/dev/ddl_smoke.sh
```

By default it runs `podman exec postgres psql -U postgres -d quant_core`, applies
`sql/postgres_quant_core.sql`, and lists key tables in `public`.
