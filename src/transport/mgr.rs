use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use crate::error::*;
use crate::transport::packet;
use crate::transport::proto_msg;
use crate::transport::session;
use crate::transport::udp;
use crate::utils::ParseBuf;

#[derive(Default)]
pub struct RxCtx {
    src: Option<std::net::SocketAddr>,
    len: usize,
    plain_hdr: packet::PlainHdr,
}

pub struct TxCtx {

}

pub struct Mgr {
    transport: udp::UdpListener,
    sess_mgr:  session::SessionMgr,
}

/* Currently matches with the one in connectedhomeip repo */
const MAX_BUF_SIZE: usize = 1583;

impl Mgr {
    pub fn new() -> Result<Mgr, Error> {
        let mut mgr = Mgr{
            transport: udp::UdpListener::new()?,
            sess_mgr: session::SessionMgr::new(),
        };

        // Create a fake entry as hard-coded in the 'bypass mode' in chip-tool
        let test_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
        let i2r_key = [ 0x44, 0xd4, 0x3c, 0x91, 0xd2, 0x27, 0xf3, 0xba, 0x08, 0x24, 0xc5, 0xd8, 0x7c, 0xb8, 0x1b, 0x33];
        mgr.sess_mgr.add(0, i2r_key, i2r_key, test_addr.ip()).unwrap();

        Ok(mgr)
    }

    pub fn start(&self) -> Result<(), Error>{
        /* I would have liked this in .bss instead of the stack, will likely move this later */
        let mut in_buf: [u8; MAX_BUF_SIZE] = [0; MAX_BUF_SIZE];


        loop {
            let mut rx_ctx = RxCtx::default();

            // Read from the transport
            let (len, src) = self.transport.recv(&mut in_buf)?;
            rx_ctx.len = len;
            rx_ctx.src = Some(src);
            let mut parse_buf = ParseBuf::new(&mut in_buf, len);

            // Read unencrypted packet header
            match packet::parse_plain_hdr(&mut parse_buf) {
                Ok(h) => rx_ctx.plain_hdr = h,
                Err(_) => continue,
            }

            // Get session
            // Ok to use unwrap here since we know 'src' is certainly not None
            let session = match self.sess_mgr.get(rx_ctx.plain_hdr.sess_id, rx_ctx.src.unwrap().ip()) {
                Some(a) => a,
                None => { continue; },
            };
            
            // Read encrypted header
            match proto_msg::parse_enc_hdr(&rx_ctx.plain_hdr, &mut parse_buf, &session.dec_key) {
                Ok(_) => (),
                Err(_) => continue,
            }
        }
    }
}