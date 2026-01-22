# News Analysis Service Design

## Goal
Split market news analysis into an independent microservice with a gRPC API, scheduled collection, and a dedicated PostgreSQL database. The service fetches news from Jinse and stores normalized items for downstream use.

## Scope
- New repo at /Users/mac2/onions/news-analysis-service
- gRPC API for fetching and listing news
- Internal 1-minute scheduled fetch
- Dedicated Postgres storage with migrations
- Docker-based local development

## Out of Scope
- Tight coupling with existing rust_quant crates
- Real-time streaming or pub/sub integrations
- Advanced NLP or sentiment analysis

## Architecture
- Single Rust binary service
- Modules:
  - config: env loading, defaults, validation
  - grpc: tonic service definitions and server
  - collector: HTTP client + Jinse response mapping
  - scheduler: cron-driven fetch loop
  - storage: sqlx repositories + migrations
  - domain: news model and normalization rules

## Data Flow
1. Scheduler triggers every 1 minute.
2. Collector calls Jinse API with required headers.
3. JSON parsed into domain NewsItem.
4. Storage upserts into Postgres with dedupe.
5. gRPC ListLatest serves stored items.

## gRPC API
Proto (tonic):
- FetchLatest(FetchLatestRequest) -> FetchLatestResponse
  - Triggers immediate fetch, optional limit override.
- ListLatest(ListLatestRequest) -> ListLatestResponse
  - Paginated list by limit + cursor (or since_id).

## Data Model
Table: news_items
- id (bigint) PRIMARY KEY
- title (text)
- content (text)
- source (text)
- category (text)
- published_at (timestamptz)
- raw_json (jsonb)
- created_at (timestamptz)

Constraints:
- UNIQUE(id) to enforce dedupe
- Optional fallback unique key if upstream id is unstable

## Error Handling
- HTTP errors: retry with capped exponential backoff
- JSON parsing errors: log raw payload summary and continue
- DB errors: map to gRPC INTERNAL with safe message

## Testing
- Unit tests for JSON -> NewsItem mapping
- Storage integration test for upsert + unique constraint
- gRPC smoke test with mock storage

## Deployment
- Dockerfile for service
- docker-compose.yml for local dev with Postgres
- Config via env (DB URL, cron interval, HTTP timeout)

## Success Criteria
- Service runs independently and fetches every minute
- gRPC ListLatest returns stored items
- Dedupe prevents duplicates across runs
- Migrations managed via sqlx
