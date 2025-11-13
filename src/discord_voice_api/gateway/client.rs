use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use std::sync::Arc;
use tokio::{
    net::TcpStream,
    sync::{Mutex, mpsc},
    time::Duration,
};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async, tungstenite::Message};
use url::Url;

pub struct Gateway {
    ws_tx: Arc<
        Mutex<futures_util::stream::SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>>,
    >,
    ws_rx:
        Arc<Mutex<futures_util::stream::SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>>>,
    events_tx: mpsc::Sender<Value>,
    events_rx: mpsc::Receiver<Value>,
    heartbeat_interval: Arc<Mutex<Option<u64>>>,
}

impl Gateway {
    pub async fn connect() -> Result<Self> {
        let url = Url::parse("wss://gateway.discord.gg/?v=10&encoding=json")?;
        let (ws, _) = connect_async(url).await?;
        let (ws_tx, ws_rx) = ws.split();

        let ws_tx = Arc::new(Mutex::new(ws_tx));
        let ws_rx = Arc::new(Mutex::new(ws_rx));
        let (events_tx, events_rx) = mpsc::channel(100);

        Ok(Self {
            ws_tx,
            ws_rx,
            events_tx,
            events_rx,
            heartbeat_interval: Arc::new(Mutex::new(None)),
        })
    }

    pub async fn start(&mut self, token: &str) -> Result<()> {
        let ws_rx = self.ws_rx.clone();
        let tx = self.events_tx.clone();
        let hb_interval = self.heartbeat_interval.clone();
        let ws_tx = self.ws_tx.clone();

        tokio::spawn(async move {
            while let Some(msg_result) = ws_rx.lock().await.next().await {
                match msg_result {
                    Ok(Message::Text(text)) => {
                        if let Ok(json) = serde_json::from_str::<Value>(&text) {
                            if json["op"] == 10
                                && let Some(interval) = json["d"]["heartbeat_interval"].as_u64() {
                                    println!(
                                        "💓 Hello received — heartbeat interval {}ms",
                                        interval
                                    );
                                    *hb_interval.lock().await = Some(interval);

                                    Self::spawn_heartbeat(ws_tx.clone(), interval);
                                }

                            let _ = tx.send(json).await;
                        }
                    }
                    Ok(Message::Close(_)) => {
                        println!("🔌 Gateway closed.");
                        break;
                    }
                    Err(e) => {
                        eprintln!("❌ WebSocket error: {:?}", e);
                        break;
                    }
                    _ => {}
                }
            }
        });

        self.identify(token).await?;
        Ok(())
    }

    async fn identify(&self, token: &str) -> Result<()> {
        let payload = serde_json::json!({
            "op": 2,
            "d": {
                "token": token,
                "intents": 641,
                "properties": {
                    "os": "windows",
                    "browser": "metalbot",
                    "device": "metalbot"
                }
            }
        });

        let mut ws = self.ws_tx.lock().await;
        ws.send(Message::Text(payload.to_string())).await?;
        println!("✅ Sent Identify");
        Ok(())
    }

    fn spawn_heartbeat(
        ws_tx: Arc<
            Mutex<
                futures_util::stream::SplitSink<
                    WebSocketStream<MaybeTlsStream<TcpStream>>,
                    Message,
                >,
            >,
        >,
        interval_ms: u64,
    ) {
        tokio::spawn(async move {
            let mut delay = tokio::time::interval(Duration::from_millis(interval_ms));
            loop {
                delay.tick().await;
                let heartbeat = serde_json::json!({ "op": 1, "d": null });
                let mut ws = ws_tx.lock().await;
                if ws.send(Message::Text(heartbeat.to_string())).await.is_ok() {
                    //  println!("❤️ Sent Heartbeat");
                } else {
                    println!("⚠️ Heartbeat failed");
                    break;
                }
            }
        });
    }

    pub async fn wait_for_voice_info(
        &mut self,
        guild_id: &str,
    ) -> Result<(String, String, String)> {
        let mut session_id = None;
        let mut token = None;
        let mut endpoint = None;

        println!("⏳ Waiting for VOICE_* events...");

        while let Some(event) = self.events_rx.recv().await {
            if let Some(t) = event["t"].as_str() {
                match t {
                    "VOICE_STATE_UPDATE" if event["d"]["guild_id"] == guild_id => {
                        session_id = event["d"]["session_id"].as_str().map(|s| s.to_string());
                    }
                    "VOICE_SERVER_UPDATE" if event["d"]["guild_id"] == guild_id => {
                        token = event["d"]["token"].as_str().map(|s| s.to_string());
                        endpoint = event["d"]["endpoint"].as_str().map(|s| s.to_string());
                    }
                    _ => {}
                }
            }

            if session_id.is_some() && token.is_some() && endpoint.is_some() {
                break;
            }
        }

        Ok((
            session_id.ok_or_else(|| anyhow::anyhow!("Missing session_id"))?,
            token.ok_or_else(|| anyhow::anyhow!("Missing token"))?,
            endpoint.ok_or_else(|| anyhow::anyhow!("Missing endpoint"))?,
        ))
    }

    pub async fn wait_until_ready(&mut self) -> Option<String> {
        println!("⏳ Waiting for READY / GUILD_CREATE...");
        while let Some(event) = self.events_rx.recv().await {
            if let Some(t) = event["t"].as_str() {
                match t {
                    "READY" => {
                        if let Some(user_id) = event["d"]["user"]["id"].as_str() {
                            println!("✅ READY — Bot ID: {}", user_id);
                            return Some(user_id.to_string());
                        }
                    }
                    "GUILD_CREATE" => {
                        println!("✅ GUILD_CREATE — initial guild loaded");
                    }
                    _ => {}
                }
            }
        }
        println!("⚠️ No READY event received before channel closed");
        None
    }

    pub async fn send_json(&self, payload: &serde_json::Value) -> Result<()> {
        let mut ws = self.ws_tx.lock().await;
        ws.send(Message::Text(payload.to_string())).await?;
        Ok(())
    }
}
