use crate::{
    error::*,
    tlv::{self, FromTLV, TLVElement, TLVWriter, TagType, ToTLV},
    transport::{
        proto_demux::{self, ProtoRx, ProtoTx, ResponseRequired},
        session::SessionHandle,
    },
};
use colored::Colorize;
use log::{error, info};
use num;
use num_derive::FromPrimitive;

use super::InteractionConsumer;
use super::InteractionModel;
use super::Transaction;
use super::TransactionState;

/* Handle messages related to the Interation Model
 */

/* Interaction Model ID as per the Matter Spec */
const PROTO_ID_INTERACTION_MODEL: usize = 0x01;

#[derive(FromPrimitive, Debug)]
pub enum OpCode {
    Reserved = 0,
    StatusResponse = 1,
    ReadRequest = 2,
    SubscribeRequest = 3,
    SubscriptResponse = 4,
    ReportData = 5,
    WriteRequest = 6,
    WriteResponse = 7,
    InvokeRequest = 8,
    InvokeResponse = 9,
    TimedRequest = 10,
}

impl<'a, 'b> Transaction<'a, 'b> {
    pub fn new(session: &'b mut SessionHandle<'a>) -> Self {
        Self {
            state: TransactionState::Ongoing,
            data: None,
            session,
        }
    }

    pub fn complete(&mut self) {
        self.state = TransactionState::Complete
    }

    pub fn is_complete(&self) -> bool {
        self.state == TransactionState::Complete
    }
}

impl InteractionModel {
    pub fn new(consumer: Box<dyn InteractionConsumer>) -> InteractionModel {
        InteractionModel { consumer }
    }
}

impl proto_demux::HandleProto for InteractionModel {
    fn handle_proto_id(
        &mut self,
        proto_rx: &mut ProtoRx,
        proto_tx: &mut ProtoTx,
    ) -> Result<ResponseRequired, Error> {
        let mut trans = Transaction::new(&mut proto_rx.session);
        let proto_opcode: OpCode =
            num::FromPrimitive::from_u8(proto_rx.proto_opcode).ok_or(Error::Invalid)?;
        proto_tx.proto_id = PROTO_ID_INTERACTION_MODEL;

        info!("{} {:?}", "Received command".cyan(), proto_opcode);
        tlv::print_tlv_list(proto_rx.buf);
        let result = match proto_opcode {
            OpCode::InvokeRequest => self.handle_invoke_req(&mut trans, proto_rx.buf, proto_tx)?,
            OpCode::ReadRequest => self.handle_read_req(&mut trans, proto_rx.buf, proto_tx)?,
            OpCode::WriteRequest => self.handle_write_req(&mut trans, proto_rx.buf, proto_tx)?,
            _ => {
                error!("Opcode Not Handled: {:?}", proto_opcode);
                return Err(Error::InvalidOpcode);
            }
        };

        if result == ResponseRequired::Yes {
            info!("Sending response");
            tlv::print_tlv_list(proto_tx.write_buf.as_borrow_slice());
        }
        if trans.is_complete() {
            proto_rx.exchange.close();
        }
        Ok(result)
    }

    fn get_proto_id(&self) -> usize {
        PROTO_ID_INTERACTION_MODEL as usize
    }
}

#[derive(FromPrimitive, Debug, Clone, Copy, PartialEq)]
pub enum IMStatusCode {
    Sucess = 0,
    Failure = 1,
    InvalidSubscription = 0x7D,
    UnsupportedAccess = 0x7E,
    UnsupportedEndpoint = 0x7F,
    InvalidAction = 0x80,
    UnsupportedCommand = 0x81,
    InvalidCommand = 0x85,
    UnsupportedAttribute = 0x86,
    ConstraintError = 0x87,
    UnsupportedWrite = 0x88,
    ResourceExhausted = 0x89,
    NotFound = 0x8b,
    UnreportableAttribute = 0x8c,
    InvalidDataType = 0x8d,
    UnsupportedRead = 0x8f,
    DataVersionMismatch = 0x92,
    Timeout = 0x94,
    Busy = 0x9c,
    UnsupportedCluster = 0xc3,
    NoUpstreamSubscription = 0xc5,
    NeedsTimedInteraction = 0xc6,
}

impl From<Error> for IMStatusCode {
    fn from(e: Error) -> Self {
        match e {
            Error::EndpointNotFound => IMStatusCode::UnsupportedEndpoint,
            Error::ClusterNotFound => IMStatusCode::UnsupportedCluster,
            Error::AttributeNotFound => IMStatusCode::UnsupportedAttribute,
            Error::CommandNotFound => IMStatusCode::UnsupportedCommand,
            _ => IMStatusCode::Failure,
        }
    }
}

impl FromTLV<'_> for IMStatusCode {
    fn from_tlv(t: &TLVElement) -> Result<Self, Error> {
        num::FromPrimitive::from_u16(t.u16()?).ok_or(Error::Invalid)
    }
}

impl ToTLV for IMStatusCode {
    fn to_tlv(&self, tw: &mut TLVWriter, tag_type: TagType) -> Result<(), Error> {
        tw.u16(tag_type, *self as u16)
    }
}
