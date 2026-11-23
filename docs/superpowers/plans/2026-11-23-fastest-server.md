# Fastest download server (spec §6m.2): plan

Native execution, test-first. Files: `crates/fishy-core/src/install/client.rs`, `crates/fishy-core/src/install/engine.rs`.

1. **client.rs: retries switch server.** Test: two servers, the first answers 500, the chunk comes from the second. Change `download_chunk(base, …)` to `download_chunk(bases: &[String], …)`; try `k` uses `bases[k % len]`; empty list is `BadManifest("no CDN")`. Update callers and old tests.
2. **client.rs: `fastest_first(bases, dir, sample)`.** Tests: slow server (delay) goes after the fast one; a failing server goes last; one server returns unchanged without any request. Each server fetches the sample once (no retries), all servers at the same time.
3. **client.rs: `host_of(url)`.** Test: `https://a.b.com/Builds/x` gives `a.b.com`; junk gives `"?"`.
4. **engine.rs: use them.** Test: with a limiter set, no test request is made (wiremock `expect` counts). `run_job` orders the servers (sample: first 2 chunks of the plan, skipped when the limiter has a limit, cancellable by Stop), logs "download: using host" and at the end "download: N MB at X MB/s".
5. `cargo test --workspace`, `cargo clippy`, commit.
