use std::error::Error;

use rust_game_networking::message::NetworkMessage;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub enum TestMessage {
    Int(i32),
    Float(f32),
    String(TestMessageStruct),
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

#[derive(Debug, Serialize, Deserialize)]
pub struct TestMessageStruct {
    pub value: String,
    pub supplmentary_value: i32,
}
