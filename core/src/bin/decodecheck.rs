use std::time::Instant;

use flacplayer::decoder::TrackReader;
use flacplayer::metadata;

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(path) = args.next() else {
        eprintln!("uso: decodecheck <archivo> [segundos]");
        std::process::exit(2);
    };
    let limit_s: f64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(f64::MAX);

    println!("metadata: {}", metadata::read_meta(&path));

    let mut reader = match TrackReader::open(&path) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error abriendo: {e}");
            std::process::exit(1);
        }
    };

    println!(
        "codec={} rate={} canales={} bits={} duración={} ms",
        reader.codec, reader.rate, reader.channels, reader.bits, reader.duration_ms
    );

    let start = Instant::now();
    let mut buf: Vec<f32> = Vec::with_capacity(8192);
    let mut frames = 0u64;
    let mut peak = 0.0f32;
    let mut packets = 0u64;

    loop {
        match reader.next_frames(&mut buf) {
            Ok(true) => break,
            Ok(false) => {
                packets += 1;
                let ch = reader.channels.max(1);
                frames += (buf.len() / ch) as u64;
                for s in &buf {
                    let a = s.abs();
                    if a > peak {
                        peak = a;
                    }
                }
                let seconds = frames as f64 / reader.rate as f64;
                if seconds > limit_s {
                    break;
                }
            }
            Err(e) => {
                eprintln!("error de decodificación: {e}");
                std::process::exit(1);
            }
        }
    }

    let elapsed = start.elapsed().as_secs_f64();
    let seconds = frames as f64 / reader.rate as f64;
    println!(
        "decodificados {frames} frames ({seconds:.3} s) en {packets} paquetes, pico {peak:.3}"
    );
    println!(
        "{:.2}x tiempo real ({elapsed:.2} s)",
        seconds / elapsed.max(0.001)
    );
}
