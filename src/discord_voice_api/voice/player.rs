use super::{consumer::audio_consumer, producer::audio_producer};
use crate::discord_voice_api::voice::connection::{VoiceConnection, VoiceSession};
use crate::discord_voice_api::voice::filters::{
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
}

impl AudioPlayer {
    pub fn new(conn: VoiceConnection, session: VoiceSession) -> Arc<Self> {
        let (cmd_tx, cmd_rx) = mpsc::channel::<AudioCommand>(8);
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

            if let Some(cmd_rx) = cmd_rx {
                tokio::spawn(async move {
                    if let Err(e) = player_clone.process_queue(cmd_rx).await {
                        eprintln!("[AudioPlayer] Queue processing failed: {e:?}");
                    }
                });
            } else {
                println!("[ENQUEUE] Queue processing already active");
            }
        }
    }

    async fn process_queue(self: Arc<Self>, cmd_rx: mpsc::Receiver<AudioCommand>) -> Result<()> {
        let (tx, rx) = mpsc::channel::<AudioFrame>(BUFFER_FRAMES);

        let q = self.queue.clone();
        let conn = Arc::new(self.conn.clone());
        let seq = self.seq.clone();
        let ts = self.timestamp.clone();

        let prod = tokio::spawn(audio_producer(q, tx));
        let cons = tokio::spawn(audio_consumer(
            conn,
            seq,
            ts,
            rx,
            cmd_rx,
            self.audio_filter_state.clone(),
            self.filters.clone(),
        ));

        let _ = tokio::try_join!(prod, cons);
        Ok(())
    }
}

/*
impl AudioPlayer {
    pub fn new(conn: VoiceConnection, session: VoiceSession) -> Self {
        Self {
            conn,
            session: Some(Arc::new(session)),
            queue: Arc::new(TrackQueue::new()),
            seq: Arc::new(AtomicU16::new(0)),
            timestamp: Arc::new(AtomicU32::new(0)),
        }
    }

    pub async fn enqueue(self: Arc<Self>, track: Track) {
        {
            let mut q = self.queue.inner.lock().await;
            q.push_back(track.clone());
        }
        println!("[ENQUEUE] Track added: {}", track.title);

        let mut playing = self.queue.is_playing.lock().await;
        if !*playing {
            *playing = true;
            println!("[ENQUEUE] Starting queue processing");
            let player_clone = Arc::clone(&self);
            tokio::spawn(async move {
                player_clone.process_queue().await;
            });
        }
    }

    // === MAIN: Startet die Pipeline ===
    async fn process_queue(self: Arc<Self>) -> Result<()> {
        println!("[QUEUE] Start processing with pipeline…");

        // Channel mit Buffer für ~3 Sekunden
        let (tx, rx) = mpsc::channel::<AudioFrame>(BUFFER_FRAMES);

        let queue = self.queue.clone();
        let conn = Arc::new(self.conn.clone());
        let seq = self.seq.clone();
        let timestamp = self.timestamp.clone();

        let producer = tokio::spawn(async move {
            if let Err(e) = audio_producer(queue, tx).await {
                eprintln!("[PRODUCER] Error: {}", e);
            }
        });

        let consumer = tokio::spawn(async move {
            if let Err(e) = audio_consumer(conn, seq, timestamp, rx).await {
                eprintln!("[CONSUMER] Error: {}", e);
            }
        });

        // Warten bis beide fertig sind
        let _ = tokio::try_join!(producer, consumer);

        Ok(())
    }


    pub async fn play_file(&self, path: &str) -> Result<()> {
        /* let conn = self.conn.clone();
        let path = path.to_string();
        tokio::spawn(async move {
            let player = AudioPlayer::new(conn);
            if let Err(e) = player.play_internal(&path, false).await {
                eprintln!("File playback error: {:?}", e);
            }
        });*/
        Ok(())
    }

    pub async fn play_stream(&self, meta: &Track) -> Result<()> {
        /*let stream_url = meta
                    .url
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("No stream URL found"))?
                    .to_string();

                let conn = self.conn.clone();
                let session = self.
                tokio::spawn(async move {
                    let player = AudioPlayer::new(conn, /* VoiceSession */);
                    if let Err(e) = player.play_internal(&stream_url, true).await {
                        eprintln!("Stream playback error: {:?}", e);
                    }
                });
        */
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


async fn spawn_ffmpeg_with_buffer(
    url: &str,
    buffer_size: usize,
) -> anyhow::Result<(tokio::process::Child, tokio::process::ChildStdout)> {
    use tokio::{io::AsyncWriteExt, process::Command as TokioCommand};
    use futures_util::StreamExt;
    use std::time::Duration;

    let args = [
        "-i", "pipe:0",
        "-f", "s16le", "-ar", "48000", "-ac", "2", "pipe:1",
    ];

    let mut child = TokioCommand::new("ffmpeg")
        .args(&args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()?;

    let mut ffmpeg_stdin = child.stdin.take().expect("child stdin");
    let ffmpeg_stdout = child.stdout.take().expect("child stdout");

    let (tx, mut rx) = tokio::sync::mpsc::channel::<Vec<u8>>(buffer_size);
    let url_owned = url.to_string();

    // --- Range-basierter Fetcher ---
    tokio::spawn(async move {
        let mut start: u64 = 0;
        let chunk_size: u64 = 256 * 1024; // 256 KB

        // Zuerst HEAD-Request machen, um die Content-Length zu bestimmen
        let client = reqwest::Client::new();
        let total_size = match client.head(&url_owned).send().await {
            Ok(r) => r
                .headers()
                .get(reqwest::header::CONTENT_LENGTH)
                .and_then(|h| h.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(0),
            Err(e) => {
                eprintln!("[FETCHER] Failed to get content length: {e}");
                0
            }
        };

        println!("[FETCHER] Total size: {} bytes", total_size);

        while start < total_size {
            let end = (start + chunk_size - 1).min(total_size - 1);
            let range_header = format!("bytes={}-{}", start, end);

            let resp = client
                .get(&url_owned)
                .header("Range", range_header)
                .send()
                .await;

            match resp {
                Ok(r) => {
                    let mut stream = r.bytes_stream();
                    while let Some(chunk) = stream.next().await {
                        match chunk {
                            Ok(bytes) => {
                                start += bytes.len() as u64;
                                if tx.send(bytes.to_vec()).await.is_err() {
                                    println!("[FETCHER] Channel closed");
                                    return;
                                }
                            }
                            Err(e) => {
                                eprintln!("[FETCHER] Read error: {e}, retrying in 1s");
                                tokio::time::sleep(Duration::from_secs(1)).await;
                                break;
                            }
                        }
                    }
                }
                Err(e) => {
                    eprintln!("[FETCHER] HTTP error: {e}, retrying in 2s");
                    tokio::time::sleep(Duration::from_secs(2)).await;
                }
            }
        }

        println!("[FETCHER] ✅ Finished downloading stream.");
    });

    // --- Feeder ---
    tokio::spawn(async move {
        while let Some(buf) = rx.recv().await {
            if ffmpeg_stdin.write_all(&buf).await.is_err() {
                break;
            }
        }
        let _ = ffmpeg_stdin.shutdown().await;
        println!("[FEEDER] Input stream closed");
    });

    Ok((child, ffmpeg_stdout))
}

// === PRODUCER: Lädt Tracks und produziert Audio-Frames ===
async fn audio_producer(
    queue: Arc<TrackQueue>,
    tx: mpsc::Sender<AudioFrame>,
) -> Result<()> {
    let mut current_proc: Option<tokio::process::Child> = None;
    let mut current_out: Option<tokio::process::ChildStdout> = None;
    let mut current_track: Option<Track> = None;
    let mut played_seconds: f64 = 0.0;
  //  let mut played_seconds_next: f64 = 0.0;

    let mut next_proc: Option<tokio::process::Child> = None;
    let mut next_out: Option<tokio::process::ChildStdout> = None;
    let mut next_track: Option<Track> = None;
    let mut fading = false;

    let mut buf_curr = vec![0u8; FRAME_SIZE];
    let mut buf_next = vec![0u8; FRAME_SIZE];
    let frame_duration = 960.0 / 48000.0;

    loop {
        // Neuen Track starten
        if current_out.is_none() {
            played_seconds = 0.0;
            println!("[PRODUCER] current track process is none, poping track from queue");
            if !fading {
                played_seconds += frame_duration;
            }
            let next = {
                let mut q = queue.inner.lock().await;
                q.pop_front()
            };

            if let Some(track) = next {
                println!("[PRODUCER] ▶ Starting track: {}", track.title);
                let (proc, out) = spawn_ffmpeg_with_buffer(track.url.as_ref().unwrap(), 64).await?;
                current_proc = Some(proc);
                current_out = Some(out);
                current_track = Some(track);
                played_seconds = 0.0; // TODO played_seconds aktualisieren wenn im neuen track, irgenfwie buffern 8 sekunden extra
            } else {
                println!("[PRODUCER] ✅ Queue finished.");
                break;
            }
        }

        // Frame vom aktuellen Track lesen
        let out_ref = match current_out.as_mut() {
            Some(o) => o,
            None => continue,
        };

        let n = out_ref.read(&mut buf_curr).await.unwrap_or(0);

        // Wenn keine Daten gelesen wurden, prüfen, ob ffmpeg wirklich beendet ist
        let mut curr_ok = n > 0;

        if n == 0 {
            // Prüfen, ob ffmpeg-Prozess noch läuft
            let ffmpeg_alive = if let Some(proc) = current_proc.as_mut() {
                match proc.try_wait() {
                    Ok(Some(status)) => {
                        if !status.success() {
                            eprintln!("[WARN] ffmpeg exited early: {:?}", status);
                        }
                        false
                    }
                    Ok(None) => true, // Prozess läuft noch
                    Err(e) => {
                        eprintln!("[WARN] Failed to check ffmpeg status: {e}");
                        true
                    }
                }
            } else {
                false
            };

            if ffmpeg_alive {
                // ffmpeg läuft noch – also wahrscheinlich kurz kein Audio verfügbar
                tokio::time::sleep(Duration::from_millis(50)).await;
                continue; // → nächste Iteration versuchen
            } else {
                curr_ok = false;
            }
        }

        if !curr_ok && !fading {
            println!(
                "[PRODUCER] ⏹ Track ended: {} duration: {:.2}, {:?}",
                current_track.as_ref().map(|t| &t.title).unwrap_or(&"<unknown>".to_string()),
                played_seconds,
                current_track.as_ref().and_then(|t| t.duration)
            );
            current_proc = None;
            current_out = None;
            current_track = None;
            continue;
        }

        // Track zu Ende während Crossfade → mit Stille füllen
        if n == 0 && fading {
            buf_curr.fill(0);
        }

        let pcm_curr: Vec<i16> = buf_curr
            .chunks_exact(2)
            .map(|b| i16::from_le_bytes([b[0], b[1]]))
            .collect();

        played_seconds += frame_duration;

        // === Crossfade initiieren ===
        if !fading {
            if let Some(total_dur) = current_track.as_ref().and_then(|t| t.duration) {
                let remaining = total_dur - played_seconds;
                if remaining <= FADE_SEC {
                    // Nächsten Track aus Queue holen
                    let maybe_next = {
                        let mut q = queue.inner.lock().await;
                        q.pop_front()
                    };

                    if let Some(nt) = maybe_next {
                        println!("[PRODUCER] 🔁 Initiating crossfade: {} → {}",
                                 current_track.as_ref().unwrap().title, nt.title);
                        let (p, o) = spawn_ffmpeg_with_buffer(nt.url.as_ref().unwrap(), 64).await?;
                        next_proc = Some(p);
                        next_out = Some(o);
                        next_track = Some(nt);

//                        played_seconds_next = 0.0;

                        fading = true;
                    }
                }
            }
        }

        // === Audio Frame produzieren ===
        let frame = if fading {
            if let Some(no) = next_out.as_mut() {
                let next_ok = no.read_exact(&mut buf_next).await.is_ok();

                if next_ok {
                    // Crossfade mixen
                    let pcm_next: Vec<i16> = buf_next
                        .chunks_exact(2)
                        .map(|b| i16::from_le_bytes([b[0], b[1]]))
                        .collect();

                //    played_seconds_next += frame_duration;

                    let total_dur = current_track.as_ref()
                        .and_then(|t| t.duration)
                        .unwrap_or(FADE_SEC);

                    // Fortschritt des Fades (0.0 → 1.0)
                    let fade_pos = ((FADE_SEC - (total_dur - played_seconds)) / FADE_SEC)
                        .clamp(0.0, 1.0);

                    let mixed: Vec<i16> = pcm_curr
                        .iter()
                        .zip(pcm_next.iter())
                        .map(|(&a, &b)| {
                            let s = (a as f64 * (1.0 - fade_pos)) + (b as f64 * fade_pos);
                            s as i16
                        })
                        .collect();

                    // --- NEU: Crossfade hier beenden, wenn Zeit abgelaufen ---
                    if fade_pos >= 1.0 {
                        println!("[PRODUCER] ✅ Crossfade completed (fade duration reached).");

                        if let Some(mut proc) = current_proc.take() {
                            let _ = proc.kill().await;
                        }

                        // Neuen Track übernehmen
                        current_proc = next_proc.take();
                        current_out = next_out.take();
                        current_track = next_track.take();

                        played_seconds = FADE_SEC;
                        fading = false;

                        buf_next.fill(0);
                    }

                    mixed
                } else {
                    pcm_curr
                }
            } else {
                pcm_curr
            }
        } else {
            pcm_curr
        };

        // Frame in Channel senden (blockiert wenn voll → Backpressure)
        if tx.send(frame).await.is_err() {
            println!("[PRODUCER] Consumer disconnected");
            break;
        }
    }

    Ok(())
}

// === CONSUMER: Sendet Audio-Frames an Discord ===
async fn audio_consumer(
    conn: Arc<VoiceConnection>,
    seq: Arc<AtomicU16>,
    timestamp: Arc<AtomicU32>,
    mut rx: mpsc::Receiver<AudioFrame>,
) -> Result<()> {
    let mut encoder = Encoder::new(48000, Channels::Stereo, Application::Audio)?;
    let mut interval = interval(Duration::from_millis(20));
    interval.set_missed_tick_behavior(MissedTickBehavior::Delay);

    let mut seq_val = seq.load(Ordering::Relaxed);
    let mut ts_val = timestamp.load(Ordering::Relaxed);

    println!("[CONSUMER] Ready to send audio");

    while let Some(frame) = rx.recv().await {
        interval.tick().await;

        send_voice_packet(&conn, &frame, &mut encoder, seq_val, ts_val).await?;

        seq_val = seq_val.wrapping_add(1);
        ts_val = ts_val.wrapping_add(960);
    }

    seq.store(seq_val, Ordering::Relaxed);
    timestamp.store(ts_val, Ordering::Relaxed);

    println!("[CONSUMER] Finished");
    Ok(())
}
*/
