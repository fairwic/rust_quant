# gRPC Service Migration TODO

This document tracks every actionable step required to migrate the monolithic Rust Quant pipeline into gRPC-based services. Update the status markers (`[ ]` Todo, `[~]` In progress, `[x]` Done, `[!]` Blocked, `[>]` Deferred) immediately after any change that touches the related scope.

## Phase 0 — Baseline and Guardrails
1. [ ] Freeze reference branch and capture current system diagram (link to `docs/ARCHITECTURE_DIAGRAM.md`).
2. [ ] Document MySQL schema snapshot (`create_table.sql` plus new `signals`, `orders`, `risk_checks` tables).
3. [ ] Confirm Redis topology, authentication, and resource limits for cross-service messaging.
4. [ ] Add independent workspace crates at the repository root (e.g., `market_data_service`, `indicator_service`, `strategy_service`, `risk_service`, `execution_service`) each with its own `Cargo.toml`, binary entry, and Docker build context.

## Phase 1 — Domain Contracts and Protos
1. [ ] Introduce `domain/src/events/{signal_event.rs, order_event.rs, risk_event.rs}` and DTOs for candles, indicators, signals, orders.
2. [ ] Define `MarketDataPort`, `IndicatorPort`, `StrategyPort`, `ExecutionPort`, `RiskPort` traits under `domain/src/traits/`.
3. [ ] Create `proto/` workspace directory with `.proto` files per service, mirroring domain DTOs and service RPCs.
4. [ ] Wire `prost-build` (or tonic-build) into `build.rs` for generated gRPC clients/servers and expose them through a new `common::grpc` module.

## Phase 2 — Service Layer Skeletons
1. [ ] Inside each new service crate, expose a tonic server entrypoint (`main.rs`) delegating to a shared library (`crates/services-common`) that implements the corresponding domain trait.
2. [ ] Refactor existing `crates/services` into `crates/services-common` (lib crate) exporting reusable service logic and DI wiring.
3. [ ] Implement `market-data-service` binary using `services-common::market::MarketDataService` plus infrastructure adapters (MySQL/Redis).
4. [ ] Implement `indicator-service` binary orchestrating indicator computations with gRPC APIs for batch + streaming requests.
5. [ ] Implement `strategy-service` binary hosting strategy engines, emitting signals via Redis Streams and persisting to MySQL.
6. [ ] Implement `risk-service` binary encapsulating rule evaluation, providing synchronous `ValidateSignal`/`ValidateOrder` RPCs.
7. [ ] Implement `execution-service` binary wrapping exchange APIs, persisting orders, and exposing submission/status RPCs.

## Phase 3 — Infrastructure Adapters
1. [ ] Create `common/src/messaging/redis_stream.rs` with publish, subscribe, and consumer-group helpers reused by strategy, execution, risk.
2. [ ] Update `infrastructure/src/repositories/*` to implement the new domain traits (`MarketDataPort`, etc.) and register them behind trait objects.
3. [ ] Add gRPC client adapters in each consumer crate (e.g., `strategies` depends on `indicator-service` client) that satisfy the same trait interfaces for local invocation.
4. [ ] Provide connection pooling and retry middleware in `core` for tonic channels (interceptors, tracing, auth metadata).

## Phase 4 — Flow Rewiring
1. [ ] Modify `market` crate pipelines to publish candles via gRPC to indicator service instead of intra-process calls.
2. [ ] Refactor `strategies/src/adapters` to request indicators through gRPC clients and persist signals to MySQL + Redis Stream.
3. [ ] Introduce signal ingestion worker in `execution` crate that consumes Redis Stream entries, calls risk service over gRPC, and submits approved orders.
4. [ ] Ensure feedback loop: execution results pushed to risk and strategy services via `OrderEvent` RPCs for state reconciliation.

## Phase 5 — Observability, QA, and Cleanup
1. [ ] Embed `tracing` spans + Prometheus metrics into every new tonic server and client; ensure each binary exposes `/metrics`.
2. [ ] Write integration tests in `tests/grpc_pipeline.rs` covering candle→indicator→strategy→risk→execution round trip within 500 ms budget (using mocked timers where needed).
3. [ ] Update documentation (`docs/ARCHITECTURE_REDESIGN.md`, `SERVICE_DOMAIN_INFRASTRUCTURE_CALLING_PATTERNS.md`) to reflect the gRPC topology and independent deployment model.
4. [ ] Remove deprecated direct-function adapters once gRPC paths are stable and validated; deprecate former in-process service constructors.

## Phase 6 — Rollout Checklist
1. [ ] Prepare deployment manifests (systemd, Docker, or Kubernetes) even if not immediately used, to capture runtime configuration per service.
2. [ ] Conduct load test hitting 2× target throughput, capture latency histogram, and verify 99p <= 500 ms.
3. [ ] Final sign-off: switch orchestration to call the gRPC services exclusively and archive this TODO list with completion markers.


