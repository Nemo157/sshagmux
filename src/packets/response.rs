use bytes::{BufMut, BytesMut};
use eyre::Error;

const SSH_AGENT_FAILURE: u8 = 5;
const SSH_AGENT_SUCCESS: u8 = 6;
/*
const SSH_AGENT_EXTENSION_FAILURE: u8 = 28;
const SSH_AGENT_IDENTITIES_ANSWER: u8 = 12;
const SSH_AGENT_SIGN_RESPONSE: u8 = 14;
*/

#[derive(Debug)]
#[allow(dead_code)] // some variants are unused
pub(crate) enum Response {
    Success,
    Failure,
}

impl super::Encode for Response {
    #[fehler::throws]
    fn encode_to(self, dst: &mut BytesMut) {
        match self {
            Response::Success => {
                dst.put_u8(SSH_AGENT_SUCCESS);
            }
            Response::Failure => {
                dst.put_u8(SSH_AGENT_FAILURE);
            }
        }
    }

    fn encoded_length_estimate(&self) -> usize {
        match self {
            Response::Success | Response::Failure => 1,
        }
    }
}
