use crate::discord_voice_api::voice::ffmpeg::spawn_ffmpeg_with_buffer;
use crate::discord_voice_api::voice::player::{
    AudioFrame, FADE_SEC, FRAME_SIZE, Track, TrackQueue,
};
use anyhow::Result;
use std::sync::Arc;
use tokio::io::AsyncReadExt;
use tokio::sync::mpsc;
use tokio::time::Duration;
use crate::discord_voice_api::voice::audio_commands::AudioCommand;

pub async fn audio_producer(
    queue: Arc<TrackQueue>,
    tx: mpsc::Sender<AudioFrame>,
    mut playback_cmd_rx: mpsc::Receiver<AudioCommand>
) -> Result<(mpsc::Receiver<AudioCommand>)> {
    let mut current_proc: Option<tokio::process::Child> = None;
    let mut current_out: Option<tokio::process::ChildStdout> = None;
    let mut current_track: Option<Track> = None;
    let mut played_seconds: f64 = 0.0;
    let mut paused = false;

    struct CrossfadeState {
        proc: Option<tokio::process::Child>,
        out: Option<tokio::process::ChildStdout>,
        track: Option<Track>,
        fading: bool,
    }

    let mut crossfade = CrossfadeState {
        proc: None,
        out: None,
        track: None,
        fading: false,
    };

    let mut buf_curr = vec![0u8; FRAME_SIZE];
    let mut buf_next = vec![0u8; FRAME_SIZE];
    let frame_duration = 960.0 / 48000.0;

    async fn start_track(track: &Track) -> Result<(tokio::process::Child, tokio::process::ChildStdout)> {
        let url = track.url.as_ref().unwrap();
        spawn_ffmpeg_with_buffer(url, 64).await
    }

    loop {
        while let Ok(cmd) = playback_cmd_rx.try_recv() {
            match cmd {
                AudioCommand::Pause => {
                    paused = true;
                    println!("[PRODUCER] ⏸ Paused");
                }
                AudioCommand::Resume => {
                    paused = false;
                    println!("[PRODUCER] ▶ Resumed");
                }
                AudioCommand::Skip => {
                    println!("[PRODUCER] ⏭ Skipping track");
                    if let Some(mut proc) = current_proc.take() {
                        let _ = proc.kill().await;
                    }
                    current_out = None;
                    current_track = None;
                    continue;
                }
                _ => {}
            }
        }

        if paused {
            tokio::time::sleep(Duration::from_millis(20)).await;
            continue;
        }

        if current_out.is_none() {
            played_seconds = 0.0;
            if let Some(track) = queue.pop().await {
                queue.set_current_track(track.clone()).await;
                println!("[PRODUCER] ▶ Starting track: {}", track.title);
                let (proc, out) = start_track(&track).await?;
                current_proc = Some(proc);
                current_out = Some(out);
                current_track = Some(track);
            } else {
                println!("[PRODUCER] ✅ Queue finished.");
                queue.clear_current_track().await;
                break;
            }
        }

        let n = match current_out.as_mut() {
            Some(out) => out.read(&mut buf_curr).await.unwrap_or(0),
            None => continue,
        };

        let ffmpeg_alive = current_proc.as_mut()
            .map(|p| p.try_wait().map(|s| s.is_none()).unwrap_or(true))
            .unwrap_or(false);

        if n == 0 && !ffmpeg_alive && !crossfade.fading {
            println!(
                "[PRODUCER] ⏹ Track ended: {}",
                current_track.as_ref().map(|t| &t.title).unwrap_or(&"<unknown>".to_string())
            );
            current_proc = None;
            current_out = None;
            current_track = None;
            continue;
        }

        if n == 0 && ffmpeg_alive && !crossfade.fading {
            tokio::time::sleep(Duration::from_millis(50)).await;
            continue;
        }

        let pcm_curr: Vec<i16> = buf_curr.chunks_exact(2)
            .map(|b| i16::from_le_bytes([b[0], b[1]]))
            .collect();

        played_seconds += frame_duration;

        if !crossfade.fading {
            if let Some(track) = current_track.as_ref() {
                if let Some(total_dur) = track.duration {
                    if total_dur - played_seconds <= FADE_SEC {
                        if let Some(next_track) = queue.pop().await {
                            println!(
                                "[PRODUCER] 🔁 Initiating crossfade: {} → {}",
                                track.title, next_track.title
                            );
                            let (proc, out) = start_track(&next_track).await?;
                            crossfade.proc = Some(proc);
                            crossfade.out = Some(out);
                            crossfade.track = Some(next_track);
                            crossfade.fading = true;
                        }
                    }
                }
            }
        }

        let frame: Vec<i16> = if crossfade.fading {
            if let Some(no) = crossfade.out.as_mut() {
                if no.read_exact(&mut buf_next).await.is_ok() {
                    let pcm_next: Vec<i16> = buf_next.chunks_exact(2)
                        .map(|b| i16::from_le_bytes([b[0], b[1]]))
                        .collect();

                    let total_dur = current_track.as_ref()
                        .and_then(|t| t.duration)
                        .unwrap_or(FADE_SEC);

                    let fade_pos = ((FADE_SEC - (total_dur - played_seconds)) / FADE_SEC)
                        .clamp(0.0, 1.0);

                    let mixed: Vec<i16> = pcm_curr.iter()
                        .zip(pcm_next.iter())
                        .map(|(&a, &b)| ((a as f64 * (1.0 - fade_pos)) + (b as f64 * fade_pos)) as i16)
                        .collect();

                    if fade_pos >= 1.0 {
                        println!("[PRODUCER] ✅ Crossfade completed.");
                        if let Some(mut proc) = current_proc.take() {
                            let _ = proc.kill().await;
                        }
                        current_proc = crossfade.proc.take();
                        current_out = crossfade.out.take();
                        current_track = crossfade.track.take();
                        played_seconds = FADE_SEC;
                        crossfade.fading = false;
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

        if tx.send(frame).await.is_err() {
            println!("[PRODUCER] Consumer disconnected");
            break;
        }
    }

    Ok((playback_cmd_rx))
}