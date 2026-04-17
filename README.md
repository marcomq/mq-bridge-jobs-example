# mq-bridge jobs example

A minimal example showing how to build a **transport-agnostic job system in Rust**.

Define your jobs once, start locally (file-based), and switch to NATS/Kafka or other backends — **without changing your handler code**.

---

## Idea

Most job/queue systems require committing early to a specific broker and worker model.

This example shows a simpler approach:
- define jobs as plain Rust structs  
- run everything locally (file-based)  
- switch to a real transport via config  
- keep business logic unchanged  

---

## Quick walkthrough (file → NATS)

### 1. Define job structs

[src/jobs.rs](./src/jobs.rs)
```rust
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct SendEmail {
    pub to: String,
    pub subject: String,
}

#[derive(Serialize, Deserialize)]
pub struct GenerateReport {
    pub user_id: u32,
}

impl SendEmail {
    pub const KIND: &'static str = "send_mail";
}
impl GenerateReport {
    pub const KIND: &'static str = "generate_report";
}

````

[src/bin/worker.rs](https://github.com/marcomq/mq-bridge-jobs-example/blob/73c43e4db8fb5256937638a949d601adf09a4c09/src/bin/worker.rs)
```rust
use mq_bridge::{
    Handled, Route,
    models::{Endpoint, EndpointType, FileConfig, FileConsumerMode},
    type_handler::TypeHandler,
};
use mq_bridge_jobs_example::jobs::{GenerateReport, SendEmail};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new("info"))
        .init();

    let jobs = TypeHandler::new()
        .add(SendEmail::KIND, |job: SendEmail| async move {
            tracing::info!("Sending email to {}", job.to);
            tokio::time::sleep(Duration::from_millis(100)).await;
            Ok(Handled::Ack)
        })
        .add(GenerateReport::KIND, |job: GenerateReport| async move {
            tracing::info!("Generating report for user {}", job.user_id);
            Ok(Handled::Ack)
        });

    let route = Route::new(
        Endpoint::new(EndpointType::File(
            FileConfig::new("jobs.jsonl").with_mode(FileConsumerMode::Consume { delete: true }),
        )),
        Endpoint::null(), // No output needed here
    )
    .with_handler(jobs);
    route.deploy("job_worker").await?;

    // wait for ctrl + c
    tracing::info!("Worker running — press Ctrl-C to exit");
    tokio::signal::ctrl_c().await?;
    tracing::info!("Shutting down");
    Ok(())
}
```

[src/bin/submit.rs](https://github.com/marcomq/mq-bridge-jobs-example/blob/73c43e4db8fb5256937638a949d601adf09a4c09/src/bin/submit.rs)
```rust
use mq_bridge::{
    Publisher,
    models::{Endpoint, EndpointType, FileConfig},
    msg,
};
use mq_bridge_jobs_example::jobs::SendEmail;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new("info"))
        .init();

    let publisher = Publisher::new(Endpoint::new(EndpointType::File(FileConfig::new(
        "jobs.jsonl",
    ))))
    .await?;
    publisher
        .send(msg!(
            SendEmail {
                to: "user@example.com".into(),
                subject: "Welcome!".into()
            },
            SendEmail::KIND
        ))
        .await?;
    Ok(())
}
```
---

### 2. Run locally (no broker)

Start the worker:

```bash
cargo run --bin worker
```

Submit a job:

```bash
cargo run --bin submit
```

- Works fully offline using a file backend.

---

### 3. Switch endpoint configuration to read files
```rust
    let route: Route = serde_json::from_str(include_str!("config.json"))?;
    let publisher = Publisher::new(route.input).await?;
```

### 4. Switch to NATS

Enable nats feature:

```toml
mq-bridge = { version = "0.2.11", features = ["nats"] }
```

Update config:

```json
{
  "input": {
    "nats": {
      "url": "nats://localhost:4222",
      "subject": "test-stream.pipeline",
      "stream": "test-stream"
    }
  }
}
```

Start nats
```bash
docker run -p 4222:4222 nats:2.12.2 -js
```

- No changes to handler code required.

---

## Supported transports

* HTTP / gRPC
* NATS, Kafka, RabbitMQ
* SQL (MySQL, MariaDB, PostgreSQL via sqlx)
* MongoDB
* Files and Memory (local dev)

---

## Why this is useful

* no early lock-in to a broker
* same code for local dev and production
* easy to test (memory backend)
* infrastructure lives in config, not code

---

## Full tutorial

See the full step-by-step guide:
👉 [`TUTORIAL.md`](./TUTORIAL.md)

---

## Related

* mq-bridge: [https://github.com/marcomq/mq-bridge](https://github.com/marcomq/mq-bridge)

*(author note: this example is built on mq-bridge, I am the author of mq-bridge, just for transparency)*

