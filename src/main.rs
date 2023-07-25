use sha2::{Digest, Sha512};
use std::thread;
use std::time::Instant;

const N: usize = 1_000_000_000;

fn main() {
    let start = Instant::now();

    let (generator_to_sha512_tx, generator_to_sha512_rx) = flume::bounded(1_000_000);
    let (generator_to_blake3_tx, generator_to_blake3_rx) = flume::bounded(1_000_000);
    let (sha512_to_result_tx, sha512_to_result_rx) = flume::bounded(1_000_000);
    let (blake3_to_result_tx, blake3_to_result_rx) = flume::bounded(1_000_000);

    // Generator
    thread::spawn(move || {
        for i in 0..N {
            let preimage = (i as u64).to_le_bytes();
            generator_to_sha512_tx.send(preimage.clone()).unwrap();
            generator_to_blake3_tx.send(preimage).unwrap();
        }
    });

    // Sha512
    thread::spawn(move || {
        while let Ok(preimage) = generator_to_sha512_rx.recv() {
            let hash = Sha512::digest(&preimage);
            sha512_to_result_tx.send(hash).ok();
        }
    });

    // Blake3
    thread::spawn(move || {
        while let Ok(preimage) = generator_to_blake3_rx.recv() {
            let hash = blake3::hash(&preimage);
            blake3_to_result_tx.send(hash).ok();
        }
    });

    // Result
    let result_thread = thread::spawn(move || {
        for _ in 0..N {
            sha512_to_result_rx.recv().unwrap();
            blake3_to_result_rx.recv().unwrap();
        }
    });

    result_thread.join().unwrap();

    println!("{:?}", start.elapsed());
}
