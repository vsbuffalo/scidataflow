use serde_derive::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct DataDryadAPI {
    base_url: String,

    #[serde(skip_serializing)]
    token: String,
}
