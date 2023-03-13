use std::error::Error;

use serde::{Deserialize, Serialize};
use yojimbo::message::NetworkMessage;

pub const SPECIAL_MESSAGE_STRING: &str = "server got the special message";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TestMessage {
    Int(i32),
    Float(f32),
    Struct(TestMessageStruct),
}

impl NetworkMessage for TestMessage {
    type Error = Box<dyn Error>;

    fn serialize<W: std::io::Write>(&self, writer: W) -> Result<(), Self::Error> {
        Ok(bincode::serialize_into(writer, self)?)
    }

    fn deserialize<R: std::io::Read>(reader: R) -> Result<TestMessage, Self::Error> {
        Ok(bincode::deserialize_from(reader)?)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestMessageStruct {
    pub value: String,
    pub supplmentary_value: i32,
}
