use std::any::Any;

use crate::{
    error::Error, tlv::TLVElement, tlv_writer::TLVWriter, transport::session::SessionHandle,
};

#[derive(PartialEq)]
pub enum TransactionState {
    Ongoing,
    Complete,
}
pub struct Transaction<'a, 'b> {
    pub state: TransactionState,
    pub data: Option<Box<dyn Any>>,
    pub session: &'b mut SessionHandle<'a>,
}

#[derive(Debug, Clone, Copy)]
pub struct CmdPathIb {
    pub endpoint: Option<u16>,
    pub cluster: Option<u32>,
    pub command: u16,
}

pub trait InteractionConsumer {
    fn consume_invoke_cmd(
        &self,
        cmd_path_ib: &CmdPathIb,
        data: TLVElement,
        trans: &mut Transaction,
        tlvwriter: &mut TLVWriter,
    ) -> Result<(), Error>;

    fn consume_read_attr(
        &self,
        attr_list: TLVElement,
        fab_scoped: bool,
        tlvwriter: &mut TLVWriter,
    ) -> Result<(), Error>;
}

pub struct InteractionModel {
    consumer: Box<dyn InteractionConsumer>,
}
pub mod command;
pub mod core;
pub mod messages;
pub mod read;
