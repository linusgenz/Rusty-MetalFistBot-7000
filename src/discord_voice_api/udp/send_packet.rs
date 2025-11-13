use std::sync::atomic::Ordering;
use crate::discord_voice_api::voice::VoiceConnection;
use opus::Encoder;

pub async fn send_voice_packet(
    conn: &VoiceConnection,
    pcm_samples: &[i16],
    encoder: &mut Encoder,
    seq: u16,
    timestamp: u32,
) -> anyhow::Result<()> {
    // Opus encode
    let mut opus_buf = vec![0u8; 1000]; // 1000
    let n = encoder.encode(pcm_samples, &mut opus_buf)?;
    let opus_payload = &opus_buf[..n];

    // RTP header
    let mut rtp_header = [0u8; 12];
    rtp_header[0] = 0x80;
    rtp_header[1] = 0x78;
    rtp_header[2..4].copy_from_slice(&seq.to_be_bytes());
    rtp_header[4..8].copy_from_slice(&timestamp.to_be_bytes());
    rtp_header[8..12].copy_from_slice(&conn.ssrc.to_be_bytes());

    let counter_val = conn.counter.fetch_add(1, Ordering::Relaxed);

    // Encrypt
    let encrypted_payload_and_extra =
        conn.cipher
            .encrypt_packet(&rtp_header, opus_payload, counter_val)?;

    // Packet = RTP header + ciphertext(+counter)
    let mut packet = Vec::with_capacity(12 + encrypted_payload_and_extra.len());
    packet.extend_from_slice(&rtp_header);
    packet.extend_from_slice(&encrypted_payload_and_extra);

    // UDP send
    conn.socket.send(&packet).await?;

    Ok(())
}
