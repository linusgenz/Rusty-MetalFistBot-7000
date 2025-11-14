use crate::discord_voice_api::udp::send_packet::send_voice_packet;
use crate::discord_voice_api::voice::connection::VoiceConnection;
use crate::discord_voice_api::voice::audio_commands::{
    AudioCommand, SharedAudioFilterState, SharedAudioFilters,
};
use crate::discord_voice_api::voice::player::AudioFrame;
use anyhow::Result;
use opus::{Application, Channels, Encoder};
use std::sync::Arc;
use std::sync::atomic::{AtomicU16, AtomicU32, Ordering};
use tokio::sync::mpsc;
use tokio::time::{Duration, MissedTickBehavior};

pub async fn audio_consumer(
    conn: Arc<VoiceConnection>,
    seq: Arc<AtomicU16>,
    ts: Arc<AtomicU32>,
    mut rx: mpsc::Receiver<AudioFrame>,
    mut cmd_rx: mpsc::Receiver<AudioCommand>,
    filter_state: SharedAudioFilterState,
    filters: SharedAudioFilters,
) -> Result<mpsc::Receiver<AudioCommand>, anyhow::Error> {
    let mut encoder = Encoder::new(48000, Channels::Stereo, Application::Audio)?;
    let mut tick = tokio::time::interval(Duration::from_millis(20));
    tick.set_missed_tick_behavior(MissedTickBehavior::Skip);

    let mut seq_val = seq.load(Ordering::Relaxed);
    let mut ts_val = ts.load(Ordering::Relaxed);

    println!("[CONSUMER] Ready to send audio");

    while let Some(mut frame) = rx.recv().await {
        tick.tick().await;

        while let Ok(cmd) = cmd_rx.try_recv() {
            let mut state = filter_state.write().await;
            match cmd {
                AudioCommand::ToggleBassBoost(on) => {
                    state.bass_boost = on;
                    println!("[FILTER] BassBoost = {}", on);
                }
                AudioCommand::ToggleNightcore(on) => {
                    state.nightcore = on;
                    println!("[FILTER] Nightcore = {}", on);
                }
                AudioCommand::ToggleVaporwave(on) => {
                    state.vaporwave = on;
                    println!("[FILTER] Vaporwave = {}", on);
                }
                AudioCommand::SetVolume(vol) => {
                    state.volume = vol;
                    println!("[FILTER] Volume = {:.2}", vol);
                }
                _ => {}
            }
        }

        if filter_state.read().await.bass_boost {
            let mut fx = filters.lock().await;
            fx.apply(&mut frame, 2);
        }

        /*let state = filter_state.read().await;
        for s in frame.iter_mut() {
            *s = (*s as f32 * state.volume).clamp(i16::MIN as f32, i16::MAX as f32) as i16;
        }*/

        send_voice_packet(&conn, &frame, &mut encoder, seq_val, ts_val).await?;
        seq_val = seq_val.wrapping_add(1);
        ts_val = ts_val.wrapping_add(960);
    }

    seq.store(seq_val, Ordering::Relaxed);
    ts.store(ts_val, Ordering::Relaxed);
    println!("[CONSUMER] Finished");

    Ok((cmd_rx))
}
