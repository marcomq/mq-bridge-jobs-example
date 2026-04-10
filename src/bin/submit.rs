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
