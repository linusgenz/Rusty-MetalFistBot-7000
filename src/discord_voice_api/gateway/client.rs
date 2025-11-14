use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use socket2::{Domain, Socket, Type};
use std::sync::Arc;
use tokio::{
    net::TcpStream,
    sync::{Mutex, mpsc},
    time::Duration,
};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, client_async, connect_async, tungstenite::Message,
};
use url::Url;

pub struct Gateway {
    ws_tx: Arc<
        Mutex<futures_util::stream::SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>>,
    >,
    ws_rx:
        Arc<Mutex<futures_util::stream::SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>>>,
    events_tx: mpsc::Sender<Value>,
    heartbeat_interval: Arc<Mutex<Option<u64>>>,
    session_id: Arc<Mutex<Option<String>>>,
    resume_url: Arc<Mutex<Option<String>>>,
    last_seq: Arc<Mutex<Option<i64>>>,
}

impl Gateway {
    pub async fn connect() -> Result<(Self, mpsc::Receiver<Value>)> {
        let url = Url::parse("wss://gateway.discord.gg/?v=10&encoding=json")?;
        let (ws, _) = connect_async(url).await?;
        let (ws_tx, ws_rx) = ws.split();

        let ws_tx = Arc::new(Mutex::new(ws_tx));
        let ws_rx = Arc::new(Mutex::new(ws_rx));
        let (events_tx, events_rx) = mpsc::channel(100);

        let gw = Self {
            ws_tx,
            ws_rx,
            events_tx,
            heartbeat_interval: Arc::new(Mutex::new(None)),
            session_id: Arc::new(Mutex::new(None)),
            resume_url: Arc::new(Mutex::new(None)),
            last_seq: Arc::new(Mutex::new(None)),
        };

        Ok((gw, events_rx))
    }

    fn clone_for_task(&self) -> Self {
        Self {
            ws_tx: self.ws_tx.clone(),
            ws_rx: self.ws_rx.clone(),
            events_tx: self.events_tx.clone(),
            heartbeat_interval: self.heartbeat_interval.clone(),
            session_id: self.session_id.clone(),
            resume_url: self.resume_url.clone(),
            last_seq: self.last_seq.clone(),
        }
    }

    pub async fn start(&mut self, token: &str) -> Result<()> {
        let token = token.to_string();

            self.gateway_loop(&*token).await;

        Ok(())
    }

    pub async fn gateway_loop(&mut self, token: &str) {
        loop {
            println!("🔌 Starting gateway cycle…");

            match self.run_gateway_cycle(token).await {
                Ok(()) => {
                    println!("Gateway exited cleanly.");
                    return; // beendet Hintergrundtask
                }
                Err(err) => {
                    eprintln!("⚠️ Error: {err:?}");
                    println!("⏳ Reconnecting in 5s…");
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
            }
        }
    }

    pub async fn run_gateway_cycle(&mut self, token: &str) -> Result<()> {
        let mut trying_resume = false;

        {
            let sid = self.session_id.lock().await.clone();
            let url = self.resume_url.lock().await.clone();
            let seq = self.last_seq.lock().await.clone();

            if sid.is_some() && url.is_some() && seq.is_some() {
                trying_resume = true;
                println!("🔁 Trying resume instead of identify…");
            }
        }

        if trying_resume {
            self.reconnect_ws(true).await?;
        } else {
            self.reconnect_ws(false).await?;
        }

        let mut this = self.clone_for_task();
        tokio::spawn(async move {
            this.listen_loop().await.expect("Listen Loop exited");
        });

        if trying_resume {
            match self.send_resume(token).await {
                Ok(()) => println!("🔁 Sent RESUME"),
                Err(e) => {
                    println!("❌ Resume failed, falling back to IDENTIFY: {e:?}");
                    self.identify(token).await?;
                }
            }
        } else {
            self.identify(token).await?;
        }

        Ok(())
    }

    async fn reconnect_ws(&mut self, use_resume_url: bool) -> Result<()> {
        let url = if use_resume_url {
            if let Some(url) = self.resume_url.lock().await.clone() {
                Url::parse(&(url + "?v=10&encoding=json"))?
            } else {
                Url::parse("wss://gateway.discord.gg/?v=10&encoding=json")?
            }
        } else {
            Url::parse("wss://gateway.discord.gg/?v=10&encoding=json")?
        };

        println!("🌐 Connecting to {}", url);

        let (ws, _) = connect_async(url).await?;
        let (tx, rx) = ws.split();

        self.ws_tx = Arc::new(Mutex::new(tx));
        self.ws_rx = Arc::new(Mutex::new(rx));

        Ok(())
    }

    pub async fn listen_loop(&mut self) -> Result<()> {
        loop {
            let msg = self.ws_rx.lock().await.next().await;

            let msg = match msg {
                Some(Ok(m)) => m,
                Some(Err(e)) => return Err(anyhow::anyhow!(e)),
                None => return Err(anyhow::anyhow!("WS closed")),
            };

            match msg {
                Message::Text(text) => {
                    let json: Value = serde_json::from_str(&text)?;
                    let op = json["op"].as_i64().unwrap_or(-1);

                    match op {
                        // HELLO
                        10 => {
                            if let Some(interval) = json["d"]["heartbeat_interval"].as_u64() {
                                *self.heartbeat_interval.lock().await = Some(interval);
                                Self::spawn_heartbeat(self.ws_tx.clone(), interval);
                            }
                        }

                        // DISPATCH
                        0 => {
                            if let Some(s) = json["s"].as_i64() {
                                *self.last_seq.lock().await = Some(s);
                            }
                            match json["t"].as_str().unwrap_or("") {
                                "READY" => {
                                    *self.session_id.lock().await =
                                        json["d"]["session_id"].as_str().map(|s| s.to_string());
                                    *self.resume_url.lock().await =
                                        json["d"]["resume_gateway_url"]
                                            .as_str()
                                            .map(|s| s.to_string());
                                }

                                "RESUMED" => {
                                    println!("🔁 RESUMED — replay complete");
                                }

                                _ => {}
                            }
                        }

                        // RECONNECT
                        7 => {
                            println!("🔁 Discord requested reconnect (op 7)");
                            return Err(anyhow::anyhow!("Discord RECONNECT"));
                        }

                        // INVALID SESSION
                        9 => {
                            let can_resume = json["d"].as_bool().unwrap_or(false);
                            if can_resume {
                                println!("⚠️ Invalid Session — resume allowed");
                                return Err(anyhow::anyhow!("InvalidSessionResume"));
                            } else {
                                println!("❌ Invalid Session — cannot resume");
                                *self.session_id.lock().await = None;
                                return Err(anyhow::anyhow!("InvalidSessionNoResume"));
                            }
                        }

                        _ => {}
                    }

                    let _ = self.events_tx.send(json).await;
                }

                Message::Close(c) => {
                    println!("🔌 Gateway closed: {:?}", c);
                    return Err(anyhow::anyhow!("Closed"));
                }

                _ => {}
            }
        }
    }


    async fn send_resume(&self, token: &str) -> Result<()> {
        let sid = self.session_id.lock().await.clone();
        let seq = self.last_seq.lock().await.clone();

        let (sid, seq) = match (sid, seq) {
            (Some(sid), Some(seq)) => (sid, seq),
            _ => return Err(anyhow::anyhow!("Missing resume data")),
        };

        let payload = serde_json::json!({
        "op": 6,
        "d": {
            "token": token,
            "session_id": sid,
            "seq": seq
        }
    });

        self.ws_tx.lock().await.send(Message::Text(payload.to_string())).await?;
        println!("🔁 Sent Resume packet");
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
        mut events_rx: mpsc::Receiver<Value>
    ) -> Result<(String, String, String)> {
        let mut session_id = None;
        let mut token = None;
        let mut endpoint = None;

        println!("⏳ Waiting for VOICE_* events...");

        while let Some(event) = events_rx.recv().await {
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

    pub async fn wait_until_ready(&mut self,         mut events_rx: mpsc::Receiver<Value>
    ) -> (Option<(mpsc::Receiver<Value>, String)>) {
        println!("⏳ Waiting for READY / GUILD_CREATE...");
        while let Some(event) = events_rx.recv().await {
            if let Some(t) = event["t"].as_str() {
                match t {
                    "READY" => {
                        if let Some(user_id) = event["d"]["user"]["id"].as_str() {
                            println!("✅ READY — Bot ID: {}", user_id);
                            return Some((events_rx, user_id.to_string()));
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
