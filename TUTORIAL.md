**Introduction**

Every web application eventually needs background jobs. Send an email, resize an image, import some data. Most background job libraries in other languages require you to commit to their ecosystem from day one – their queue, their worker format, their retry logic. I wanted something where I could start with a file on disk and scale up when needed, without rewriting my business logic.

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

Let's also add a lib.rs file:
```rust
// src/lib.rs
pub mod jobs;
```

Then we register handlers for each job type using `TypeHandler`:

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
// bin/worker.rs
//...
let route = Route::new(
    Endpoint::new(EndpointType::File(
        FileConfig::new("jobs.jsonl").with_mode(FileConsumerMode::Consume { delete: true }),
    )),
    Endpoint::null(), // No output needed here
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

    // actual stuff
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

When analyzing the file `jobs.jsonl`, we can see that it is empty. This is because we 
delete the line, by using `FileConsumerMode::Consume { delete: true }` on the consumer side.

We would keep the line and just send newly received messages with `delete: false`.
But we would also then send all messages on next start of the consumer.

---

**Step 4: Switch to Json config**

The business logic stays in Rust. The infrastructure moves to config:
`cargo add serde_json`

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
// worker.rs - load route from config instead
let route: Route = serde_json::from_str(include_str!("mq-bridge.json"))?;
let route = route.with_handler(jobs);

route.deploy("job_worker").await?;
```

```rust
// submit.rs - create a publisher from route / config
let route: Route = serde_json::from_str(include_str!("config.json"))?;
let publisher = Publisher::new(route.input).await?;
```

We can load the configuration from file or database. And the code is actually smaller now.

Now you can also change the backend without touching your handler code.

---

**Step 5: Switch to NATS for production**

To have a real job engine, you'll want to run the worker on a separate machine. 
Let's use `nats` for that. It's lightweight – just a single binary, no dependencies.

First, we need to enable feature `nats` in Cargo.toml:

```toml
mq-bridge = { version = "0.2.11", features = ["nats"] }
```

Then, we install nats via brew
```bash
brew install nats-server
# Ubuntu/Debian 
# wget https://github.com/nats-io/nats-server/releases/latest/download/nats-server-linux-amd64.deb
# sudo apt install ./nats-server-linux-amd64.deb
nats-server -js
```

Or run it with docker:

```bash
docker run -p 4222:4222 nats:2.12.2 -js
```

One config change:

```yaml
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

The rest of the code is untouched.
You can now retry to start the worker.rs, it will connect with nats 
and submit.rs will also publish the message to nats.

---

**What you get for free**

Switching to NATS doesn't just give you a real broker. It unlocks everything mq-bridge builds on top:

You can also just add middlewares in the json, for example to define retries for the publisher:

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
            "initial_interval_ms": 100,
            "max_attempts": 3,
            "max_interval_ms": 5000,
            "multiplier": 2
        }
    },
    {
        "dlq": {
            "endpoint": {
                "middlewares": [],
                "file": {
                "format": "normal",
                "path": "error.log"
                }
            }
        }
    }
    ]
}
```

If you are already using mongodb, mysql, mariadb or postgres, you can still 
re-use them to store and forward computation events.
Switching may just be a simple configuration task.

For testing, there is a memory endpoint available. So you don't need to spin
up your database or broker in your unit tests to run tests.

If you have a more simple tasks - these are also available. You may skip the 
whole event handler and route concept and just receive or send to your endpoints with the same API calls.

---

**Conclusion**

Overall, mq-bridge covers more than just background jobs.
You may use it for events or just to receive and send messages from your existing brokers.
Or you could just use it to easily scale up your setup by adding kafka as
buffer or fan out.

Mq-bridge is still a young library. Don't expect it to as complete as for example 
Watermill (go). 
Some concepts like event-stores are missing, as it is mostly just focussed on transport.
And the documentation still needs improvement. This tutorial is a first step 
toward better documentation.

Feedback welcome: https://github.com/marcomq/mq-bridge/issues
