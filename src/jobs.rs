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
