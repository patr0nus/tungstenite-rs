//! WebSocket handshake control.

pub mod client;
pub mod headers;
pub mod server;

mod machine;

use std::error::Error as ErrorTrait;
use std::fmt;
use std::io::{Read, Write};

use base64;
use sha1::{Digest, Sha1};

use self::machine::{HandshakeMachine, RoundResult, StageResult, TryParse};
use crate::error::Error;

/// A WebSocket handshake.
#[derive(Debug)]
pub struct MidHandshake<Role: HandshakeRole> {
    role: Role,
    machine: HandshakeMachine<Role::InternalStream>,
}

impl<Role: HandshakeRole> MidHandshake<Role> {
    /// Allow access to machine
    pub fn get_ref(&self) -> &HandshakeMachine<Role::InternalStream> {
        &self.machine
    }

    /// Allow mutable access to machine
    pub fn get_mut(&mut self) -> &mut HandshakeMachine<Role::InternalStream> {
        &mut self.machine
    }

    /// Restarts the handshake process.
    pub fn handshake(mut self) -> Result<Role::FinalResult, HandshakeError<Role>> {
        let mut mach = self.machine;
        loop {
            mach = match mach.single_round()? {
                RoundResult::WouldBlock(m) => {
                    return Err(HandshakeError::Interrupted(MidHandshake {
                        machine: m,
                        ..self
                    }))
                }
                RoundResult::Incomplete(m) => m,
                RoundResult::StageFinished(s) => match self.role.stage_finished(s)? {
                    ProcessingResult::Continue(m) => m,
                    ProcessingResult::Done(result) => return Ok(result),
                },
            }
        }
    }
}

/// A handshake result.
pub enum HandshakeError<Role: HandshakeRole> {
    /// Handshake was interrupted (would block).
    Interrupted(MidHandshake<Role>),
    /// Handshake failed.
    Failure(Error),
}

impl<Role: HandshakeRole> fmt::Debug for HandshakeError<Role> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            HandshakeError::Interrupted(_) => write!(f, "HandshakeError::Interrupted(...)"),
            HandshakeError::Failure(ref e) => write!(f, "HandshakeError::Failure({:?})", e),
        }
    }
}

impl<Role: HandshakeRole> fmt::Display for HandshakeError<Role> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            HandshakeError::Interrupted(_) => write!(f, "Interrupted handshake (WouldBlock)"),
            HandshakeError::Failure(ref e) => write!(f, "{}", e),
        }
    }
}

impl<Role: HandshakeRole> ErrorTrait for HandshakeError<Role> {}

impl<Role: HandshakeRole> From<Error> for HandshakeError<Role> {
    fn from(err: Error) -> Self {
        HandshakeError::Failure(err)
    }
}

/// Handshake role.
pub trait HandshakeRole {
    #[doc(hidden)]
    type IncomingData: TryParse;
    #[doc(hidden)]
    type InternalStream: Read + Write;
    #[doc(hidden)]
    type FinalResult;
    #[doc(hidden)]
    fn stage_finished(
        &mut self,
        finish: StageResult<Self::IncomingData, Self::InternalStream>,
    ) -> Result<ProcessingResult<Self::InternalStream, Self::FinalResult>, Error>;
}

/// Stage processing result.
#[doc(hidden)]
#[derive(Debug)]
pub enum ProcessingResult<Stream, FinalResult> {
    Continue(HandshakeMachine<Stream>),
    Done(FinalResult),
}

/// Turns a Sec-WebSocket-Key into a Sec-WebSocket-Accept.
fn convert_key(input: &[u8]) -> Result<String, Error> {
    // ... field is constructed by concatenating /key/ ...
    // ... with the string "258EAFA5-E914-47DA-95CA-C5AB0DC85B11" (RFC 6455)
    const WS_GUID: &[u8] = b"258EAFA5-E914-47DA-95CA-C5AB0DC85B11";
    let mut sha1 = Sha1::default();
    sha1.update(input);
    sha1.update(WS_GUID);
    Ok(base64::encode(&sha1.finalize()))
}

#[cfg(test)]
mod tests {
    use super::convert_key;

    #[test]
    fn key_conversion() {
        // example from RFC 6455
        assert_eq!(
            convert_key(b"dGhlIHNhbXBsZSBub25jZQ==").unwrap(),
            "s3pPLMBiTxaQ9kYGzzhZRbK+xOo="
        );
    }
}
