
use std::{convert::TryFrom, io::Cursor};

use crate::error::{ProxyError, ProxyResult};

use super::{Addr};
use log::info;
use tokio_tungstenite::tungstenite::Message;
use byteorder::{LittleEndian, WriteBytesExt, ReadBytesExt};

pub enum Packet {
    Connect(Addr),
    Data(Vec<u8>),
    Close(),
}

impl TryFrom<Packet> for Message {
    type Error = ProxyError;

    fn try_from(value: Packet) -> ProxyResult<Message> {
        match value {
            Packet::Connect(addr) => {
                let mut msg = vec![1];
                match addr {
                    Addr::IpV4((addr, port)) => {
                        msg.write_u8(1)?;
                        msg.write_u16::<LittleEndian>(port)?;
                        msg.extend(addr);
                    }
                    Addr::Domain((addr, port)) => {
                        msg.write_u8(3)?;
                        msg.write_u16::<LittleEndian>(port)?;
                        msg.extend(addr.as_bytes());
                    }
                    Addr::IpV6((addr, port)) => {
                        msg.write_u8(4)?;
                        msg.write_u16::<LittleEndian>(port)?;
                        msg.extend(addr);
                    }
                }
                info!("connect packet msg is {:?}", msg);
                Ok(Message::binary(msg))
            }
            Packet::Data(data) => {
                let mut msg = Vec::with_capacity(data.len() + 1);
                msg.push(2);
                msg.extend(data);
                info!("data packet msg is {:?}", msg);
                Ok(Message::binary(msg))
            }
            Packet::Close() => {
                Ok(Message::binary(vec![3]))
            }
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
            1 => {
                let addr = Addr::from_bytes(&data[1..])?;
                Ok(Packet::Connect(addr))
            }
            2 => {
                Ok(Packet::Data(data[1..].into()))
            },
            3 => {
                Ok(Packet::Close())
            },
            _ => unreachable!()
        }
    }
}
