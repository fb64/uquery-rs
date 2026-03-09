use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};

#[derive(Debug, Clone)]
pub struct UQueryError {
    pub(crate) status_code: u16,
    pub(crate) title: String,
    pub(crate) detail: String,
}

impl Serialize for UQueryError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("UQueryError", 3)?;
        state.serialize_field("status", &self.status_code)?;
        state.serialize_field("title", &self.title)?;
        state.serialize_field("detail", &self.detail)?;
        state.end()
    }
}
