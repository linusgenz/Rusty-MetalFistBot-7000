use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use std::sync::{Arc, Mutex};
use std::sync::atomic::AtomicU32;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::net::UdpSocket;
use tokio::time::sleep;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async, tungstenite::Message};
use url::Url;

use crate::discord_voice_api::udp::{handshake, setup};
use crate::discord_voice_api::voice::crypto::CipherMode;

#[derive(Clone)]
pub struct VoiceConnection {
    pub socket: Arc<UdpSocket>,
    pub ssrc: u32,
    pub mode: String,
    pub cipher: CipherMode,
    pub counter: Arc<AtomicU32>,
}

#[derive(Clone)]
pub struct VoiceSession {
    pub ws: Arc<
        tokio::sync::Mutex<
            futures_util::stream::SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>,
        >,
    >,
}

unsafe impl Send for VoiceConnection {}
unsafe impl Sync for VoiceConnection {}

impl VoiceConnection {
    pub async fn connect(
        endpoint: String,
        token: String,
        session_id: String,
        guild_id: &str,
        user_id: String,
    ) -> Result<(Self, VoiceSession)> {
        let url = format!("wss://{}", endpoint);
        let (ws, _) = connect_async(Url::parse(&url)?).await?;
        let (tx, rx) = ws.split();
        let ws_tx = Arc::new(tokio::sync::Mutex::new(tx));
        let ws_rx = Arc::new(tokio::sync::Mutex::new(rx));

        // Identify (Voice)
        {
            let identify = json!({
                "op": 0,
                "d": {
                    "server_id": guild_id,
                    "user_id": user_id,
                    "session_id": session_id,
                    "token": token
                }
            });
            ws_tx
                .lock()
                .await
                .send(Message::Text(identify.to_string()))
                .await?;
        }
        println!("🎧 Sent Voice Identify");

        // Wait for HELLO
        let heartbeat_interval = handshake::wait_for_hello(&ws_rx).await?;
        println!("📡 Voice heartbeat interval: {}ms", heartbeat_interval);

        // Spawn heartbeat loop
        {
            let hb_tx = ws_tx.clone();
            tokio::spawn(async move {
                let interval = Duration::from_millis(heartbeat_interval);
                loop {
                    sleep(interval).await;
                    let heartbeat = json!({ "op": 3, "d": chrono::Utc::now().timestamp_millis() });
                    if let Err(e) = hb_tx
                        .lock()
                        .await
                        .send(Message::Text(heartbeat.to_string()))
                        .await
                    {
                        eprintln!("Fehler beim Senden des Voice Heartbeats: {:?}", e);
                        break;
                    }
                    //  println!("❤️ Voice Heartbeat gesendet");
                }
            });
        }

        // READY (ssrc, ip, port)
        let (ssrc, server_ip, server_port) = handshake::wait_for_ready(&ws_rx).await?;
        println!(
            "✅ Voice Ready received! {}:{} (ssrc={})",
            server_ip, server_port, ssrc
        );

        // UDP socket
        /*let udp_socket = Arc::new({
            let s = UdpSocket::bind("0.0.0.0:0").await?;
            s.connect((server_ip.as_str(), server_port)).await?;
            s
        });*/
        let udp_socket = Arc::new({
            let s = setup::make_udp_socket("0.0.0.0:0").await?;
            s.connect((server_ip.as_str(), server_port)).await?;
            s
        });


        // IP discovery
        let (address, port) = setup::discover_ip(ssrc, &udp_socket).await?;
        println!("🌍 Discovered external IP: {}:{}", address, port);

        // SELECT_PROTOCOL
        {
            let select_protocol = json!({
                "op": 1,
                "d": {
                    "protocol": "udp",
                    "data": {
                        "address": address,
                        "port": port,
                        "mode": "aead_xchacha20_poly1305_rtpsize"
                    }
                }
            });

            let mut w = ws_tx.lock().await;
            w.send(Message::Text(select_protocol.to_string())).await?;
        }

        // SECRET KEY
        let (secret_key, mode) = handshake::wait_for_secret(&ws_rx).await?;
        let cipher = CipherMode::from_secret_and_mode(&secret_key, &mode)?;
        println!("🔑 Received secret key, mode: {}", mode);

        {
            let ws_rx_clone = ws_rx.clone();
            tokio::spawn(async move {
                loop {
                    let msg_opt = {
                        let mut lock = ws_rx_clone.lock().await;
                        lock.next().await
                    };
                    match msg_opt {
                        Some(Ok(Message::Text(txt))) => {
                            //  println!("📨 WS-Text: {}", txt);
                        }
                        Some(Ok(_)) => continue,
                        Some(Err(e)) => {
                            eprintln!("Fehler beim Lesen des Voice WebSockets: {:?}", e);
                            break;
                        }
                        None => {
                            eprintln!("Voice WebSocket wurde geschlossen");
                            break;
                        }
                    }
                }
            });
        }

        let conn = VoiceConnection {
            socket: udp_socket,
            ssrc,
            mode: mode.clone(),
            cipher,
            counter: Arc::new(AtomicU32::new(0)),
        };

        let session = VoiceSession { ws: ws_tx };

        Ok((conn, session))
    }
}
