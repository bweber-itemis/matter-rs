use core::fmt;
use std::{
    any::Any,
    net::SocketAddr,
    ops::{Deref, DerefMut},
};

use crate::{
    error::*,
    transport::{plain_hdr, proto_hdr},
    utils::{parsebuf::ParseBuf, writebuf::WriteBuf},
};
use log::{info, trace};

use super::{plain_hdr::PlainHdr, proto_hdr::ProtoHdr};

const MATTER_AES128_KEY_SIZE: usize = 16;

#[derive(Debug, PartialEq, Copy, Clone)]
pub enum SessionMode {
    // The Case session will capture the local fabric index
    Case(u8),
    Pase,
    PlainText,
}

impl Default for SessionMode {
    fn default() -> Self {
        SessionMode::PlainText
    }
}

#[derive(Debug)]
pub struct Session {
    peer_addr: std::net::SocketAddr,
    // I find the session initiator/responder role getting confused with exchange initiator/responder
    // So, we might keep this as enc_key and dec_key for now
    dec_key: [u8; MATTER_AES128_KEY_SIZE],
    enc_key: [u8; MATTER_AES128_KEY_SIZE],
    att_challenge: [u8; MATTER_AES128_KEY_SIZE],
    local_sess_id: u16,
    peer_sess_id: u16,
    msg_ctr: u32,
    mode: SessionMode,
    data: Option<Box<dyn Any>>,
}

#[derive(Debug)]
pub struct CloneData {
    pub dec_key: [u8; MATTER_AES128_KEY_SIZE],
    pub enc_key: [u8; MATTER_AES128_KEY_SIZE],
    pub att_challenge: [u8; MATTER_AES128_KEY_SIZE],
    local_sess_id: u16,
    peer_sess_id: u16,
    mode: SessionMode,
}
impl CloneData {
    pub fn new(peer_sess_id: u16, local_sess_id: u16, mode: SessionMode) -> CloneData {
        CloneData {
            dec_key: [0; MATTER_AES128_KEY_SIZE],
            enc_key: [0; MATTER_AES128_KEY_SIZE],
            att_challenge: [0; MATTER_AES128_KEY_SIZE],
            peer_sess_id,
            local_sess_id,
            mode,
        }
    }
}

impl Session {
    pub fn new(peer_addr: std::net::SocketAddr) -> Session {
        Session {
            peer_addr: peer_addr,
            dec_key: [0; MATTER_AES128_KEY_SIZE],
            enc_key: [0; MATTER_AES128_KEY_SIZE],
            att_challenge: [0; MATTER_AES128_KEY_SIZE],
            peer_sess_id: 0,
            local_sess_id: 0,
            msg_ctr: 1,
            mode: SessionMode::PlainText,
            data: None,
        }
    }

    // A new encrypted session always clones from a previous 'new' session
    pub fn clone(&mut self, clone_from: &CloneData) -> Session {
        let session = Session {
            peer_addr: self.peer_addr,
            dec_key: clone_from.dec_key,
            enc_key: clone_from.enc_key,
            att_challenge: clone_from.att_challenge,
            local_sess_id: clone_from.local_sess_id,
            peer_sess_id: clone_from.peer_sess_id,
            msg_ctr: 1,
            mode: clone_from.mode,
            data: None,
        };
        session
    }

    pub fn set_data(&mut self, data: Box<dyn Any>) {
        self.data = Some(data);
    }

    pub fn clear_data(&mut self) {
        self.data = None;
    }

    pub fn get_data<T: Any>(&mut self) -> Option<&mut T> {
        self.data.as_mut()?.downcast_mut::<T>()
    }

    pub fn take_data<T: Any>(&mut self) -> Option<Box<T>> {
        self.data.take()?.downcast::<T>().ok()
    }

    pub fn get_local_sess_id(&self) -> u16 {
        self.local_sess_id
    }

    #[cfg(test)]
    pub fn set_local_sess_id(&mut self, sess_id: u16) {
        self.local_sess_id = sess_id;
    }

    pub fn get_peer_sess_id(&self) -> u16 {
        self.peer_sess_id
    }

    pub fn get_peer_addr(&self) -> SocketAddr {
        self.peer_addr
    }

    pub fn is_encrypted(&self) -> bool {
        match self.mode {
            SessionMode::Case(_) | SessionMode::Pase => true,
            SessionMode::PlainText => false,
        }
    }

    pub fn get_local_fabric_idx(&self) -> Option<u8> {
        match self.mode {
            SessionMode::Case(a) => Some(a),
            _ => None,
        }
    }

    pub fn get_session_mode(&self) -> SessionMode {
        self.mode
    }

    pub fn get_msg_ctr(&mut self) -> u32 {
        let ctr = self.msg_ctr;
        self.msg_ctr += 1;
        ctr
    }

    pub fn get_dec_key(&self) -> Option<&[u8]> {
        match self.mode {
            SessionMode::Case(_) | SessionMode::Pase => Some(&self.dec_key),
            SessionMode::PlainText => None,
        }
    }

    pub fn get_enc_key(&self) -> Option<&[u8]> {
        match self.mode {
            SessionMode::Case(_) | SessionMode::Pase => Some(&self.enc_key),
            SessionMode::PlainText => None,
        }
    }

    pub fn get_att_challenge(&self) -> &[u8] {
        &self.att_challenge
    }

    pub fn recv(
        &self,
        plain_hdr: &PlainHdr,
        proto_hdr: &mut ProtoHdr,
        parse_buf: &mut ParseBuf,
    ) -> Result<(), Error> {
        proto_hdr.decrypt_and_decode(plain_hdr, parse_buf, self.get_dec_key())
    }

    pub fn pre_send(&mut self, plain_hdr: &mut PlainHdr) -> Result<(), Error> {
        plain_hdr.sess_id = self.get_peer_sess_id();
        plain_hdr.ctr = self.get_msg_ctr();
        if self.is_encrypted() {
            plain_hdr.sess_type = plain_hdr::SessionType::Encrypted;
        }
        Ok(())
    }

    pub fn send(
        &self,
        plain_hdr: &mut PlainHdr,
        proto_hdr: &mut ProtoHdr,
        packet_buf: &mut WriteBuf,
    ) -> Result<(), Error> {
        // Generate encrypted header
        let mut tmp_buf: [u8; proto_hdr::max_proto_hdr_len()] = [0; proto_hdr::max_proto_hdr_len()];
        let mut write_buf = WriteBuf::new(&mut tmp_buf[..], proto_hdr::max_proto_hdr_len());
        proto_hdr.encode(&mut write_buf)?;
        packet_buf.prepend(write_buf.as_slice())?;

        // Generate plain-text header
        let mut tmp_buf: [u8; plain_hdr::max_plain_hdr_len()] = [0; plain_hdr::max_plain_hdr_len()];
        let mut write_buf = WriteBuf::new(&mut tmp_buf[..], plain_hdr::max_plain_hdr_len());
        plain_hdr.encode(&mut write_buf)?;
        let plain_hdr_bytes = write_buf.as_slice();

        trace!("unencrypted packet: {:x?}", packet_buf.as_slice());
        let enc_key = self.get_enc_key();
        if let Some(e) = enc_key {
            proto_hdr::encrypt_in_place(plain_hdr.ctr, plain_hdr_bytes, packet_buf, e)?;
        }

        packet_buf.prepend(plain_hdr_bytes)?;
        trace!("Full encrypted packet: {:x?}", packet_buf.as_slice());
        Ok(())
    }
}

impl Default for SessionMgr {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for Session {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "peer: {:?}, local: {}, remote: {}, msg_ctr: {}, mode: {:?}",
            self.peer_addr, self.local_sess_id, self.peer_sess_id, self.msg_ctr, self.mode,
        )
    }
}

#[derive(Debug)]
pub struct SessionMgr {
    next_sess_id: u16,
    sessions: [Option<Session>; 16],
}

impl SessionMgr {
    pub fn new() -> SessionMgr {
        SessionMgr {
            sessions: Default::default(),
            next_sess_id: 1,
        }
    }

    fn get_next_sess_id(&mut self) -> u16 {
        let mut next_sess_id: u16;
        loop {
            next_sess_id = self.next_sess_id;

            // Increment next sess id
            self.next_sess_id = self.next_sess_id.overflowing_add(1).0;
            if self.next_sess_id == 0 {
                self.next_sess_id = 1;
            }

            // Ensure the currently selected id doesn't match any existing session
            if self.get_with_id(next_sess_id).is_none() {
                break;
            }
        }
        next_sess_id
    }

    fn get_empty_slot(&self) -> Option<usize> {
        self.sessions.iter().position(|x| x.is_none())
    }

    pub fn add(&mut self, peer_addr: std::net::SocketAddr) -> Result<SessionHandle, Error> {
        let session = Session::new(peer_addr);
        self.add_session(session)
    }

    pub fn add_session(&mut self, session: Session) -> Result<SessionHandle, Error> {
        let index = self.get_empty_slot().ok_or(Error::NoSpace)?;
        self.sessions[index] = Some(session);
        Ok(self.get_session_handle(index))
    }

    fn _get(
        &self,
        sess_id: u16,
        peer_addr: std::net::SocketAddr,
        is_encrypted: bool,
    ) -> Option<usize> {
        self.sessions.iter().position(|x| {
            if let Some(x) = x {
                x.local_sess_id == sess_id
                    && x.peer_addr == peer_addr
                    && x.is_encrypted() == is_encrypted
            } else {
                false
            }
        })
    }

    pub fn get_with_id(&mut self, sess_id: u16) -> Option<SessionHandle> {
        let index = self
            .sessions
            .iter_mut()
            .position(|x| x.as_ref().map(|s| s.local_sess_id) == Some(sess_id))?;
        Some(self.get_session_handle(index))
    }

    pub fn get_or_add(
        &mut self,
        sess_id: u16,
        peer_addr: std::net::SocketAddr,
        is_encrypted: bool,
    ) -> Option<SessionHandle> {
        if let Some(index) = self._get(sess_id, peer_addr, is_encrypted) {
            Some(self.get_session_handle(index))
        } else if sess_id == 0 && !is_encrypted {
            // We must create a new session for this case
            info!("Creating new session");
            self.add(peer_addr).ok()
        } else {
            None
        }
    }

    pub fn recv(
        &mut self,
        plain_hdr: &mut PlainHdr,
        parse_buf: &mut ParseBuf,
        src: SocketAddr,
    ) -> Result<SessionHandle, Error> {
        // Read unencrypted packet header
        plain_hdr.decode(parse_buf)?;

        // Get session
        self.get_or_add(plain_hdr.sess_id, src, plain_hdr.is_encrypted())
            .ok_or(Error::NoSession)
    }

    fn get_session_handle<'a>(&'a mut self, sess_idx: usize) -> SessionHandle<'a> {
        SessionHandle {
            sess_mgr: self,
            sess_idx,
        }
    }
}

impl fmt::Display for SessionMgr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{{[")?;
        for s in self.sessions.iter().flatten() {
            writeln!(f, "{{ {}, }},", s)?;
        }
        write!(f, "], next_sess_id: {}", self.next_sess_id)?;
        write!(f, "}}")
    }
}

#[derive(Debug)]
pub struct SessionHandle<'a> {
    sess_mgr: &'a mut SessionMgr,
    sess_idx: usize,
}

impl<'a> SessionHandle<'a> {
    pub fn reserve_new_sess_id(&mut self) -> u16 {
        self.sess_mgr.get_next_sess_id()
    }
}

impl<'a> Deref for SessionHandle<'a> {
    type Target = Session;
    fn deref(&self) -> &Self::Target {
        // There is no other option but to panic if this is None
        self.sess_mgr.sessions[self.sess_idx].as_ref().unwrap()
    }
}

impl<'a> DerefMut for SessionHandle<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        // There is no other option but to panic if this is None
        self.sess_mgr.sessions[self.sess_idx].as_mut().unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::SessionMgr;
    use std::net::{Ipv4Addr, SocketAddr};

    #[test]
    fn test_next_sess_id_doesnt_reuse() {
        let mut sm = SessionMgr::new();
        let mut sess = sm
            .add(SocketAddr::new(
                std::net::IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
                8080,
            ))
            .unwrap();
        sess.set_local_sess_id(1);
        assert_eq!(sm.get_next_sess_id(), 2);
        assert_eq!(sm.get_next_sess_id(), 3);
        let mut sess = sm
            .add(SocketAddr::new(
                std::net::IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
                8080,
            ))
            .unwrap();
        sess.set_local_sess_id(4);
        assert_eq!(sm.get_next_sess_id(), 5);
    }

    #[test]
    fn test_next_sess_id_overflows() {
        let mut sm = SessionMgr::new();
        let mut sess = sm
            .add(SocketAddr::new(
                std::net::IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
                8080,
            ))
            .unwrap();
        sess.set_local_sess_id(1);
        assert_eq!(sm.get_next_sess_id(), 2);
        sm.next_sess_id = 65534;
        assert_eq!(sm.get_next_sess_id(), 65534);
        assert_eq!(sm.get_next_sess_id(), 65535);
        assert_eq!(sm.get_next_sess_id(), 2);
    }
}
