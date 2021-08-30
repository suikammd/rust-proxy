use std::convert::TryFrom;

use bytes::BytesMut;

use crate::error::{CustomError, SocksResult};

const SOCKS5_VERSION: u8 = 0x05;

pub enum MethodType {
    NoAuth,
    UserPass,
}

impl TryFrom<u8> for MethodType {
    type Error = CustomError;
    fn try_from(orig: u8) -> SocksResult<Self> {
        match orig {
            0 => Ok(MethodType::NoAuth),
            2 => Ok(MethodType::UserPass),
            _ => Err(CustomError::UnsupportedMethodType),
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum Command {
    Connect,
    Bind,
    Udp,
}

impl TryFrom<u8> for Command {
    type Error = CustomError;
    fn try_from(value: u8) -> SocksResult<Self> {
        match value {
            1 => Ok(Command::Connect),
            2 => Ok(Command::Bind),
            3 => Ok(Command::Udp),
            _ => Err(CustomError::UnsupportedCommand),
        }
    }
}

impl From<Command> for u8 {
    fn from(command: Command) -> Self {
        match command {
            Command::Connect => 0x01,
            Command::Bind => 0x02,
            Command::Udp => 0x03,
        }
    }
}

#[derive(Debug)]
pub enum RepCode {
    Success,
    ConnectError,
    DisallowConnection,
    NetworkUnreachable,
    HostUnreachable,
    ConnectionRefused,
    TTLTimeout,
    UnsupportedCommand,
    UnsupportedAddrType,
    Undefined,
}

impl TryFrom<u8> for RepCode {
    type Error = CustomError;
    fn try_from(orig: u8) -> Result<Self, Self::Error> {
        match orig {
            0 => Ok(RepCode::Success),
            1 => Ok(RepCode::ConnectError),
            2 => Ok(RepCode::DisallowConnection),
            3 => Ok(RepCode::NetworkUnreachable),
            4 => Ok(RepCode::HostUnreachable),
            5 => Ok(RepCode::ConnectionRefused),
            6 => Ok(RepCode::TTLTimeout),
            7 => Ok(RepCode::UnsupportedCommand),
            8 => Ok(RepCode::UnsupportedAddrType),
            _ => Err(CustomError::InvalidRepCode),
        }
    }
}

impl From<RepCode> for u8 {
    fn from(orig: RepCode) -> u8 {
        match orig {
            RepCode::Success => 0,
            RepCode::ConnectError => 1,
            RepCode::DisallowConnection => 2,
            RepCode::NetworkUnreachable => 3,
            RepCode::HostUnreachable => 4,
            RepCode::ConnectionRefused => 5,
            RepCode::TTLTimeout => 6,
            RepCode::UnsupportedCommand => 7,
            RepCode::UnsupportedAddrType => 8,
            RepCode::Undefined => 9,
        }
    }
}

// pub enum PacketType {
//     Connect,
//     Data,
// }

// impl TryFrom<u8> for PacketType {
//     type Error = CustomError;

//     fn try_from(value: u8) -> Result<Self, Self::Error> {
//         match value {
//             0 => Ok(PacketType::Data),
//             1 => Ok(PacketType::Data),
//             _ => Err(CustomError::InvalidPacketType),
//         }
//     }
// }

// // new message protocol
// // data_type 2 bit
// // connection_id 14bit

// // if data_type == 0, data_len 16bit

// // data_type = 0 data
// // data_type = 1 connect
// pub enum WSPacket {
//     packet_type: PacketType,
//     connection_id:
// }