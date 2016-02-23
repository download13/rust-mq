use byteorder::{ReadBytesExt, BigEndian};
use error::{Error, Result};
use std::io::{BufReader, Read, Take, Cursor};
use std::net::TcpStream;
use std::sync::Arc;
use super::{PacketType, Header, QoS, LastWill, Protocol, PacketIdentifier, MULTIPLIER};

use mqtt::{
    Packet,
    Connect,
    Connack,
    Publish,
    Subscribe,
    Suback,
    Unsubscribe
};

pub trait MqttRead: ReadBytesExt {
    fn read_packet(&mut self) -> Result<Packet> {
        let hd = try!(self.read_u8());
        let len = try!(self.read_remaining_length());
        let header = try!(Header::new(hd, len));
        //println!("Header {:?}", header);
        if len == 0 {
            // no payload packets
            return match header.typ {
                PacketType::Pingreq => Ok(Packet::Pingreq),
                PacketType::Pingresp => Ok(Packet::Pingresp),
                _ => Err(Error::PayloadRequired)
            };
        }
        let mut raw_packet = self.take(len as u64);

        match header.typ {
            PacketType::Connect => Ok(Packet::Connect(try!(raw_packet.read_connect(header)))),
            PacketType::Connack => Ok(Packet::Connack(try!(raw_packet.read_connack(header)))),
            PacketType::Publish => Ok(Packet::Publish(try!(raw_packet.read_publish(header)))),
            PacketType::Subscribe => Ok(Packet::Subscribe(try!(raw_packet.read_subscribe(header)))),
            PacketType::Unsubscribe => Ok(Packet::Unsubscribe(try!(raw_packet.read_unsubscribe(header)))),
            PacketType::Pingreq => Err(Error::IncorrectPacketFormat),
            PacketType::Pingresp => Err(Error::IncorrectPacketFormat),
            _ => Err(Error::UnsupportedPacketType)
        }
    }

    fn read_connect(&mut self, header: Header) -> Result<Arc<Connect>> {
        let protocol_name = try!(self.read_mqtt_string());
        let protocol_level = try!(self.read_u8());
        let protocol = try!(Protocol::new(protocol_name, protocol_level));

        let connect_flags = try!(self.read_u8());
        let keep_alive = try!(self.read_u16::<BigEndian>());
        let client_id = try!(self.read_mqtt_string());

        let last_will = match connect_flags & 0b100 {
            0 => {
                if (connect_flags & 0b00111000) != 0 {
                    return Err(Error::IncorrectPacketFormat)
                }
                None
            },
            _ => {
                let will_topic = try!(self.read_mqtt_string());
                let will_message = try!(self.read_mqtt_string());
                let will_qod = try!(QoS::from_u8((connect_flags & 0b11000) >> 3));
                Some(LastWill {
                    topic: will_topic,
                    message: will_message,
                    qos: will_qod,
                    retain: (connect_flags & 0b00100000) != 0
                })
            }
        };

        let username = match connect_flags & 0b10000000 {
            0 => None,
            _ => Some(try!(self.read_mqtt_string()))
        };

        let password = match connect_flags & 0b01000000 {
            0 => None,
            _ => Some(try!(self.read_mqtt_string()))
        };

        Ok(Arc::new(
            Connect {
                protocol: protocol,
                keep_alive: keep_alive,
                client_id: client_id,
                clean_session: (connect_flags & 0b10) != 0,
                last_will: last_will,
                username: username,
                password: password
            }
        ))
    }

    fn read_connack(&mut self, header: Header) -> Result<Connack> {
        Err(Error::UnsupportedPacketType)
    }

    fn read_publish(&mut self, header: Header) -> Result<Arc<Publish>> {
        let topic_name = try!(self.read_mqtt_string());
        // Packet identifier exists where QoS > 0
        let pid = if header.qos().unwrap() != QoS::AtMostOnce {
            Some(PacketIdentifier(try!(self.read_u16::<BigEndian>())))
        } else {
            None
        };
        let payload_len = header.len - topic_name.len() - 2;
        let mut payload = Vec::with_capacity(payload_len);
        try!(self.read_to_end(&mut payload));

        Ok(Arc::new(
            Publish {
                dup: header.dup(),
                qos: try!(header.qos()),
                retain: header.retain(),
                topic_name: topic_name,
                pid: pid,
                payload: Arc::new(payload)
            }
        ))
    }

    fn read_subscribe(&mut self, header: Header) -> Result<Arc<Subscribe>> {
        let pid = try!(self.read_u16::<BigEndian>());
        let mut remaining_bytes = header.len - 2;
        let mut topics = Vec::with_capacity(1);

        while remaining_bytes > 0 {
            let topic_filter = try!(self.read_mqtt_string());
            let requested_qod = try!(self.read_u8());
            remaining_bytes -= topic_filter.len() + 3;
            topics.push((topic_filter, try!(QoS::from_u8(requested_qod))));
        };

        Ok(Arc::new(Subscribe {
            pid: PacketIdentifier(pid),
            topics: topics
        }))
    }

    fn read_unsubscribe(&mut self, header: Header) -> Result<Arc<Unsubscribe>> {
        let pid = try!(self.read_u16::<BigEndian>());
        let mut remaining_bytes = header.len - 2;
        let mut topics = Vec::with_capacity(1);

        while remaining_bytes > 0 {
            let topic_filter = try!(self.read_mqtt_string());
            remaining_bytes -= topic_filter.len() + 2;
            topics.push(topic_filter);
        };

        Ok(Arc::new(Unsubscribe {
            pid: PacketIdentifier(pid),
            topics: topics
        }))
    }

    fn read_payload(&mut self, len: usize) -> Result<Box<Vec<u8>>> {
        let mut payload = Box::new(Vec::with_capacity(len));
        try!(self.take(len as u64).read_to_end(&mut *payload));
        Ok(payload)
    }

    fn read_mqtt_string(&mut self) -> Result<String> {
        let len = try!(self.read_u16::<BigEndian>()) as usize;
        let mut string = String::with_capacity(len);
        try!(self.take(len as u64).read_to_string(&mut string));
        Ok(string)
    }

    fn read_remaining_length(&mut self) -> Result<usize> {
        let mut mult: usize = 1;
        let mut len: usize = 0;
        let mut done = false;


        while !done {
            let byte = try!(self.read_u8()) as usize;
            len += (byte & 0x7F) * mult;
            mult *= 0x80;
            if mult > MULTIPLIER {
                return Err(Error::MalformedRemainingLength);
            }
            done = (byte & 0x80) == 0
        }

        Ok(len)
    }
}

impl MqttRead for TcpStream {}
impl MqttRead for Cursor<Vec<u8>> {}
impl<T: Read> MqttRead for Take<T> where T: Read {}
impl<T: Read> MqttRead for BufReader<T> {}

#[cfg(test)]
mod test {
    use std::io::Cursor;
    use std::sync::Arc;
    use super::MqttRead;
    use super::super::{Protocol, LastWill, QoS, PacketIdentifier};
    use super::super::mqtt::{
        Packet,
        Connect,
        Publish,
        Subscribe,
        Unsubscribe
    };

    #[test]
    fn read_packet_connect_mqtt_protocol_test() {
        let mut stream = Cursor::new(vec![
            0x10, 39,
            0x00, 0x04, 'M' as u8, 'Q' as u8, 'T' as u8, 'T' as u8,
            0x04,
            0b11001110, // +username, +password, -will retain, will qos=1, +last_will, +clean_session
            0x00, 0x0a, // 10 sec
            0x00, 0x04, 't' as u8, 'e' as u8, 's' as u8, 't' as u8, // client_id
            0x00, 0x02, '/' as u8, 'a' as u8, // will topic = '/a'
            0x00, 0x07, 'o' as u8, 'f' as u8, 'f' as u8, 'l' as u8, 'i' as u8, 'n' as u8, 'e' as u8, // will msg = 'offline'
            0x00, 0x04, 'r' as u8, 'u' as u8, 's' as u8, 't' as u8, // username = 'rust'
            0x00, 0x02, 'm' as u8, 'q' as u8 // password = 'mq'
        ]);

        let packet = stream.read_packet().unwrap();

        assert_eq!(packet, Packet::Connect(Arc::new(Connect {
            protocol: Protocol::MQTT(4),
            keep_alive: 10,
            client_id: "test".to_owned(),
            clean_session: true,
            last_will: Some(LastWill {
                topic: "/a".to_owned(),
                message: "offline".to_owned(),
                retain: false,
                qos: QoS::AtLeastOnce
            }),
            username: Some("rust".to_owned()),
            password: Some("mq".to_owned())
        })));
    }

    #[test]
    fn read_packet_connect_mqisdp_protocol_test() {
        let mut stream = Cursor::new(vec![
            0x10, 18,
            0x00, 0x06, 'M' as u8, 'Q' as u8, 'I' as u8, 's' as u8, 'd' as u8, 'p' as u8,
            0x03,
            0b00000000, // -username, -password, -will retain, will qos=0, -last_will, -clean_session
            0x00, 0x3c, // 60 sec
            0x00, 0x04, 't' as u8, 'e' as u8, 's' as u8, 't' as u8 // client_id
        ]);

        let packet = stream.read_packet().unwrap();

        assert_eq!(packet, Packet::Connect(Arc::new(Connect {
            protocol: Protocol::MQIsdp(3),
            keep_alive: 60,
            client_id: "test".to_owned(),
            clean_session: false,
            last_will: None,
            username: None,
            password: None
        })));
    }

    #[test]
    fn read_packet_publish_qos1_test() {
        let mut stream = Cursor::new(vec![
            0b00110010, 11,
            0x00, 0x03, 'a' as u8, '/' as u8, 'b' as u8, // topic name = 'a/b'
            0x00, 0x0a, // pid = 10
            0xF1, 0xF2, 0xF3, 0xF4
        ]);

        let packet = stream.read_packet().unwrap();

        assert_eq!(packet, Packet::Publish(Arc::new(Publish {
            dup: false,
            qos: QoS::AtLeastOnce,
            retain: false,
            topic_name: "a/b".to_owned(),
            pid: Some(PacketIdentifier(10)),
            payload: Arc::new(vec![0xF1, 0xF2, 0xF3, 0xF4])
        })));
    }

    #[test]
    fn read_packet_publish_qos0_test() {
        let mut stream = Cursor::new(vec![
            0b00110000, 7,
            0x00, 0x03, 'a' as u8, '/' as u8, 'b' as u8, // topic name = 'a/b'
            0x01, 0x02
        ]);

        let packet = stream.read_packet().unwrap();

        assert_eq!(packet, Packet::Publish(Arc::new(Publish {
            dup: false,
            qos: QoS::AtMostOnce,
            retain: false,
            topic_name: "a/b".to_owned(),
            pid: None,
            payload: Arc::new(vec![0x01, 0x02])
        })));
    }

    #[test]
    fn read_packet_subscribe_test() {
        let mut stream = Cursor::new(vec![
            0b10000010, 20,
            0x01, 0x04, // pid = 260
            0x00, 0x03, 'a' as u8, '/' as u8, '+' as u8, // topic filter = 'a/+'
            0x00, // qos = 0
            0x00, 0x01, '#' as u8, // topic filter = '#'
            0x01, // qos = 1
            0x00, 0x05, 'a' as u8, '/' as u8, 'b' as u8, '/' as u8, 'c' as u8, // topic filter = 'a/b/c'
            0x02 // qos = 2
        ]);

        let packet = stream.read_packet().unwrap();

        assert_eq!(packet, Packet::Subscribe(Arc::new(Subscribe {
            pid: PacketIdentifier(260),
            topics: vec![
                ("a/+".to_owned(), QoS::AtMostOnce),
                ("#".to_owned(), QoS::AtLeastOnce),
                ("a/b/c".to_owned(), QoS::ExactlyOnce)
            ]
        })));
    }

    #[test]
    fn read_packet_unsubscribe_test() {
        let mut stream = Cursor::new(vec![
            0b10100010, 17,
            0x00, 0x0F, // pid = 15
            0x00, 0x03, 'a' as u8, '/' as u8, '+' as u8, // topic filter = 'a/+'
            0x00, 0x01, '#' as u8, // topic filter = '#'
            0x00, 0x05, 'a' as u8, '/' as u8, 'b' as u8, '/' as u8, 'c' as u8, // topic filter = 'a/b/c'
        ]);

        let packet = stream.read_packet().unwrap();

        assert_eq!(packet, Packet::Unsubscribe(Arc::new(Unsubscribe {
            pid: PacketIdentifier(15),
            topics: vec![
                "a/+".to_owned(),
                "#".to_owned(),
                "a/b/c".to_owned()
            ]
        })));
    }
}