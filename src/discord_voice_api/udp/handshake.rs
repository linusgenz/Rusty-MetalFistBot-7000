use anyhow::Result;
use futures_util::StreamExt;
use serde_json::Value;
use tokio::net::TcpStream;
use tokio::sync::Mutex as TokioMutex;
use tokio_tungstenite::MaybeTlsStream;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::Message;

pub async fn wait_for_hello(
    ws_rx: &TokioMutex<
        futures_util::stream::SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>,
    >,
) -> Result<u64> {
    loop {
        let msg_opt = {
            let mut rx = ws_rx.lock().await;
            rx.next().await
        };

        let msg = msg_opt.ok_or_else(|| anyhow::anyhow!("Voice WebSocket closed"))??;
        if let Message::Text(txt) = msg {
            let data: Value = serde_json::from_str(&txt)?;
            if data["op"] == 8 {
                let interval = data["d"]["heartbeat_interval"]
                    .as_u64()
                    .ok_or_else(|| anyhow::anyhow!("Heartbeat interval missing"))?;
                return Ok(interval);
            }
        }
    }
}

pub async fn wait_for_ready(
    ws_rx: &TokioMutex<
        futures_util::stream::SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>,
    >,
) -> Result<(u32, String, u16)> {
    loop {
        let msg_opt = {
            let mut rx = ws_rx.lock().await;
            rx.next().await
        };

        let msg = msg_opt.ok_or_else(|| anyhow::anyhow!("Voice WebSocket closed"))??;
        if let Message::Text(txt) = msg {
            let data: Value = serde_json::from_str(&txt)?;
            if data["op"] == 2 {
                let ssrc = data["d"]["ssrc"].as_u64().unwrap() as u32;
                let ip = data["d"]["ip"].as_str().unwrap().to_string();
                let port = data["d"]["port"].as_u64().unwrap() as u16;
                return Ok((ssrc, ip, port));
            }
        }
    }
}

pub async fn wait_for_secret(
    ws_rx: &TokioMutex<
        futures_util::stream::SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>,
    >,
) -> Result<(Vec<u8>, String)> {
    loop {
        let msg_opt = {
            let mut rx = ws_rx.lock().await;
            rx.next().await
        };

        let msg = msg_opt.ok_or_else(|| anyhow::anyhow!("Voice WebSocket closed"))??;
        if let Message::Text(txt) = msg {
            let data: Value = serde_json::from_str(&txt)?;
            if data["op"] == 4 {
                let key: Vec<u8> = data["d"]["secret_key"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_u64().unwrap() as u8)
                    .collect();
                let mode = data["d"]["mode"].as_str().unwrap().to_string();
                return Ok((key, mode));
            }
        }
    }
}
