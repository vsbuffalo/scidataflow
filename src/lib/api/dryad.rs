use serde_derive::{Serialize,Deserialize};

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct DataDryadAPI {
    base_url: String,

    #[serde(skip_serializing)]
    token: String
}


