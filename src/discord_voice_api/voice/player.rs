use super::{consumer::audio_consumer, producer::audio_producer};
use crate::discord_voice_api::voice::connection::{VoiceConnection, VoiceSession};
use crate::discord_voice_api::voice::audio_commands::{
    AudioCommand, AudioFilterState, AudioFilters, SharedAudioFilters,
};
use anyhow::Result;
use serde::Deserialize;
use std::collections::VecDeque;
use std::sync::{
    Arc,
    atomic::{AtomicU16, AtomicU32},
};
use tokio::sync::{Mutex, RwLock, mpsc};

pub type AudioFrame = Vec<i16>;

pub const FRAME_SIZE: usize = 960 * 2 * 2;
pub const FADE_SEC: f64 = 8.0;
pub const BUFFER_FRAMES: usize = 100;

#[derive(Debug, Deserialize, Clone, Default)]
pub struct Track {
    pub id: String,
    pub title: String,
    pub duration: Option<f64>,
    pub thumbnail: Option<String>,
    pub url: Option<String>,
}

#[derive(Clone)]
pub struct TrackQueue {
    inner: Arc<Mutex<VecDeque<Track>>>,
    current_track: Arc<Mutex<Option<Track>>>,
}

impl TrackQueue {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(VecDeque::new())),
            current_track: Arc::new(Mutex::new(None)),
        }
    }

    pub async fn push(&self, track: Track) {
        let mut queue = self.inner.lock().await;
        queue.push_back(track);
    }

    pub async fn pop(&self) -> Option<Track> {
        let mut queue = self.inner.lock().await;
        queue.pop_front()
    }

    pub async fn is_empty(&self) -> bool {
        let queue = self.inner.lock().await;
        queue.is_empty()
    }

    pub async fn len(&self) -> usize {
        let queue = self.inner.lock().await;
        queue.len()
    }

    pub async fn iter(&self) -> Vec<Track> {
        let queue = self.inner.lock().await;
        queue.iter().cloned().collect()
    }

    pub async fn set_current_track(&self, track: Track) {
        let mut curr = self.current_track.lock().await;
        *curr = Some(track);
    }

    pub async fn get_current_track(&self) -> Option<Track> {
        let curr = self.current_track.lock().await;
        curr.clone()
    }

    pub async fn clear_current_track(&self) {
        let mut curr = self.current_track.lock().await;
        *curr = None;
    }
}

pub struct AudioPlayer {
    conn: VoiceConnection,
    session: Option<Arc<VoiceSession>>,
    queue: Arc<TrackQueue>,
    seq: Arc<AtomicU16>,
    timestamp: Arc<AtomicU32>,
    is_playing: Arc<Mutex<bool>>,
    pub audio_filter_state: Arc<RwLock<AudioFilterState>>,
    pub filter_cmd_tx: mpsc::Sender<AudioCommand>,
    filter_cmd_rx: Arc<Mutex<Option<mpsc::Receiver<AudioCommand>>>>,
    filters: SharedAudioFilters,
    pub playback_cmd_tx: mpsc::Sender<AudioCommand>,
    playback_cmd_rx: Arc<Mutex<Option<mpsc::Receiver<AudioCommand>>>>,
}

impl AudioPlayer {
    pub fn new(conn: VoiceConnection, session: VoiceSession) -> Arc<Self> {
        let (cmd_tx, cmd_rx) = mpsc::channel::<AudioCommand>(8);
        let (p_cmd_tx, p_cmd_rx) = mpsc::channel::<AudioCommand>(8);

        let filter_state = Arc::new(RwLock::new(AudioFilterState::default()));

        Arc::new(Self {
            conn,
            session: Some(Arc::new(session)),
            queue: Arc::new(TrackQueue::new()),
            seq: Arc::new(AtomicU16::new(0)),
            timestamp: Arc::new(AtomicU32::new(0)),
            is_playing: Arc::new(Mutex::new(false)),
            audio_filter_state: filter_state,
            filter_cmd_tx: cmd_tx,
            filter_cmd_rx: Arc::new(Mutex::new(Some(cmd_rx))),
            playback_cmd_tx: p_cmd_tx,
            playback_cmd_rx: Arc::new(Mutex::new(Some(p_cmd_rx))),
            filters: Arc::new(Mutex::new(AudioFilters::new(48_000.0))),
        })
    }

    pub fn get_queue(&self) -> Arc<TrackQueue> {
        self.queue.clone()
    }

    pub async fn enqueue(self: Arc<Self>, track: Track) {
        {
            let mut q = self.queue.inner.lock().await;
            q.push_back(track.clone());
        }
        println!("[ENQUEUE] Track added: {}", track.title);

        let mut playing = self.is_playing.lock().await;
        if !*playing {
            *playing = true;
            println!("[ENQUEUE] Starting queue processing");
            let player_clone = Arc::clone(&self);
            let cmd_rx = {
                let mut rx_lock = player_clone.filter_cmd_rx.lock().await;
                rx_lock.take()
            };
            let playback_cmd_rx = {
                let mut rx_lock = player_clone.playback_cmd_rx.lock().await;
                rx_lock.take()
            };

            match (cmd_rx, playback_cmd_rx) {
                (Some(cmd_rx), Some(playback_cmd_rx)) => {
                    tokio::spawn(async move {
                        if let Err(e) = player_clone.process_queue(cmd_rx, playback_cmd_rx).await {
                            eprintln!("[AudioPlayer] Queue processing failed: {e:?}");
                        }
                    });
                }
                _ => println!("[ENQUEUE] Channels missing or already active"),
            }
        }
    }

    async fn process_queue(self: Arc<Self>, cmd_rx: mpsc::Receiver<AudioCommand>, playback_cmd_rx: mpsc::Receiver<AudioCommand>) -> Result<()> {
        let (tx, rx) = mpsc::channel::<AudioFrame>(BUFFER_FRAMES);

        let q = self.queue.clone();
        let conn = Arc::new(self.conn.clone());
        let seq = self.seq.clone();
        let ts = self.timestamp.clone();

        let prod = tokio::spawn(audio_producer(q, tx, playback_cmd_rx));
        let cons = tokio::spawn(audio_consumer(
            conn,
            seq,
            ts,
            rx,
            cmd_rx,
            self.audio_filter_state.clone(),
            self.filters.clone(),
        ));

        let join = tokio::try_join!(prod, cons)?;

        let (filter_cmd_res, playback_cmd_res) = join;

        let filter_cmd_rx = filter_cmd_res?;
        let playback_cmd_rx = playback_cmd_res?;

        self.filter_cmd_rx.lock().await.replace(filter_cmd_rx);
        self.playback_cmd_rx.lock().await.replace(playback_cmd_rx);

        {
            let mut playing = self.is_playing.lock().await;
            *playing = false;
        }

        Ok(())
    }
}

/*
       // Bot-Speaking OFF
       let stop_payload = json!({
           "op": 5,
           "d": { "speaking": 0, "delay": 0, "ssrc": self.conn.ssrc }
       });
       {
           let mut w = self.conn.ws.lock().await;
           w.send(Message::Text(stop_payload.to_string())).await?;
       }

       println!(
           "🔇 Bot stopped speaking ({} ended)",
           if is_stream { "stream" } else { "file" }
       );
*/