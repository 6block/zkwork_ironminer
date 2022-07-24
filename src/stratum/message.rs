/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use anyhow::Result;
use std::io::Write;

use bytes::{BufMut, BytesMut};
use serde::{Deserialize, Serialize};
use tokio_util::codec::{Decoder, Encoder};

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[allow(non_snake_case)]
pub struct MiningSubscribeBody {
    pub version: i64,
    pub name: String,
    pub publicAddress: String,
}
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct MiningSubscribeMessage {
    pub id: i64,
    pub method: String,
    pub body: MiningSubscribeBody,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[allow(non_snake_case)]
pub struct MiningSubscribedBody {
    pub clientId: u64,
    pub graffiti: String,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct MiningSubscribedMessage {
    pub id: i64,
    pub method: String,
    pub body: MiningSubscribedBody,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct MiningSetTargetBody {
    pub target: String,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct MiningSetTargetMessage {
    pub id: i64,
    pub method: String,
    pub body: MiningSetTargetBody,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[allow(non_snake_case)]
pub struct MiningNotifyBody {
    pub miningRequestId: u32,
    pub header: String,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct MiningNotifyMessage {
    pub id: i64,
    pub method: String,
    pub body: MiningNotifyBody,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[allow(non_snake_case)]
pub struct MiningSubmitBody {
    pub miningRequestId: u32,
    pub randomness: String,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct MiningSubmitMessage {
    pub id: i64,
    pub method: String,
    pub body: MiningSubmitBody,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct MiningWaitForWorkMessage {
    pub id: i64,
    pub method: String,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(untagged)]
pub enum StratumMessage {
    MiningSubscribeMessage(MiningSubscribeMessage),
    MiningSubscribedMessage(MiningSubscribedMessage),
    MiningSetTargetMessage(MiningSetTargetMessage),
    MiningNotifyMessage(MiningNotifyMessage),
    MiningSubmitMessage(MiningSubmitMessage),
    MiningWaitForWorkMessage(MiningWaitForWorkMessage),
}
#[derive(Default)]
pub struct StratumMessageCodec {
    cursor: usize,
}

impl Encoder<StratumMessage> for StratumMessageCodec {
    type Error = anyhow::Error;
    fn encode(&mut self, message: StratumMessage, dst: &mut BytesMut) -> Result<(), Self::Error> {
        //bincode::serialize_into(&mut dst.writer(), &message)?;
        let json_string = serde_json::to_string(&message).unwrap();
        dst.writer().write_all(json_string.as_bytes())?;
        dst.writer().write_all("\n".as_bytes())?;
        Ok(())
    }
}

impl Decoder for StratumMessageCodec {
    type Error = anyhow::Error;
    type Item = StratumMessage;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let mut i = self.cursor;
        while i < src.len() {
            if src[i] == 10u8 {
                self.cursor = 0;
                let mut data = src.split_to(i + 1);
                unsafe {
                    data.set_len(i);
                }
                src.reserve(100);
                let message = serde_json::from_slice(&data[..])?;
                return Ok(Some(message));
            }
            i += 1;
        }
        self.cursor = i;
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_subscribe_message() {
        let origin_json_string = "{\"id\":0,\"method\":\"mining.subscribe\",\"body\":{\"version\":0,\"publicAddress\":\"127.0.0.1:8888\"}}";

        let message = StratumMessage::MiningSubscribeMessage(MiningSubscribeMessage {
            id: 0,
            method: String::from("mining.subscribe"),
            body: MiningSubscribeBody {
                version: 0,
                publicAddress: String::from("127.0.0.1:8888"),
            },
        });
        let json_string = serde_json::to_string(&message).unwrap();
        println!("{:?}", json_string);
        let message_one: StratumMessage = serde_json::from_str(origin_json_string).unwrap();
        println!("{:?}", message_one);
        assert_eq!(message, message_one);
        assert_eq!(origin_json_string, json_string);

        let mut buf = BytesMut::new();
        let mut codec = StratumMessageCodec::default();
        let _ = codec.encode(message.clone(), &mut buf);
        println!("buf: {:?}", buf);
        let message_one = codec.decode(&mut buf).unwrap().unwrap();
        println!("{:?}", message_one);
        assert_eq!(message, message_one);
        let _ = codec.encode(message.clone(), &mut buf);
        println!("buf: {:?}", buf);
        let message_one = codec.decode(&mut buf).unwrap().unwrap();
        println!("{:?}", message_one);
        assert_eq!(message, message_one);
    }

    #[test]
    fn test_subscribed_message() {
        let  origin_json_string = "{\"id\":0,\"method\":\"mining.subscribed\",\"body\":{\"clientId\":0,\"graffiti\":\"zk.work\"}}";

        let message = StratumMessage::MiningSubscribedMessage(MiningSubscribedMessage {
            id: 0,
            method: String::from("mining.subscribed"),
            body: MiningSubscribedBody {
                clientId: 0,
                graffiti: String::from("zk.work"),
            },
        });
        let json_string = serde_json::to_string(&message).unwrap();
        println!("{:?}", json_string);
        let message_one: StratumMessage = serde_json::from_str(origin_json_string).unwrap();
        println!("{:?}", message_one);
        assert_eq!(message, message_one);
        assert_eq!(origin_json_string, json_string);

        let mut buf = BytesMut::new();
        let mut codec = StratumMessageCodec { cursor: 0 };
        let _ = codec.encode(message.clone(), &mut buf);
        println!("buf: {:?}", buf);
        let message_one = codec.decode(&mut buf).unwrap().unwrap();
        println!("{:?}", message_one);
        assert_eq!(message, message_one);
        let _ = codec.encode(message.clone(), &mut buf);
        println!("buf: {:?}", buf);
        let message_one = codec.decode(&mut buf).unwrap().unwrap();
        println!("{:?}", message_one);
        assert_eq!(message, message_one);
    }

    #[test]
    fn test_settarget_message() {
        let origin_json_string =
            "{\"id\":0,\"method\":\"mining.set_target\",\"body\":{\"target\":\"1234567890\"}}";

        let message = StratumMessage::MiningSetTargetMessage(MiningSetTargetMessage {
            id: 0,
            method: String::from("mining.set_target"),
            body: MiningSetTargetBody {
                target: 1234567890.to_string(),
            },
        });
        let json_string = serde_json::to_string(&message).unwrap();
        println!("{:?}", json_string);
        let message_one: StratumMessage = serde_json::from_str(origin_json_string).unwrap();
        println!("{:?}", message_one);
        assert_eq!(message, message_one);
        assert_eq!(origin_json_string, json_string);

        let mut buf = BytesMut::new();
        let mut codec = StratumMessageCodec::default();
        let _ = codec.encode(message.clone(), &mut buf);
        println!("buf: {:?}", buf);
        let message_one = codec.decode(&mut buf).unwrap().unwrap();
        println!("{:?}", message_one);
        assert_eq!(message, message_one);
        let _ = codec.encode(message.clone(), &mut buf);
        println!("buf: {:?}", buf);
        let message_one = codec.decode(&mut buf).unwrap().unwrap();
        println!("{:?}", message_one);
        assert_eq!(message, message_one);
    }

    #[test]
    fn test_notify_message() {
        let  origin_json_string = "{\"id\":0,\"method\":\"mining.notify\",\"body\":{\"miningRequestId\":12345,\"header\":\"header data...\"}}";

        let message = StratumMessage::MiningNotifyMessage(MiningNotifyMessage {
            id: 0,
            method: String::from("mining.notify"),
            body: MiningNotifyBody {
                miningRequestId: 12345,
                header: String::from("header data..."),
            },
        });
        let json_string = serde_json::to_string(&message).unwrap();
        println!("{:?}", json_string);
        let message_one: StratumMessage = serde_json::from_str(origin_json_string).unwrap();
        println!("{:?}", message_one);
        assert_eq!(message, message_one);
        assert_eq!(origin_json_string, json_string);

        let mut buf = BytesMut::new();
        let mut codec = StratumMessageCodec::default();
        let _ = codec.encode(message.clone(), &mut buf);
        println!("buf: {:?}", buf);
        let message_one = codec.decode(&mut buf).unwrap().unwrap();
        println!("{:?}", message_one);
        assert_eq!(message, message_one);
        let _ = codec.encode(message.clone(), &mut buf);
        println!("buf: {:?}", buf);
        let message_one = codec.decode(&mut buf).unwrap().unwrap();
        println!("{:?}", message_one);
        assert_eq!(message, message_one);
    }

    #[test]
    fn test_submit_message() {
        let  origin_json_string = "{\"id\":0,\"method\":\"mining.submit\",\"body\":{\"miningRequestId\":12345,\"randomness\":\"123456789\"}}";

        let message = StratumMessage::MiningSubmitMessage(MiningSubmitMessage {
            id: 0,
            method: String::from("mining.submit"),
            body: MiningSubmitBody {
                miningRequestId: 12345,
                randomness: 123456789.to_string(),
            },
        });
        let json_string = serde_json::to_string(&message).unwrap();
        println!("{:?}", json_string);
        let message_one: StratumMessage = serde_json::from_str(origin_json_string).unwrap();
        println!("{:?}", message_one);
        assert_eq!(message, message_one);
        assert_eq!(origin_json_string, json_string);

        let mut buf = BytesMut::new();
        let mut codec = StratumMessageCodec::default();
        let _ = codec.encode(message.clone(), &mut buf);
        println!("buf: {:?}", buf);
        let message_one = codec.decode(&mut buf).unwrap().unwrap();
        println!("{:?}", message_one);
        assert_eq!(message, message_one);
        let _ = codec.encode(message.clone(), &mut buf);
        println!("buf: {:?}", buf);
        let message_one = codec.decode(&mut buf).unwrap().unwrap();
        println!("{:?}", message_one);
        assert_eq!(message, message_one);
    }

    #[test]
    fn test_waitfortask_message() {
        let origin_json_string = "{\"id\":0,\"method\":\"mining.wait_for_work\"}";

        let message = StratumMessage::MiningWaitForWorkMessage(MiningWaitForWorkMessage {
            id: 0,
            method: String::from("mining.wait_for_work"),
        });
        let json_string = serde_json::to_string(&message).unwrap();
        println!("{:?}", json_string);
        let message_one: StratumMessage = serde_json::from_str(origin_json_string).unwrap();
        println!("{:?}", message_one);
        assert_eq!(message, message_one);
        assert_eq!(origin_json_string, json_string);

        let mut buf = BytesMut::new();
        let mut codec = StratumMessageCodec::default();
        let _ = codec.encode(message.clone(), &mut buf);
        println!("buf: {:?}", buf);
        let message_one = codec.decode(&mut buf).unwrap().unwrap();
        println!("{:?}", message_one);
        assert_eq!(message, message_one);
        let _ = codec.encode(message.clone(), &mut buf);
        println!("buf: {:?}", buf);
        let message_one = codec.decode(&mut buf).unwrap().unwrap();
        println!("{:?}", message_one);
        assert_eq!(message, message_one);
    }
}
