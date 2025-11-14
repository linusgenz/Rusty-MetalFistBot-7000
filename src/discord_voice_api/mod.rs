use crate::discord_voice_api::gateway::Gateway;
use crate::discord_voice_api::voice::VoiceConnection;
use crate::discord_voice_api::voice::player::AudioPlayer;
use anyhow::Result;
use futures_util::SinkExt;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_tungstenite::tungstenite::Message;

pub mod gateway;
pub mod udp;
pub mod voice;

pub struct DiscordVoiceApi {
    connections: Arc<Mutex<HashMap<String, Arc<AudioPlayer>>>>, // key = guild_id
}

impl DiscordVoiceApi {
    pub fn new() -> Self {
        Self {
            connections: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn get_player(&self, guild_id: &str) -> Option<Arc<AudioPlayer>> {
        let conns = self.connections.lock().await;

        conns.get(guild_id).cloned()
    }

    pub async fn join(
        &self,
        token: &str,
        guild_id: &str,
        channel_id: &str,
    ) -> Result<Arc<AudioPlayer>> {
        if let Some(player) = self.get_player(guild_id).await {
            return Ok(player);
        }

        let mut res = Gateway::connect().await?;
        
        let mut gateway = res.0;
        let event_rx = res.1;

        gateway.start(token).await?;

        let (event_rx, user_id) = gateway
            .wait_until_ready(event_rx)
            .await
            .ok_or_else(|| anyhow::anyhow!("No READY event received"))?;

        let join_payload = serde_json::json!({
            "op": 4,
            "d": {
                "guild_id": guild_id,
                "channel_id": channel_id,
                "self_mute": false,
                "self_deaf": false
            }
        });
        gateway.send_json(&join_payload).await?;
        println!("🎤 Sent Voice State Update (JOIN)");

        let (session_id, voice_token, endpoint) = gateway.wait_for_voice_info(guild_id, event_rx).await?;
        println!("✅ Got Voice Info — endpoint: {}", endpoint);

        let voice_conn =
            VoiceConnection::connect(endpoint, voice_token, session_id, guild_id, user_id).await?;

        let player = AudioPlayer::new(voice_conn.0.clone(), voice_conn.1.clone());

        // Bot-Speaking ON
        let speaking_payload = json!({
            "op": 5,
            "d": { "speaking": 5, "delay": 0, "ssrc": voice_conn.0.ssrc }
        });
        {
            let mut w = voice_conn.1.ws.lock().await;
            w.send(Message::Text(speaking_payload.to_string())).await?;
        }

        let mut conns = self.connections.lock().await;
        conns.insert(guild_id.to_string(), player.clone());

        Ok(player)
    }

    /* pub async fn leave(&self, guild_id: &str) -> Result<()> {
        let mut conns = self.connections.lock().await;
        if let Some(player) = conns.remove(guild_id) {
            player.conn.disconnect().await?; // Annahme: VoiceConnection hat eine disconnect-Methode
            println!("🛑 Left voice channel for guild {}", guild_id);
        }
        Ok(())
    }*/
}
