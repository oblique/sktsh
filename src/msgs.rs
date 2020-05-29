use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub enum ServerMsg<'a> {
    Data(&'a [u8]),
    SetDimensions(TermDimensions),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TermDimensions {
    pub rows: u16,
    pub columns: u16,
}
