use std::{
    fmt::Debug,
    io::{Read, Write},
};

/// A message that can be sent and received from the network.
///
/// NOTE: Clone should be a temporary requirement. This is a stop-gap solution
/// that simplifies porting reliable channels; I have a design in mind that
/// should eliminate the Clone requirement but want to get it working first.
pub trait NetworkMessage: Clone + 'static
where
    Self: Sized,
{
    type Error: Debug;

    fn serialize<W: Write>(&self, writer: W) -> Result<(), Self::Error>;

    fn deserialize<R: Read>(reader: R) -> Result<Self, Self::Error>;
}
