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
    // init logging
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new("info"))
        .init();

    // actual stuff
    let route: Route = serde_json::from_str(include_str!("config.json"))?;
    let publisher = Publisher::new(route.input).await?;
    send_mail(publisher).await
}
#[cfg(test)]
mod tests {
    use mq_bridge::endpoints::memory::MemoryConsumer;
    use mq_bridge::models::Endpoint;
    use mq_bridge::traits::MessageConsumer;
    use mq_bridge::Publisher;
    use mq_bridge_jobs_example::jobs::SendEmail;

    use crate::send_mail;

    #[tokio::test]
    async fn test_submit_sends_email_job() {
        let topic = "test-submit";
        let mut consumer = MemoryConsumer::new_local(topic, 10);
        let publisher = Publisher::new(Endpoint::new_memory(topic, 10))
            .await
            .unwrap();
        send_mail(publisher).await.unwrap();
        let received = consumer.receive().await.unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&received.message.payload).unwrap();
        assert_eq!(payload["to"], "user@example.com");
        assert_eq!(received.message.metadata["kind"], SendEmail::KIND);
    }
}
