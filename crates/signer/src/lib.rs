pub mod msg_signer;

#[cfg(test)]
mod tests {
    use super::msg_signer::{Keccak256Secp256k1, MessageVerify, Signer};
    use hex::FromHex;
    use secp256k1::{PublicKey, Secp256k1, SecretKey};
    use serde::{Deserialize, Serialize};
    use serde_json::Value;
    use std::str::FromStr;

    use alloy::{
        primitives::keccak256,
        signers::{local::PrivateKeySigner, Signature as AlloySignature, SignerSync},
    };

    #[derive(Serialize, Deserialize)]
    struct MessageData {
        operator: String,
        address: String,
    }

    //#[test]
    //fn test_signer() {
    //    let secp = Secp256k1::new();
    //    let secret_key = SecretKey::from_slice(&[0x1f; 32]).expect("32 bytes, within curve order");
    //    let public_key = PublicKey::from_secret_key(&secp, &secret_key);

    //    let message = MessageData {
    //        operator: "operator_address".to_string(),
    //        address: "public_key_address".to_string(),
    //    };

    //    let signer = Keccak256Secp256k1;

    //    let signature = signer.sign_message(&secret_key, &message);
    //    dbg!(&signature);

    //    let is_valid = signer.verify_signature(&public_key, &message, &signature);
    //    dbg!(&is_valid);
    //    assert_eq!(is_valid, true);
    //}

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct WsMethodMsg {
        pub id: String,
        pub method: Option<String>,
        pub params: Option<Value>,
        pub result: Option<Value>,
        pub address: String,
        pub hash: String,
        pub signature: String,
    }

    #[derive(Serialize, Deserialize, Debug)]
    pub struct WsResponse {
        pub code: u16,
        pub message: String,
    }

    /*
        #[test]
        fn test_eth_signer() {
            //let signer = PrivateKeySigner::from_slice(&[0x1f; 32]).expect("singer err");

            let signer = PrivateKeySigner::from_slice(
                &<[u8; 32]>::from_hex(
                    "77f4b2fbf3f32687f03d84d323bd5cb443f53b0fc338b51c24e319a520c87217",
                )
                .unwrap_or_default(),
            )
            .expect("signer err");

            dbg!(&signer.address());
            let verify = MessageVerify(signer);

            let message = MessageData {
                operator: "operator_address".to_string(),
                address: "public_key_address".to_string(),
            };

            let sig = verify.sign_message(&verify.0, &message);
            dbg!(&sig);
            let is_verify = verify.verify_signature(&verify.get_address(), &message, &sig);

            assert_eq!(is_verify, true, "");

            let result = WsResponse {
                code: 200,
                message: "success".to_owned(),
            };

            let rep = WsMethodMsg {
                id: "9aaba61d-d097-4287-8569-1cb1a186328d".to_owned(),
                method: None,
                params: None,
                result: Some(serde_json::to_value(&result).unwrap()),
                address: "0x02a5592a6dE1568F6eFdC536DA3EF887f98414cb".to_owned(),
                hash: "".to_owned(),
                signature: "".to_owned(),
            };

            let sigs = "d80b17bf06103bcb4c57563fea0940d46e9c16c602f2aee38ddf30b1a1b2e1875930fd2b9f3c5be5fe0074d26265afb30c0a057cf77edf284575c293dfb30f6a1c";
            dbg!(&rep);
            let is_verify = verify.verify_signature(&rep.address, &rep, sigs);
            let hash = MessageVerify::generate_hash_str(&rep).unwrap();
            dbg!(hash);
            dbg!(is_verify);

            let rep2 = super::msg_signer::WsMethodMsg {
                id: "9aaba61d-d097-4287-8569-1cb1a186328d".to_owned(),
                method: None,
                params: None,
                result: Some(serde_json::to_value(&result).unwrap()),
                address: "0x02a5592a6dE1568F6eFdC536DA3EF887f98414cb".to_owned(),
                hash: "".to_owned(),
                signature: "d80b17bf06103bcb4c57563fea0940d46e9c16c602f2aee38ddf30b1a1b2e1875930fd2b9f3c5be5fe0074d26265afb30c0a057cf77edf284575c293dfb30f6a1c".to_owned(),
            };

            let is_verify2 = MessageVerify::verify_message(&rep2);
            dbg!(is_verify2);
        }

        #[test]
        fn test2() {
            //let signer = PrivateKeySigner::from_slice(&[0x1f; 32]).expect("singer err");

            let result = WsResponse {
                code: 200,
                message: "success".to_owned(),
            };

            let rep2 = super::msg_signer::WsMethodMsg {
                id: "9aaba61d-d097-4287-8569-1cb1a186328d".to_owned(),
                method: None,
                params: None,
                result: Some(serde_json::to_value(&result).unwrap()),
                address: "0x02a5592a6dE1568F6eFdC536DA3EF887f98414cb".to_owned(),
                hash: "".to_owned(),
                signature: "d80b17bf06103bcb4c57563fea0940d46e9c16c602f2aee38ddf30b1a1b2e1875930fd2b9f3c5be5fe0074d26265afb30c0a057cf77edf284575c293dfb30f6a1c".to_owned(),
            };

            let is_verify = MessageVerify::verify_message(&rep2);
            dbg!(is_verify);
        }
    */
    #[test]
    fn test3() {
        let rep = super::msg_signer::WsMethodMsg {
            id: "555ebf10-f165-41f6-ae05-918a9232d862".to_owned(),
            method: Some("dispatch_job".to_owned()),
            params: Some(serde_json::json!([{
                "user": "b77f1799de0148c07bc6ef630fb75ac267f31d147cd28797ad145afe72302632".to_owned(),
                "seed": "".to_owned(),
                "tag": "".to_owned(),
                "position": "".to_owned(),
                "signature": "".to_owned(),
                "clock": serde_json::json!({
                    "1": "1".to_owned(),
                }),
                "job_id": "4a16467fc69713bd4ed0b45a4ddab8ed2b69ae14e48236973a75e941f20971fd".to_owned(),
                "job": serde_json::json!({
                    "prompt": "What is AI?".to_owned(),
                    "model": "ss".to_owned(),
                    "tag": "tee".to_owned(),
                    "params": serde_json::json!({
                        "temperature": 1.0,
                        "top_p": 0.5,
                        "max_tokens": 1024,
                    }),
                }),
            }
            ])),
            result: None,
            address: "0x1DdBd306eFFbb5FF29E41398A6a1198Ee6Fb51ce".to_owned(),
            hash: "".to_owned(),
            signature: "b5bb37c94af82989f6406de94c921224b00e6cd1ca8079b496507c23f306c56878d5894c8e5ff2fb9296bad91a908a0e6d0ddd516f7286557eaecf36cfcf49401c".to_owned(),
        };
        dbg!(&rep);

        let is_verify = MessageVerify::verify_message(&rep);
        dbg!(is_verify);

        let text_msg = r#"
        {"id":"555ebf10-f165-41f6-ae05-918a9232d862","method":"dispatch_job","params":[{"user":"b77f1799de0148c07bc6ef630fb75ac267f31d147cd28797ad145afe72302632","seed":"","tag":"","position":"","signature":"","clock":{"1":"1"},"job_id":"4a16467fc69713bd4ed0b45a4ddab8ed2b69ae14e48236973a75e941f20971fd","job":{"tag":"tee","prompt":"What is AI?","model":"ss","params":{"temperature":1.0,"top_p":0.5,"max_tokens":1024}}}],"result":null,"address":"0x1DdBd306eFFbb5FF29E41398A6a1198Ee6Fb51ce","hash":"","signature":"b5bb37c94af82989f6406de94c921224b00e6cd1ca8079b496507c23f306c56878d5894c8e5ff2fb9296bad91a908a0e6d0ddd516f7286557eaecf36cfcf49401c"}
        "#;
        let receive_msg = serde_json::from_str::<super::msg_signer::WsMethodMsg>(text_msg).unwrap();
        dbg!(&receive_msg);

        let de_is_verify = MessageVerify::verify_message(&receive_msg);
        dbg!(de_is_verify);
    }
}
