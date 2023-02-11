use std::{
    fmt::Debug,
    io::{Read, Write},
};

pub trait NetworkMessage
where
    Self: Sized,
{
    type Error: Debug;

    fn serialize<W: Write>(&self, writer: W) -> Result<(), Self::Error>;

    fn deserialize<R: Read>(reader: R) -> Result<Self, Self::Error>;
}
