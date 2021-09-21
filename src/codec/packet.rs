use std::{convert::TryFrom, io::Cursor};

use crate::{codec::{ADDR_IPV4, ADDR_IPV6, socks5::ADDR_DOMAIN}, error::{ProxyError, ProxyResult}};

use super::Addr;
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use tokio_tungstenite::tungstenite::Message;

#[derive(Debug)]
pub enum Packet {
    Connect(Addr),
    Data(Vec<u8>),
    Close(),
}

const PACKET_CONNECT: u8 = 1;
const PACKET_DATA: u8 = 2;
const PACKET_CLOSE: u8 = 3;

impl TryFrom<Packet> for Message {
    type Error = ProxyError;

    fn try_from(value: Packet) -> ProxyResult<Message> {
        match value {
            Packet::Connect(addr) => {
                let mut msg = vec![PACKET_CONNECT];
                match addr {
                    Addr::IpV4((addr, port)) => {
                        msg.write_u8(ADDR_IPV4)?;
                        msg.write_u16::<LittleEndian>(port)?;
                        msg.extend(addr);
                    }
                    Addr::Domain((addr, port)) => {
                        msg.write_u8(ADDR_DOMAIN)?;
                        msg.write_u16::<LittleEndian>(port)?;
                        msg.extend(addr.as_bytes());
                    }
                    Addr::IpV6((addr, port)) => {
                        msg.write_u8(ADDR_IPV6)?;
                        msg.write_u16::<LittleEndian>(port)?;
                        msg.extend(addr);
                    }
                }
                Ok(Message::binary(msg))
            }
            Packet::Data(data) => {
                let mut msg = Vec::with_capacity(data.len() + 1);
                msg.push(PACKET_DATA);
                msg.extend(data);
                Ok(Message::binary(msg))
            }
            Packet::Close() => Ok(Message::binary(vec![PACKET_CLOSE])),
        }
    }
}

impl Packet {
    pub fn to_packet(msg: Message) -> ProxyResult<Packet> {
        if !msg.is_binary() {
            return Err(ProxyError::PacketNotBinaryMessage);
        }
        let mut data = msg.into_data();
        let mut cursor = Cursor::new(&mut data);
        match cursor.read_u8()? {
            PACKET_CONNECT => {
                let addr = Addr::from_bytes(&data[1..])?;
                Ok(Packet::Connect(addr))
            }
            PACKET_DATA => Ok(Packet::Data(data[1..].into())),
            PACKET_CLOSE => Ok(Packet::Close()),
            _ => unreachable!(),
        }
    }
}
