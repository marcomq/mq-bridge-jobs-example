# mq-bridge-jobs-example
Example code for mq-bridge

This is the source code for the [tutorial](TUTORIAL.md)

Runs a job worker example with nats.

Start nats
```bash
docker run -p 4222:4222 nats:2.12.2 -js
```

Open worker in a  first shell:

```bash
cargo run --bin worker
```

The worker is now running and waiting for file modifications. In a second shell, submit a job:

```bash
cargo run --bin submit
```

Worker is receiving a message and prints a log.

You can also switch back to `file` endpoint by 
modifying the content of `src/bin/config.json` and replace it with the content of `src/bin/file_config.json` 