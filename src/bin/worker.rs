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

    // wait for ctrl + c
    tracing::info!("Worker running — press Ctrl-C to exit");
    tokio::signal::ctrl_c().await?;
    tracing::info!("Shutting down");
    Ok(())
}
