//! WAV (RIFF) container: a minimal writer for PCM16 mono and a real parser
//! for PCM16 mono/stereo at arbitrary sample rates.
//!
//! The parser walks RIFF chunks properly — including odd-sized chunks, which
//! carry one padding byte that must be skipped per the RIFF spec — so
//! real-world files with `LIST`/`fact`/vendor chunks between `fmt ` and
//! `data` parse correctly. Compressed formats (a-law, mu-law, ADPCM, MPEG,
//! float, extensible) are rejected with explicit errors naming the format.

use crate::Audio;
use ai_errors::{AiError, ValidationError};

/// The canonical WAV/RIFF PCM format tag.
const WAVE_FORMAT_PCM: u16 = 1;

fn invalid(context: &str, detail: impl std::fmt::Display) -> AiError {
    AiError::Validation(ValidationError::new(format!("{context}: {detail}")))
}

/// Wraps raw little-endian 16-bit mono PCM in a minimal WAV container.
pub fn wav_from_pcm(pcm: &[u8], sample_rate: u32) -> Vec<u8> {
    let mut wav = wav_header(sample_rate, 1, pcm.len());
    wav.extend_from_slice(pcm);
    wav
}

/// Wraps raw little-endian 16-bit **interleaved stereo** PCM in a WAV
/// container (2 channels).
pub fn wav_from_pcm_stereo(interleaved_pcm: &[u8], sample_rate: u32) -> Vec<u8> {
    let mut wav = wav_header(sample_rate, 2, interleaved_pcm.len());
    wav.extend_from_slice(interleaved_pcm);
    wav
}

fn wav_header(sample_rate: u32, channels: u16, data_len: usize) -> Vec<u8> {
    let byte_rate = sample_rate * u32::from(channels) * 2;
    let block_align = channels * 2;
    let mut wav = Vec::with_capacity(44 + data_len);
    wav.extend_from_slice(b"RIFF");
    // 4 ("WAVE") + 8 + 16 (fmt) + 8 + data_len; data_len is even for PCM16.
    wav.extend_from_slice(&(36 + data_len as u32).to_le_bytes());
    wav.extend_from_slice(b"WAVE");
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&WAVE_FORMAT_PCM.to_le_bytes());
    wav.extend_from_slice(&channels.to_le_bytes());
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&byte_rate.to_le_bytes());
    wav.extend_from_slice(&block_align.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&(data_len as u32).to_le_bytes());
    wav
}

/// Parses a WAV file into [`Audio`] (mono PCM16).
///
/// Accepted: RIFF/WAVE containers whose `fmt ` chunk declares uncompressed
/// PCM (`format == 1`), 16 bits per sample, mono or stereo. Stereo input is
/// folded to mono by averaging left/right pairs. Any other format tag
/// (compressed codecs, float, extensible) is rejected with an error that
/// names the offending format code.
pub fn parse_wav(bytes: &[u8]) -> Result<Audio, AiError> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(invalid(
            "wav",
            "not a RIFF/WAVE container (bad magic or truncated header)",
        ));
    }

    let mut fmt: Option<(u16, u16, u32, u16)> = None; // format, channels, rate, bits
    let mut data: Option<&[u8]> = None;

    // Walk chunks after the 12-byte RIFF header. Every chunk is padded to an
    // even offset in the file: odd-sized bodies are followed by one pad byte.
    let mut off = 12usize;
    while off + 8 <= bytes.len() {
        let id = &bytes[off..off + 4];
        let size = u32::from_le_bytes([
            bytes[off + 4],
            bytes[off + 5],
            bytes[off + 6],
            bytes[off + 7],
        ]) as usize;
        let body_start = off + 8;
        let body_end = body_start.saturating_add(size).min(bytes.len());
        match id {
            b"fmt " => {
                let body = bytes
                    .get(body_start..body_end)
                    .ok_or_else(|| invalid("wav fmt chunk", "declared size exceeds file length"))?;
                if body.len() < 16 {
                    return Err(invalid(
                        "wav fmt chunk",
                        format!("too short: {} bytes, need >= 16", body.len()),
                    ));
                }
                let format = u16::from_le_bytes([body[0], body[1]]);
                let channels = u16::from_le_bytes([body[2], body[3]]);
                let sample_rate = u32::from_le_bytes([body[4], body[5], body[6], body[7]]);
                let bits = u16::from_le_bytes([body[14], body[15]]);
                fmt = Some((format, channels, sample_rate, bits));
            }
            // First data chunk wins (streamed files may repeat it).
            b"data" if data.is_none() => {
                data = Some(&bytes[body_start..body_end]);
            }
            _ => {}
        }
        // Advance past the body plus the word-alignment pad byte.
        off = body_start + size + (size & 1);
    }

    let (format, channels, sample_rate, bits) =
        fmt.ok_or_else(|| invalid("wav", "missing fmt chunk"))?;
    let payload = data.ok_or_else(|| invalid("wav", "missing data chunk"))?;

    if format != WAVE_FORMAT_PCM {
        return Err(invalid(
            "wav",
            format!(
                "unsupported/compressed audio format tag {format} (only uncompressed PCM = 1 is supported)"
            ),
        ));
    }
    if bits != 16 {
        return Err(invalid(
            "wav",
            format!("unsupported bit depth {bits} (only 16-bit PCM is supported)"),
        ));
    }
    if sample_rate == 0 {
        return Err(invalid("wav", "sample rate must be positive"));
    }
    match channels {
        1 => Ok(Audio::from_samples(decode_pcm16(payload)?, sample_rate)),
        2 => {
            let interleaved = decode_pcm16(payload)?;
            if interleaved.len() % 2 != 0 {
                return Err(invalid("wav", "stereo data has an unpaired final sample"));
            }
            let mono: Vec<i16> = interleaved
                .chunks_exact(2)
                .map(|pair| {
                    let l = pair[0] as i32;
                    let r = pair[1] as i32;
                    ((l + r) / 2) as i16
                })
                .collect();
            Ok(Audio::from_samples(mono, sample_rate))
        }
        other => Err(invalid(
            "wav",
            format!("unsupported channel count {other} (mono or stereo only)"),
        )),
    }
}

fn decode_pcm16(payload: &[u8]) -> Result<Vec<i16>, AiError> {
    if payload.len() % 2 != 0 {
        return Err(invalid(
            "wav data chunk",
            "truncated: odd number of bytes for 16-bit samples",
        ));
    }
    Ok(payload
        .chunks_exact(2)
        .map(|p| i16::from_le_bytes([p[0], p[1]]))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_mono_preserves_samples_exactly() {
        let samples: Vec<i16> = (0..1000)
            .map(|i| {
                let v = (i * 37) % 30_000 - 15_000;
                v as i16
            })
            .collect();
        let pcm: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
        let parsed = parse_wav(&wav_from_pcm(&pcm, 44_100)).unwrap();
        assert_eq!(parsed.sample_rate, 44_100);
        assert_eq!(parsed.samples, samples);
    }

    #[test]
    fn roundtrip_stereo_folds_to_mono_average() {
        let frames: Vec<(i16, i16)> = vec![(1000, -1000), (2000, 4000), (-3000, -1000), (5, 7)];
        let mut pcm = Vec::new();
        for (l, r) in &frames {
            pcm.extend_from_slice(&l.to_le_bytes());
            pcm.extend_from_slice(&r.to_le_bytes());
        }
        let parsed = parse_wav(&wav_from_pcm_stereo(&pcm, 22_050)).unwrap();
        assert_eq!(parsed.sample_rate, 22_050);
        assert_eq!(parsed.samples, vec![0, 3000, -2000, 6]);
    }

    /// Real-world files carry metadata chunks (often odd-sized, e.g. LIST of
    /// length 17) between `fmt ` and `data`; each must be skipped including
    /// its single pad byte so the data chunk stays aligned.
    #[test]
    fn parses_odd_sized_chunks_with_padding_between_fmt_and_data() {
        let pcm: Vec<u8> = vec![1u8, 2, 3, 4, 5, 6];
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        let riff_len = 4 + (8 + 16) + (8 + 17 + 1/* odd LIST + pad */) + (8 + pcm.len());
        wav.extend_from_slice(&(riff_len as u32).to_le_bytes());
        wav.extend_from_slice(b"WAVE");
        // fmt chunk
        wav.extend_from_slice(b"fmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
        wav.extend_from_slice(&1u16.to_le_bytes()); // mono
        wav.extend_from_slice(&8_000u32.to_le_bytes());
        wav.extend_from_slice(&16_000u32.to_le_bytes()); // byte rate
        wav.extend_from_slice(&2u16.to_le_bytes()); // block align
        wav.extend_from_slice(&16u16.to_le_bytes());
        // Odd-sized INFO/LIST chunk (17 bytes body -> 1 pad byte).
        wav.extend_from_slice(b"LIST");
        wav.extend_from_slice(&17u32.to_le_bytes());
        // Exactly 17 body bytes: "INFOISFTv1" + 7 NULs.
        wav.extend_from_slice(b"INFOISFTv1");
        wav.extend_from_slice(&[0u8; 7]);
        wav.push(0x00); // pad byte required by RIFF word alignment
        // data chunk
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&(pcm.len() as u32).to_le_bytes());
        wav.extend_from_slice(&pcm);

        let parsed = parse_wav(&wav).unwrap();
        assert_eq!(parsed.sample_rate, 8_000);
        assert_eq!(parsed.samples, vec![0x0201, 0x0403, 0x0605]);
    }

    #[test]
    fn rejects_compressed_and_non_pcm_formats_with_clear_errors() {
        let base = |format_tag: u16| {
            let mut wav = Vec::new();
            wav.extend_from_slice(b"RIFF");
            wav.extend_from_slice(&36u32.to_le_bytes());
            wav.extend_from_slice(b"WAVEfmt ");
            wav.extend_from_slice(&16u32.to_le_bytes());
            wav.extend_from_slice(&format_tag.to_le_bytes());
            wav.extend_from_slice(&1u16.to_le_bytes()); // mono
            wav.extend_from_slice(&16_000u32.to_le_bytes());
            wav.extend_from_slice(&32_000u32.to_le_bytes());
            wav.extend_from_slice(&2u16.to_le_bytes());
            wav.extend_from_slice(&16u16.to_le_bytes());
            wav.extend_from_slice(b"data");
            wav.extend_from_slice(&0u32.to_le_bytes());
            wav
        };
        // mu-law (7), a-law (6), IEEE float (3), MPEG (80), adaptive PCM (2).
        for tag in [2u16, 3, 6, 7, 80] {
            let err = parse_wav(&base(tag)).unwrap_err().to_string();
            assert!(
                err.contains(&format!("{tag}")) && err.contains("PCM"),
                "tag {tag} error should name the format: {err}"
            );
        }
    }

    #[test]
    fn rejects_bad_bit_depth_channels_and_truncation() {
        let err = parse_wav(b"RIFF").unwrap_err();
        assert!(err.to_string().contains("RIFF"), "{err}");

        // 8-bit depth rejected.
        let mut wav8 = wav_from_pcm(&[0u8; 4], 16_000);
        wav8[34] = 8; // bitsPerSample offset in the canonical header
        assert!(
            parse_wav(&wav8)
                .unwrap_err()
                .to_string()
                .contains("bit depth")
        );

        // 3+ channels rejected.
        let mut wav5ch = wav_from_pcm(&[0u8; 6], 16_000);
        wav5ch[22] = 5; // numChannels offset
        assert!(
            parse_wav(&wav5ch)
                .unwrap_err()
                .to_string()
                .contains("channel")
        );

        // Odd-length data chunk (truncated sample).
        let wav_odd = wav_from_pcm(&[1u8, 2, 3], 16_000);
        let declared = u32::from_le_bytes([wav_odd[40], wav_odd[41], wav_odd[42], wav_odd[43]]);
        assert_eq!(declared, 3); // writer wrote odd size; parser must reject
        assert!(parse_wav(&wav_odd).unwrap_err().to_string().contains("odd"));
    }

    proptest::proptest! {
        /// Property: for arbitrary sample counts and rates,
        /// write -> parse reproduces the exact sample vector.
        #[test]
        fn prop_roundtrip_is_identity(len in 0usize..20_000, rate in 1u32..384_000) {
            let samples: Vec<i16> = (0..len).map(|i| (i as i16).wrapping_mul(297) ^ (i as i16).rotate_left(3)).collect();
            let expected_ms = (samples.len() as u64 * 1000) / rate as u64;
            let pcm: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
            let parsed = parse_wav(&wav_from_pcm(&pcm, rate))?;
            proptest::prop_assert_eq!(parsed.samples, samples);
            proptest::prop_assert_eq!(parsed.sample_rate, rate);
            proptest::prop_assert_eq!(parsed.duration_ms, expected_ms);
        }
    }
}
