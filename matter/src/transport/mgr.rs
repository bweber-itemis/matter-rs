use heapless::LinearMap;
use log::{debug, error, info, trace};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use crate::error::*;
use crate::proto_demux;
use crate::proto_demux::ProtoRx;
use crate::proto_demux::ProtoTx;
use crate::transport::exchange;
use crate::transport::mrp;
use crate::transport::plain_hdr;
use crate::transport::proto_hdr;
use crate::transport::session;
use crate::transport::udp;
use crate::utils::parsebuf::ParseBuf;
use colored::*;

use super::session::Session;

// Currently matches with the one in connectedhomeip repo
const MAX_RX_BUF_SIZE: usize = 1583;

pub struct Mgr {
    transport: udp::UdpListener,
    sess_mgr: session::SessionMgr,
    exch_mgr: exchange::ExchangeMgr,
    proto_demux: proto_demux::ProtoDemux,
    rel_mgr: mrp::ReliableMessage,
}

impl Mgr {
    pub fn new() -> Result<Mgr, Error> {
        let mut mgr = Mgr {
            transport: udp::UdpListener::new()?,
            sess_mgr: session::SessionMgr::new(),
            proto_demux: proto_demux::ProtoDemux::new(),
            exch_mgr: exchange::ExchangeMgr::new(),
            rel_mgr: mrp::ReliableMessage::new(),
        };

        // Create a fake entry as hard-coded in the 'bypass mode' in chip-tool
        let test_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 5541);
        let i2r_key = [
            0x44, 0xd4, 0x3c, 0x91, 0xd2, 0x27, 0xf3, 0xba, 0x08, 0x24, 0xc5, 0xd8, 0x7c, 0xb8,
            0x1b, 0x33,
        ];
        let r2i_key = [
            0xac, 0xc1, 0x8f, 0x06, 0xc7, 0xbc, 0x9b, 0xe8, 0x24, 0x6a, 0x67, 0x8c, 0xb1, 0xf8,
            0xba, 0x3d,
        ];

        let (_, session) = mgr.sess_mgr.add(test_addr).unwrap();
        session.activate(&i2r_key, &r2i_key, 0).unwrap();
        session.cheat_set_zero_local_sess_id();

        Ok(mgr)
    }

    // Allows registration of different protocols with the Transport/Protocol Demux
    pub fn register_protocol(
        &mut self,
        proto_id_handle: Box<dyn proto_demux::HandleProto>,
    ) -> Result<(), Error> {
        self.proto_demux.register(proto_id_handle)
    }

    // Borrow-checker gymnastics
    fn recv<'a>(
        transport: &mut udp::UdpListener,
        rel_mgr: &mut mrp::ReliableMessage,
        sess_mgr: &'a mut session::SessionMgr,
        exch_mgr: &'a mut exchange::ExchangeMgr,
        in_buf: &'a mut [u8],
    ) -> Result<ProtoRx<'a>, Error> {
        let mut plain_hdr = plain_hdr::PlainHdr::default();
        let mut proto_hdr = proto_hdr::ProtoHdr::default();

        // Read from the transport
        let (len, src) = transport.recv(in_buf)?;
        let mut parse_buf = ParseBuf::new(in_buf, len);

        info!("{} from src: {}", "Received".blue(), src);
        trace!("payload: {:x?}", parse_buf.as_borrow_slice());

        // Get session
        //      Ok to use unwrap here since we know 'src' is certainly not None
        let (_, session) = sess_mgr.recv(&mut plain_hdr, &mut parse_buf, src)?;

        // Read encrypted header
        session.recv(&plain_hdr, &mut proto_hdr, &mut parse_buf)?;

        // Get the exchange
        let exchange = exch_mgr.recv(&plain_hdr, &proto_hdr)?;
        debug!("Exchange is {:?}", exchange);

        // Message Reliability Protocol
        rel_mgr.recv(plain_hdr.sess_id, exchange, &plain_hdr, &proto_hdr)?;

        Ok(ProtoRx::new(
            proto_hdr.proto_id.into(),
            proto_hdr.proto_opcode,
            session,
            exchange,
            src,
            parse_buf.as_slice(),
        ))
    }

    fn send_to_exchange_id(
        &mut self,
        sess_id: u16,
        exch_id: u16,
        proto_tx: &mut ProtoTx,
    ) -> Result<(), Error> {
        let session = self.sess_mgr.get_with_id(sess_id).ok_or(Error::NoSession)?;
        let exchange = self
            .exch_mgr
            .get_with_id(sess_id, exch_id)
            .ok_or(Error::NoExchange)?;

        proto_tx.peer = session.get_peer_addr().ok_or(Error::InvalidPeerAddr)?;
        Mgr::send_to_exchange(
            &self.transport,
            &mut self.rel_mgr,
            session,
            exchange,
            proto_tx,
        )
    }

    // This function is send_to_exchange(). There will be multiple higher layer send_*() functions
    // all of them will eventually call send_to_exchange() after creating the necessary session and exchange
    // objects
    fn send_to_exchange(
        transport: &udp::UdpListener,
        rel_mgr: &mut mrp::ReliableMessage,
        session: &mut Session,
        exchange: &mut exchange::Exchange,
        proto_tx: &mut ProtoTx,
    ) -> Result<(), Error> {
        let mut plain_hdr = plain_hdr::PlainHdr::default();
        let mut proto_hdr = proto_hdr::ProtoHdr::default();

        trace!("payload: {:x?}", proto_tx.write_buf.as_slice());
        proto_hdr.proto_id = proto_tx.proto_id as u16;
        proto_hdr.proto_opcode = proto_tx.proto_opcode;

        exchange.send(&mut proto_hdr)?;

        session.pre_send(&mut plain_hdr)?;

        rel_mgr.pre_send(
            session.get_local_sess_id(),
            exchange,
            proto_tx.reliable,
            &plain_hdr,
            &mut proto_hdr,
        )?;

        session.send(&mut plain_hdr, &mut proto_hdr, &mut proto_tx.write_buf)?;

        transport.send(proto_tx.write_buf.as_slice(), proto_tx.peer)?;
        Ok(())
    }

    fn handle_rxtx(
        &mut self,
        in_buf: &mut [u8],
        proto_tx: &mut ProtoTx,
    ) -> Result<Option<Session>, Error> {
        let mut proto_rx = Mgr::recv(
            &mut self.transport,
            &mut self.rel_mgr,
            &mut self.sess_mgr,
            &mut self.exch_mgr,
            in_buf,
        )
        .map_err(|e| {
            error!("Error in recv: {:?}", e);
            e
        })?;

        // Proto Dispatch
        match self.proto_demux.handle(&mut proto_rx, proto_tx) {
            Ok(r) => {
                if let proto_demux::ResponseRequired::No = r {
                    // We need to send the Ack if reliability is enabled, in this case
                    return Ok(None);
                }
            }
            Err(e) => {
                error!("Error in proto_demux {:?}", e);
                return Err(e);
            }
        }
        // Check if a new session was created as part of the protocol handling
        let new_session = proto_tx.new_session.take();

        proto_tx.peer = proto_rx.peer;
        // tx_ctx now contains the response payload, send the packet
        Mgr::send_to_exchange(
            &self.transport,
            &mut self.rel_mgr,
            proto_rx.session,
            proto_rx.exchange,
            proto_tx,
        )
        .map_err(|e| {
            error!("Error in sending msg {:?}", e);
            e
        })?;

        Ok(new_session)
    }

    pub fn start(&mut self) -> Result<(), Error> {
        loop {
            // I would have liked this in .bss instead of the stack, will likely move this
            // later when we convert this into a pool
            const RESERVE_HDR_SIZE: usize =
                plain_hdr::max_plain_hdr_len() + proto_hdr::max_proto_hdr_len();
            let mut in_buf: [u8; MAX_RX_BUF_SIZE] = [0; MAX_RX_BUF_SIZE];
            let mut out_buf: [u8; MAX_RX_BUF_SIZE] = [0; MAX_RX_BUF_SIZE];
            let mut proto_tx = match ProtoTx::new(&mut out_buf, RESERVE_HDR_SIZE) {
                Ok(p) => p,
                Err(e) => {
                    error!("Error creating proto_tx {:?}", e);
                    continue;
                }
            };

            // Handle network operations
            if let Ok(Some(new_session)) = self.handle_rxtx(&mut in_buf, &mut proto_tx) {
                // If a new session was created, add it
                let _ = self
                    .sess_mgr
                    .add_session(new_session)
                    .map_err(|e| error!("Error adding new session {:?}", e));
            }
            proto_tx.reset(RESERVE_HDR_SIZE);

            // Handle any pending acknowledgement send
            let mut acks_to_send: LinearMap<(u16, u16), (), { mrp::MAX_MRP_ENTRIES }> =
                LinearMap::new();
            self.rel_mgr.get_acks_to_send(&mut acks_to_send);
            for (sess_id, exch_id) in acks_to_send.keys() {
                info!(
                    "Sending MRP Standalone ACK for sess {} exch {}",
                    sess_id, exch_id
                );
                self.rel_mgr.prepare_ack(*sess_id, *exch_id, &mut proto_tx);
                if let Err(e) = self.send_to_exchange_id(*sess_id, *exch_id, &mut proto_tx) {
                    error!("Error in sending Ack {:?}", e);
                }
            }

            // Handle exchange purging
            //    This need not be done in each turn of the loop, maybe once in 5 times or so?
            self.exch_mgr.purge();

            info!("Session Mgr: {}", self.sess_mgr);
            info!("Exchange Mgr: {}", self.exch_mgr);
        }
    }
}