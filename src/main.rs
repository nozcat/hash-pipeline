use sha2::{Digest, Sha512};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

const N: usize = 1_000_000_000;

const NUM_SHA512_HASHERS: usize = 8;
const NUM_BLAKE3_HASHERS: usize = 4;

fn main() {
    let start = Instant::now();

    let (mut generator_to_sha512_tx, mut generator_to_sha512_rx) =
        ring_buffers(NUM_SHA512_HASHERS, 1_000_000);
    let (mut generator_to_blake3_tx, mut generator_to_blake3_rx) =
        ring_buffers(NUM_BLAKE3_HASHERS, 1_000_000);
    let (mut sha512_to_result_tx, mut sha512_to_result_rx) =
        ring_buffers(NUM_SHA512_HASHERS, 1_000_000);
    let (mut blake3_to_result_tx, mut blake3_to_result_rx) =
        ring_buffers(NUM_BLAKE3_HASHERS, 1_000_000);

    let mut stats = vec![];

    // Generator
    let (idle, blocked) = (Arc::new(AtomicU64::new(0)), Arc::new(AtomicU64::new(0)));
    stats.push((format!("generator"), idle.clone(), blocked.clone()));
    thread::spawn(move || {
        let mut sha512_channel = 0;
        let mut blake3_channel = 0;
        for i in 0..N {
            let preimage = (i as u64).to_le_bytes();
            push(
                &mut generator_to_sha512_tx[sha512_channel],
                preimage.clone(),
                &blocked,
            );
            push(
                &mut generator_to_blake3_tx[blake3_channel],
                preimage,
                &blocked,
            );
            sha512_channel = (sha512_channel + 1) % NUM_SHA512_HASHERS;
            blake3_channel = (blake3_channel + 1) % NUM_BLAKE3_HASHERS;
        }
    });

    // Sha512
    for i in 0..NUM_SHA512_HASHERS {
        let (idle, blocked) = (Arc::new(AtomicU64::new(0)), Arc::new(AtomicU64::new(0)));
        stats.push((format!("sha512_{}", i), idle.clone(), blocked.clone()));
        let mut rx = generator_to_sha512_rx.remove(0);
        let mut tx = sha512_to_result_tx.remove(0);
        thread::spawn(move || loop {
            let preimage = pop(&mut rx, &idle);
            let hash = Sha512::digest(&preimage);
            push(&mut tx, hash, &blocked);
        });
    }

    // Blake3
    for i in 0..NUM_BLAKE3_HASHERS {
        let (idle, blocked) = (Arc::new(AtomicU64::new(0)), Arc::new(AtomicU64::new(0)));
        stats.push((format!("blake3_{}", i), idle.clone(), blocked.clone()));
        let mut rx = generator_to_blake3_rx.remove(0);
        let mut tx = blake3_to_result_tx.remove(0);
        thread::spawn(move || loop {
            let preimage = pop(&mut rx, &idle);
            let hash = blake3::hash(&preimage);
            push(&mut tx, hash, &blocked);
        });
    }

    // Result
    let (idle, blocked) = (Arc::new(AtomicU64::new(0)), Arc::new(AtomicU64::new(0)));
    stats.push((format!("result"), idle.clone(), blocked.clone()));
    let result_thread = thread::spawn(move || {
        let mut sha512_channel = 0;
        let mut blake3_channel = 0;
        for _ in 0..N {
            pop(&mut sha512_to_result_rx[sha512_channel], &idle);
            pop(&mut blake3_to_result_rx[blake3_channel], &idle);
            sha512_channel = (sha512_channel + 1) % NUM_SHA512_HASHERS;
            blake3_channel = (blake3_channel + 1) % NUM_BLAKE3_HASHERS;
        }
    });

    // Stats
    thread::spawn(move || {
        let start = Instant::now();
        loop {
            for (name, idle, blocked) in stats.iter() {
                let percent_idle = (100.0 * idle.load(Ordering::Relaxed) as f64
                    / start.elapsed().as_millis() as f64) as i32;
                let percent_blocked = (100.0 * blocked.load(Ordering::Relaxed) as f64
                    / start.elapsed().as_millis() as f64)
                    as i32;
                println!(
                    "{}: %idle={} %blocked={}",
                    name, percent_idle, percent_blocked
                );
            }
            println!("");
            thread::sleep(Duration::from_secs(1));
        }
    });

    result_thread.join().unwrap();

    println!("{:?}", start.elapsed());
}

fn ring_buffers<T>(
    count: usize,
    capacity: usize,
) -> (Vec<rtrb::Producer<T>>, Vec<rtrb::Consumer<T>>) {
    (0..count).map(|_| rtrb::RingBuffer::new(capacity)).unzip()
}

fn push<T>(tx: &mut rtrb::Producer<T>, mut value: T, blocked: &Arc<AtomicU64>) {
    loop {
        match tx.push(value) {
            Ok(_) => break,
            Err(rtrb::PushError::Full(v)) => value = v,
        }
        let start = Instant::now();
        thread::sleep(Duration::from_millis(10));
        blocked.fetch_add(start.elapsed().as_millis() as u64, Ordering::Relaxed);
    }
}

fn pop<T>(rx: &mut rtrb::Consumer<T>, idle: &Arc<AtomicU64>) -> T {
    loop {
        if let Ok(value) = rx.pop() {
            return value;
        }
        let start = Instant::now();
        thread::sleep(Duration::from_millis(10));
        idle.fetch_add(start.elapsed().as_millis() as u64, Ordering::Relaxed);
    }
}
