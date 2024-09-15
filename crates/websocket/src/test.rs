use tokio::signal::unix::Signal;

/// zchronod_websocket test
use crate::*;
use std::{net::SocketAddr, str::FromStr};

#[derive(Debug, serde::Serialize, serde::Deserialize, PartialEq)]
enum TestMsg {
    HI,
}

fn handle_jobs(msg: Vec<u8>) -> Result<Vec<u8>, std::io::Error> {
    Ok(serde_json::to_vec(&TestMsg::HI).unwrap())
}

async fn handle_connection(mut receiver: WebsocketReceiver) {
    while let Ok(msg) = receiver.recv().await {
        println!("Received message: {:?}", msg);

        match msg {
            ReceiveMessage::Signal(s) => println!("{s:?}"),
            ReceiveMessage::Request(m, p, r) => {
                let rep = match m.as_str() {
                    "dispatch_job" => handle_jobs(p),
                    &_ => panic!("todo"),
                }
                .unwrap();
                if let Err(e) = r.respond(rep).await {
                    println!("Failed to send message: {:?}", e);
                    return;
                }
            }
        };
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn test1() {
    let task = tokio::task::spawn(async move {
        let socket = SocketAddr::from_str("127.0.0.1:8080").unwrap();
        println!("ip: {:?}", socket);
        let (send, mut recv) = connect(Arc::new(WebsocketConfig::default()), socket)
            .await
            .unwrap();

        let s_task = tokio::task::spawn(async move {
            handle_connection(recv).await;
        });

        loop {
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            let _res = send
                .request_timeout(
                    String::from_str("dispatch_job").unwrap(),
                    serde_json::to_vec(&TestMsg::HI).unwrap(),
                    std::time::Duration::from_secs(5),
                )
                .await
                .unwrap();
        }

        //assert_eq!(TestMsg::HI, serde_json::from_slice(res.as_slice()).unwrap());
        s_task.await.unwrap();
    });

    task.await.unwrap();
}

/*
#[tokio::test(flavor = "multi_thread")]
async fn test1() {
    let task = tokio::task::spawn(async move {
        let socket = SocketAddr::from_str("127.0.0.1:8080").unwrap();
        println!("ip: {:?}", socket);
        let (send, mut recv) = connect(Arc::new(WebsocketConfig::default()), socket)
            .await
            .unwrap();

        let s_task = tokio::task::spawn(async move {
            handle_connection(recv).await;
        });

        loop {
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            let _res = send
                .request_timeout(
                    String::from_str("dispatch_job").unwrap(),
                    serde_json::to_vec(&TestMsg::HI).unwrap(),
                    std::time::Duration::from_secs(5),
                )
                .await
                .unwrap();
        }

        //assert_eq!(TestMsg::HI, serde_json::from_slice(res.as_slice()).unwrap());
        s_task.await.unwrap();
    });

    task.await.unwrap();
}
*/
