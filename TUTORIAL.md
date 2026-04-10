**Introduction**

Every web application eventually needs background jobs. Send an email, resize an image, import some data. The usual solutions come with a lot of baggage: Redis as a dependency, a separate worker process, a framework that wants to own your architecture.

I wanted something simpler. Start with a file during development, switch to a real broker for production – without changing my business logic.

This is how I built a background job system in Rust using [mq-bridge](https://github.com/marcomq/mq-bridge).

**Step 1: Create Cargo.toml**
Let's run `cargo init`, `cargo add mq-bridge serde tokio tracing tracing-subscriber` and some other modifications: 

```toml
# Cargo.toml
[package]
name = "mq-bridge-jobs-example"
version = "0.1.0"
edition = "2024"

[dependencies]
mq-bridge = "0.2.11"
serde = { version = "1.0.228", features = ["derive"] }
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
tracing = "0.1.44"
tracing-subscriber = { version = "0.3.23", features = ["env-filter"] }

[[bin]]
name = "worker"
path = "src/bin/worker.rs"

[[bin]]
name = "submit"
path = "src/bin/submit.rs"
```

We want 2 separate binaries: `worker` that waits for tasks and `submit` that is 
sending a single mail, which should be received by our worker.

---

**Step 2: Define your jobs**

Before we touch any infrastructure, we define what our jobs look like. Just plain Rust structs:

```rust
// src/jobs.rs
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize)]
pub struct SendEmail {
    pub to: String,
    pub subject: String,
}

#[derive(Serialize, Deserialize)]
pub struct GenerateReport {
    pub user_id: u32,
}
```
In addition, we should also define strings to identify each struct:
```rust
// src/jobs.rs
impl SendEmail {
    // We just need some string as identifier for TypeHandler
    pub const KIND: &'static str = "send_email";
}

impl GenerateReport {
    pub const KIND: &'static str = "generate_report";
}
```

Then we register handlers for each job type using `TypeHandler`:

```rust
// src/bin/worker.rs 
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
```

---

**Step 3: Start with a file backend**

No Docker. No broker. Just a file on disk for our worker.

```rust
// bin/worker.rs
//...
let route = Route::new(
    Endpoint::new_file("jobs.jsonl"),
    Endpoint::new_memory("results", 100),
).with_handler(jobs);

route.deploy("job_worker").await?;
```

Together with logging and everything, the complete file  worker.rs now looks like this:
```rust
// bin/worker.rs (complete)
use mq_bridge::{
    Handled, Route,
    models::{Endpoint, EndpointType, FileConfig, FileConsumerMode},
    type_handler::TypeHandler,
};
use mq_bridge_jobs_example::jobs::{GenerateReport, SendEmail};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // init logging
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

    tracing::info!("Worker running — press Ctrl-C to exit");
    tokio::signal::ctrl_c().await?;
    tracing::info!("Shutting down");
    Ok(())
}
```

To submit a job, just append a line to `jobs.jsonl` in our submit.rs:

```rust
// bin/submit.rs
use mq_bridge::{
    Publisher,
    models::{Endpoint, EndpointType, FileConfig},
    msg,
};
use mq_bridge_jobs_example::jobs::SendEmail;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // init logging
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new("info"))
        .init();
    
    // actual stuff
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

Works completely offline. Great for development and testing.


Now, let's test it and open our first shell:
`cargo run --bin worker`

We are now having the worker running. It is waiting for file modifications.
Let's now submit a "SendEmail" in another shell:
`cargo run --bin submit`

The worker will receive the task and print a new log message `INFO worker: Sending email to user@example.com`

---