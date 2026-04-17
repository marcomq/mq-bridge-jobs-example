**Switching from file-based jobs to NATS/Kafka in Rust without changing code**

**Introduction**

Most applications eventually need to offload work to another process. Parse a file, send an email, trigger a report – tasks that shouldn't block your main service and might even run on a different machine.

Most queue or stream solutions require you to commit to their ecosystem from day one – their broker, their worker format, their retry logic. And if you want to swap the transport later, due to a license change or some request from a good paying customer, you're rewriting business logic.

I wanted something simpler: define your jobs once in plain Rust structs, start with a file during development, and switch to a real broker for production – without touching the handler code.

This is how I built a remote job system in Rust using [mq-bridge](https://github.com/marcomq/mq-bridge).


**Step 1: Create Cargo.toml**

Let's run `cargo init`, `cargo add mq-bridge serde tokio tracing tracing-subscriber` and some other modifications:

```toml
# Cargo.toml
[package]
name = "mq-bridge-jobs-example"
version = "0.1.0"
edition = "2021"

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

We want 2 separate binaries: `worker` that waits for tasks and `submit` that sends a single mail, which should be received by our worker.

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

In addition, we define strings to identify each struct:

```rust
// src/jobs.rs
impl SendEmail {
    pub const KIND: &'static str = "send_email";
}

impl GenerateReport {
    pub const KIND: &'static str = "generate_report";
}
```

Let's also add a `lib.rs` file:

```rust
// src/lib.rs
pub mod jobs;
```

Then we register handlers for each job type using mq-bridge `TypeHandler`:

```rust
// src/bin/worker.rs
let jobs = TypeHandler::new()
    .add(SendEmail::KIND, |job: SendEmail| async move {
        // We are not actually sending a mail here - just print a log message
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
// src/bin/worker.rs
//...
let route = Route::new(
    Endpoint::new(EndpointType::File(
        FileConfig::new("jobs.jsonl").with_mode(FileConsumerMode::Consume { delete: true }),
    )),
    Endpoint::null(), // No output needed here
).with_handler(jobs);

route.deploy("job_worker").await?;
```

Together with logging and everything, the complete `worker.rs` now looks like this:

```rust
// src/bin/worker.rs (complete)
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

    tracing::info!("Worker running — press Ctrl-C to exit");
    tokio::signal::ctrl_c().await?;
    tracing::info!("Shutting down");
    Ok(())
}
```

To submit a job, just append a new line to `jobs.jsonl` in our `submit.rs`:

```rust
// src/bin/submit.rs
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

Works completely offline. Great for development and testing.

Now let's test it. Open a first shell:

```bash
cargo run --bin worker
```

The worker is now running and waiting for file modifications. In a second shell, submit a job:

```bash
cargo run --bin submit
```

The worker will receive the task and print:
```plaintext
INFO worker: Sending email to user@example.com
```


Instead of using the submit binary, you could also just simply push a new line to the file
```shell
echo '{"message_id":1,"payload":{"subject":"Welcome!","to":"user@example.com"},"metadata":{"kind":"send_mail"}}' > jobs.jsonl
```
Afterwards `jobs.jsonl` is empty — because `FileConsumerMode::Consume { delete: true }` removes consumed lines. With `delete: false`, lines would be kept and replayed on the next worker start.


There is alternatively a `GroupSubscribe` mode to prevent re-deliver by tracking the current offset via separate `.offset` file, without deleting lines.

---

**Step 4: Switch to JSON config**

The business logic stays in Rust. The infrastructure moves to config:

```bash
cargo add serde_json
```

`src/bin/config.json`
```json
{
  "input": {
    "file": {
      "path": "jobs.jsonl",
      "delete": true,
      "mode": "consume"
    }
  },
  "output": {
    "null": null
  }
}
```

```rust
// src/bin/worker.rs - load route from config
let route: Route = serde_json::from_str(include_str!("config.json"))?;
let route = route.with_handler(jobs);
route.deploy("job_worker").await?;
```

```rust
// src/bin/submit.rs - create a publisher from the same config
let route: Route = serde_json::from_str(include_str!("config.json"))?;
let publisher = Publisher::new(route.input).await?;
```

You can now load the configuration from a file or database. The code is smaller, and you can change the backend without touching your handler code.


In a later production scenario, you might also want to use a separate publisher configuration. The are properties that are only available for consumers or publishers and there would be a warning when using invalid settings. Also, you might want to configure a specific kafka group_id or use separate topics for fan out.
But for this example, using a common NATS configuration works fine.

---

**Step 5: Switch to NATS for production**

To run the worker on a separate machine, you'll want a broker or database. NATS is a great fit — it's lightweight, just a single binary with no dependencies and stores messages.

First, enable the `nats` feature in `Cargo.toml`:

```toml
mq-bridge = { version = "0.2.11", features = ["nats"] }
```

This just enables the "nats" feature. We can simply re-run the previous
example. Nothing changes yet, still using file, it just needs longer to compile.

Start NATS with JetStream:

```bash
# macOS
brew install nats-server && nats-server -js

# or Ubuntu/Debian
wget https://github.com/nats-io/nats-server/releases/latest/download/nats-server-linux-amd64.deb
sudo apt install ./nats-server-linux-amd64.deb && nats-server -js

# or Docker
docker run -p 4222:4222 nats:2.12.2 -js
```

One config.json file change, no code changes:

```json
{
  "input": {
    "nats": {
      "url": "nats://localhost:4222",
      "subject": "test-stream.pipeline",
      "stream": "test-stream"
    }
  },
  "output": {
    "null": null
  }
}
```

Restart worker and submit — both now talk to NATS. The handler code is untouched.

---

**What you get for free**

Switching to NATS unlocks everything mq-bridge builds on top. You can add middlewares in the config, for example retries and a dead-letter queue (DLQ) for failed messages:

```json
{
  "nats": {
    "url": "nats://localhost:4222",
    "subject": "test-stream.pipeline",
    "stream": "test-stream"
  },
  "middlewares": [
    {
      "retry": {
        "max_attempts": 3,
        "max_interval_ms": 5000,
        "initial_interval_ms": 100,
        "multiplier": 2
      }
    },
    {
      "dlq": {
        "endpoint": {
          "file": {
            "path": "error.log"
          }
        }
      }
    }
  ]
}
```

The `retry` middleware will retry failed deliveries with exponential backoff. If all attempts are exhausted, the `dlq` middleware writes the message to `error.log` instead of dropping it silently.

If you are already using MongoDB, MySQL, MariaDB, or PostgreSQL, you can use them as your queue backend as well — just a config change.


If you just want message forwarding from one endpoint to another or an UI to 
create different json configs, you can also use 
mq-bridge-app (cargo install mq-bridge-app).
The code also shows how you would use mq-bridge as webserver.

If you just need a simple send and receive - this is also available. You may skip the 
whole event handler and route concept and just use the same API calls for Http, gRPC, MongoDb, Kafka, RabbitMQ and NATS. They all have the same `receive` and `publish` method and use the same message struct `CanonicalMessage` for transport.

---

**Step 6: Testing with the memory endpoint**

Because mq-bridge uses the same trait for all backends, you can test your handlers without any broker or file system — just an in-memory channel.

Let's add a test for submit.rs:

```rust

// src/bin/submit.rs
use mq_bridge::{msg, Publisher, Route};
use mq_bridge_jobs_example::jobs::SendEmail;

async fn send_mail(publisher: Publisher) -> Result<(), Box<dyn std::error::Error>> {
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new("info"))
        .init();

    let route: Route = serde_json::from_str(include_str!("config.json"))?;
    let publisher = Publisher::new(route.input).await?;
    send_mail(publisher).await
}
#[cfg(test)]
mod tests {
    use mq_bridge::endpoints::memory::MemoryConsumer;
    use mq_bridge::traits::MessageConsumer;
    use mq_bridge::Publisher;
    use mq_bridge::models::Endpoint;
    use mq_bridge_jobs_example::jobs::SendEmail;

    use crate::send_mail;

    #[tokio::test]
    async fn test_submit_sends_email_job() {
        let topic = "test-submit";
        let mut consumer = MemoryConsumer::new_local(topic, 10);
        let publisher = Publisher::new(Endpoint::new_memory(topic, 10)).await.unwrap();
        send_mail(publisher).await.unwrap();
        let received = consumer.receive().await.unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&received.message.payload).unwrap();
        assert_eq!(payload["to"], "user@example.com");
        assert_eq!(received.message.metadata["kind"], SendEmail::KIND);
    }
}

```

No broker, no file, no test containers. The same `TypeHandler` that runs in production is tested here — only the transport is swapped.


**What not to expect**

Not all aspects and features of brokers or databases are supported. Some features 
are emulated, other features may not be implemented yet. Don't expect a full grown
framework that guides you on how to do stuff or already prevents misconfiguration
during compile time when reading configs during runtime.

---

**Conclusion**

mq-bridge covers more than just remote jobs. You can use it for events, or to send and receive messages from existing brokers. And you can scale up by adding Kafka as a buffer or fan-out layer — again, just config.

mq-bridge is still a young library. Don't expect it to be as complete as Watermill (Go)  or Java Spring. It uses some of their concepts, but it doesn't try to be the same — event sourcing and aggregate management are out of scope for now, as the focus is on transport. Documentation is still growing, and this tutorial is a first step toward that.


This tutorial is available here:
https://github.com/marcomq/mq-bridge-jobs-example

The mq-bridge library is available here:
(I’m the author of mq-bridge, just for transparency.)
https://github.com/marcomq/mq-bridge

Feedback and contributions welcome.
