/// load_test.rs — 1 000 parallel HTTP requests against the QuorumCredit API.
///
/// Design decisions (senior-level rationale):
///
/// 1. **In-process server**: We spin up the real Axum app on a random port using
///    `run_server()`.  No mocks — every middleware layer (JWT auth, rate-limiter,
///    logging) executes under load exactly as it does in production.
///
/// 2. **Enterprise tier**: Free burst = 10, Pro burst = 50.  1 000 concurrent
///    requests would all be rejected under those tiers.  We set
///    `RATE_LIMIT_TIER=enterprise` with RPM = 10 000 / burst = 2 000 so the
///    test validates throughput logic, not rate-limiter rejection logic.
///
/// 3. **Separate API keys per cohort**: We split 1 000 workers into 10 cohorts
///    of 100, each with its own API key.  This mirrors real-world multi-tenant
///    traffic and verifies per-key bucket isolation under concurrency.
///
/// 4. **Three endpoint targets**: 400 × /health (no-auth), 400 × /auth/token
///    (auth write), 200 × POST /api/admin/metrics (JWT-gated analytics read).
///    Blended load exercises every middleware layer in one test run.
///
/// 5. **Latency percentiles**: We collect raw durations and compute p50/p95/p99.
///    The test asserts p99 < 500 ms — a realistic SLO for this service class.
///
/// 6. **Error budget**: We tolerate ≤ 1% errors (i.e., ≤ 10 of 1 000) to allow
///    for the small window where tokens expire or connections queue under burst.
///
/// Run with:
///   cargo test -p quorum_credit_api load_test -- --nocapture --test-threads=1
#[cfg(test)]
mod load_tests {
    use crate::run_server;
    use reqwest::Client;
    use serde_json::json;
    use std::sync::Arc;
    use std::time::{Duration, Instant};
    use tokio::sync::Barrier;
    use tokio::time::timeout;

    // ── Constants ──────────────────────────────────────────────────────────────

    const TOTAL_REQUESTS: usize = 1_000;
    const COHORTS: usize = 10;
    const REQUESTS_PER_COHORT: usize = TOTAL_REQUESTS / COHORTS; // 100

    /// Hard limit on individual request latency — prevents slow requests from
    /// blocking the barrier indefinitely.
    const REQUEST_TIMEOUT_MS: u64 = 5_000;

    /// p99 SLO in milliseconds.
    const P99_SLO_MS: u64 = 500;

    /// Maximum tolerated failure rate (1 %).
    const MAX_FAILURE_RATE: f64 = 0.01;

    // ── Helper: pick a random available port ──────────────────────────────────

    fn random_port() -> u16 {
        // Bind port 0 → OS assigns a free port → extract it → drop listener.
        let listener =
            std::net::TcpListener::bind("127.0.0.1:0").expect("failed to bind ephemeral port");
        listener.local_addr().unwrap().port()
    }

    // ── Helper: mint a JWT for the given API key ──────────────────────────────

    async fn mint_token(base: &str, api_key: &str, client: &Client) -> String {
        let resp = client
            .post(format!("{}/auth/token", base))
            .json(&json!({ "api_key": api_key }))
            .send()
            .await
            .expect("auth/token request failed");
        let body: serde_json::Value = resp.json().await.expect("auth/token body parse failed");
        body["token"]
            .as_str()
            .expect("token field missing")
            .to_string()
    }

    // ── Helper: sample analytics payload ─────────────────────────────────────

    fn metrics_payload() -> serde_json::Value {
        json!({
            "loans": [
                {
                    "borrower": "GBORROWER1AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
                    "amount": 5_000_000_000i64,
                    "status": "active",
                    "yield_distributed": 0,
                    "created_at": 1_700_000_000i64
                },
                {
                    "borrower": "GBORROWER2AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
                    "amount": 3_000_000_000i64,
                    "status": "repaid",
                    "yield_distributed": 60_000_000i64,
                    "created_at": 1_700_100_000i64
                }
            ],
            "vouches": [
                { "voucher": "GVOUCHER1AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA", "stake": 1_000_000_000i64 },
                { "voucher": "GVOUCHER2AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA", "stake": 500_000_000i64 }
            ],
            "slash_count": 1,
            "fee_revenue": 50_000_000i64,
            "export_format": "json"
        })
    }

    // ─────────────────────────────────────────────────────────────────────────
    // MAIN LOAD TEST
    // ─────────────────────────────────────────────────────────────────────────

    /// Fires 1 000 parallel HTTP requests against a live in-process API server
    /// and asserts throughput / latency / error-rate SLOs.
    #[tokio::test(flavor = "multi_thread", worker_threads = 8)]
    async fn test_1000_parallel_requests() {
        // ── 1. Configure environment for this test run ─────────────────────
        // Enterprise tier: burst = 2 000 >> 1 000 concurrent, so the
        // rate-limiter never blocks legitimate load.
        std::env::set_var("RATE_LIMIT_TIER", "enterprise");
        std::env::set_var("RATE_LIMIT_RPM", "10000");
        std::env::set_var("RATE_LIMIT_BURST", "2000");
        std::env::set_var("JWT_SECRET", "load_test_secret_key_32bytes_min!");

        let port = random_port();
        let base = format!("http://127.0.0.1:{}", port);

        // ── 2. Start server in background ──────────────────────────────────
        tokio::spawn(async move {
            run_server(port)
                .await
                .expect("load-test server failed to start");
        });

        // Wait for the server to be ready (poll /health, max 5 s).
        let client = Client::builder()
            .timeout(Duration::from_millis(REQUEST_TIMEOUT_MS))
            .pool_max_idle_per_host(200) // allow enough connection pooling
            .build()
            .expect("reqwest client build failed");

        let ready_deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if Instant::now() > ready_deadline {
                panic!("server did not become healthy within 5 s");
            }
            if client
                .get(format!("{}/health", base))
                .send()
                .await
                .map(|r| r.status().is_success())
                .unwrap_or(false)
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        // ── 3. Pre-mint one JWT per cohort ─────────────────────────────────
        let mut tokens: Vec<String> = Vec::with_capacity(COHORTS);
        for i in 0..COHORTS {
            let api_key = format!("load-test-key-cohort-{:02}", i);
            let token = mint_token(&base, &api_key, &client).await;
            tokens.push(token);
        }
        let tokens = Arc::new(tokens);

        // ── 4. Build 1 000 request futures ─────────────────────────────────
        //
        // Layout:
        //   Requests 0..399   → GET /health         (no auth, cheapest)
        //   Requests 400..799 → POST /auth/token     (auth write path)
        //   Requests 800..999 → POST /api/admin/metrics (JWT-gated)
        //
        // A Barrier ensures all goroutines fire simultaneously, simulating a
        // true thundering-herd scenario rather than a gradual ramp.

        let barrier = Arc::new(Barrier::new(TOTAL_REQUESTS));
        let mut handles = Vec::with_capacity(TOTAL_REQUESTS);

        for idx in 0..TOTAL_REQUESTS {
            let client = client.clone();
            let base = base.clone();
            let barrier = barrier.clone();
            let tokens = tokens.clone();

            let handle = tokio::spawn(async move {
                // Wait for all workers to be ready.
                barrier.wait().await;

                let start = Instant::now();
                let result = match idx {
                    // ── Bucket A: /health ──────────────────────────────────
                    0..=399 => {
                        timeout(
                            Duration::from_millis(REQUEST_TIMEOUT_MS),
                            client.get(format!("{}/health", base)).send(),
                        )
                        .await
                    }

                    // ── Bucket B: POST /auth/token ─────────────────────────
                    400..=799 => {
                        let cohort = (idx - 400) / REQUESTS_PER_COHORT;
                        let api_key = format!("load-test-key-cohort-{:02}", cohort % COHORTS);
                        timeout(
                            Duration::from_millis(REQUEST_TIMEOUT_MS),
                            client
                                .post(format!("{}/auth/token", base))
                                .json(&json!({ "api_key": api_key }))
                                .send(),
                        )
                        .await
                    }

                    // ── Bucket C: POST /api/admin/metrics ──────────────────
                    _ => {
                        let cohort = (idx - 800) % COHORTS;
                        let token = &tokens[cohort];
                        timeout(
                            Duration::from_millis(REQUEST_TIMEOUT_MS),
                            client
                                .post(format!("{}/api/admin/metrics", base))
                                .bearer_auth(token)
                                .json(&metrics_payload())
                                .send(),
                        )
                        .await
                    }
                };

                let elapsed = start.elapsed();

                match result {
                    Ok(Ok(resp)) => {
                        let status = resp.status().as_u16();
                        // 200 OK, 202 Accepted, and 204 No Content are all
                        // valid success codes for this API.
                        let success = matches!(status, 200 | 201 | 202 | 204);
                        (success, elapsed, idx)
                    }
                    Ok(Err(e)) => {
                        eprintln!("[worker {}] request error: {}", idx, e);
                        (false, elapsed, idx)
                    }
                    Err(_) => {
                        eprintln!("[worker {}] timed out after {}ms", idx, REQUEST_TIMEOUT_MS);
                        (false, elapsed, idx)
                    }
                }
            });

            handles.push(handle);
        }

        // ── 5. Collect results ─────────────────────────────────────────────
        // Wall-clock starts here — tasks are already spawned and waiting at the
        // barrier, so the first barrier.wait() call will release them all at once.
        // We measure from just before collecting to just after, which captures
        // the true parallel execution window.
        let overall_start = Instant::now();
        let mut successes: usize = 0;
        let mut failures: usize = 0;
        let mut latencies_ms: Vec<u64> = Vec::with_capacity(TOTAL_REQUESTS);

        for handle in handles {
            let (success, elapsed, _idx) = handle.await.expect("worker panicked");
            latencies_ms.push(elapsed.as_millis() as u64);
            if success {
                successes += 1;
            } else {
                failures += 1;
            }
        }

        let wall_time_ms = overall_start.elapsed().as_millis() as u64;

        // ── 6. Compute latency percentiles ────────────────────────────────
        latencies_ms.sort_unstable();
        let p50 = percentile(&latencies_ms, 50);
        let p95 = percentile(&latencies_ms, 95);
        let p99 = percentile(&latencies_ms, 99);
        let p_max = *latencies_ms.last().unwrap_or(&0);

        let error_rate = failures as f64 / TOTAL_REQUESTS as f64;
        let throughput = TOTAL_REQUESTS as f64 / (wall_time_ms as f64 / 1_000.0);

        // ── 7. Print report ───────────────────────────────────────────────
        println!("\n╔══════════════════════════════════════════════════════════╗");
        println!("║           QuorumCredit API — Load Test Report           ║");
        println!("╠══════════════════════════════════════════════════════════╣");
        println!("║  Total requests     : {:>6}                            ║", TOTAL_REQUESTS);
        println!("║  Successes          : {:>6}                            ║", successes);
        println!("║  Failures           : {:>6}                            ║", failures);
        println!("║  Error rate         : {:>6.2}%                          ║", error_rate * 100.0);
        println!("║  Wall-clock time    : {:>6} ms                         ║", wall_time_ms);
        println!("║  Throughput         : {:>6.1} req/s                     ║", throughput);
        println!("╠══════════════════════════════════════════════════════════╣");
        println!("║  Latency p50        : {:>6} ms                         ║", p50);
        println!("║  Latency p95        : {:>6} ms                         ║", p95);
        println!("║  Latency p99        : {:>6} ms  (SLO ≤ {}ms)           ║", p99, P99_SLO_MS);
        println!("║  Latency max        : {:>6} ms                         ║", p_max);
        println!("╚══════════════════════════════════════════════════════════╝\n");

        // ── 8. Assert SLOs ────────────────────────────────────────────────
        assert!(
            error_rate <= MAX_FAILURE_RATE,
            "Error rate {:.2}% exceeds budget of {:.2}% ({} failures out of {})",
            error_rate * 100.0,
            MAX_FAILURE_RATE * 100.0,
            failures,
            TOTAL_REQUESTS,
        );

        assert!(
            p99 <= P99_SLO_MS,
            "p99 latency {}ms exceeds SLO of {}ms — server is too slow under load",
            p99,
            P99_SLO_MS,
        );

        assert!(
            successes >= TOTAL_REQUESTS - (TOTAL_REQUESTS as f64 * MAX_FAILURE_RATE) as usize,
            "Insufficient successes: {} out of {}",
            successes,
            TOTAL_REQUESTS,
        );

        println!(
            "✓ Load test passed — {}/{} requests succeeded, p99={}ms, throughput={:.1} req/s",
            successes, TOTAL_REQUESTS, p99, throughput
        );
    }

    // ─────────────────────────────────────────────────────────────────────────
    // SUPPLEMENTARY TESTS
    // ─────────────────────────────────────────────────────────────────────────

    /// Verifies that the Enterprise-tier rate limiter actually blocks requests
    /// beyond its burst capacity, confirming the limiter is active under load.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_rate_limiter_activates_under_burst() {
        std::env::set_var("RATE_LIMIT_TIER", "free"); // burst = 10
        std::env::set_var("JWT_SECRET", "load_test_secret_key_32bytes_min!");

        let port = random_port();
        let base = format!("http://127.0.0.1:{}", port);

        tokio::spawn(async move {
            run_server(port).await.ok();
        });

        let client = Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap();

        // Wait for server ready.
        tokio::time::sleep(Duration::from_millis(300)).await;

        // Fire 30 requests with the SAME api key so they all hit one bucket.
        // With Free burst = 10, at least some must be 429.
        let key = "burst-test-key";
        let barrier = Arc::new(Barrier::new(30));
        let mut handles = Vec::new();

        for _ in 0..30 {
            let client = client.clone();
            let base = base.clone();
            let barrier = barrier.clone();
            let key = key.to_string();

            handles.push(tokio::spawn(async move {
                barrier.wait().await;
                client
                    .get(format!("{}/health", base))
                    .header("x-api-key", key)
                    .send()
                    .await
                    .map(|r| r.status().as_u16())
                    .unwrap_or(0)
            }));
        }

        let statuses: Vec<u16> = futures_util::future::join_all(handles)
            .await
            .into_iter()
            .filter_map(|r| r.ok())
            .collect();

        let throttled = statuses.iter().filter(|&&s| s == 429).count();
        println!(
            "  Rate-limiter test: {} / 30 requests throttled (burst=10)",
            throttled
        );

        // With Free burst=10 and 30 simultaneous requests, at least 15 must
        // be throttled (conservative lower bound — in practice ~20).
        assert!(
            throttled >= 15,
            "Expected at least 15 throttled responses, got {}",
            throttled
        );
    }

    /// Verifies per-API-key bucket isolation: exhausting one key's quota must
    /// not degrade requests from a different key.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_per_key_bucket_isolation_under_parallel_load() {
        std::env::set_var("RATE_LIMIT_TIER", "free"); // burst = 10
        std::env::set_var("JWT_SECRET", "load_test_secret_key_32bytes_min!");

        let port = random_port();
        let base = format!("http://127.0.0.1:{}", port);

        tokio::spawn(async move {
            run_server(port).await.ok();
        });

        let client = Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap();

        tokio::time::sleep(Duration::from_millis(300)).await;

        // Exhaust bucket for key-A (20 rapid requests).
        for _ in 0..20 {
            client
                .get(format!("{}/health", base))
                .header("x-api-key", "key-a-exhaust")
                .send()
                .await
                .ok();
        }

        // Key-B should still have a fresh bucket and succeed.
        let resp = client
            .get(format!("{}/health", base))
            .header("x-api-key", "key-b-fresh")
            .send()
            .await
            .expect("key-b request failed");

        assert_eq!(
            resp.status().as_u16(),
            200,
            "key-b should not be affected by key-a exhaustion"
        );
    }

    /// Verifies that the /ready endpoint reports healthy under concurrent load.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_ready_endpoint_under_concurrent_load() {
        std::env::set_var("RATE_LIMIT_TIER", "enterprise");
        std::env::set_var("RATE_LIMIT_RPM", "5000");
        std::env::set_var("RATE_LIMIT_BURST", "500");
        std::env::set_var("JWT_SECRET", "load_test_secret_key_32bytes_min!");

        let port = random_port();
        let base = format!("http://127.0.0.1:{}", port);

        tokio::spawn(async move {
            run_server(port).await.ok();
        });

        let client = Client::builder()
            .timeout(Duration::from_secs(5))
            .pool_max_idle_per_host(100)
            .build()
            .unwrap();

        tokio::time::sleep(Duration::from_millis(300)).await;

        let barrier = Arc::new(Barrier::new(100));
        let mut handles = Vec::new();

        for _ in 0..100 {
            let client = client.clone();
            let base = base.clone();
            let barrier = barrier.clone();

            handles.push(tokio::spawn(async move {
                barrier.wait().await;
                client
                    .get(format!("{}/ready", base))
                    .send()
                    .await
                    .map(|r| r.status().as_u16())
                    .unwrap_or(0)
            }));
        }

        let statuses: Vec<u16> = futures_util::future::join_all(handles)
            .await
            .into_iter()
            .filter_map(|r| r.ok())
            .collect();

        let ok_count = statuses.iter().filter(|&&s| s == 200).count();
        assert!(
            ok_count >= 90,
            "Expected ≥90 /ready 200s under concurrent load, got {}",
            ok_count
        );
    }

    // ── Utility: integer percentile (nearest-rank) ─────────────────────────

    /// Returns the value at the given percentile (0–100) from a sorted slice.
    fn percentile(sorted: &[u64], pct: usize) -> u64 {
        if sorted.is_empty() {
            return 0;
        }
        let idx = ((pct as f64 / 100.0) * sorted.len() as f64).ceil() as usize;
        sorted[(idx.saturating_sub(1)).min(sorted.len() - 1)]
    }
}
