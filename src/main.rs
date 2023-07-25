use sha2::{Digest, Sha512};
use std::thread;
use std::time::{Duration, Instant};

const N: usize = 1_000_000_000;

fn main() {
    let start = Instant::now();

    let (mut generator_to_sha512_tx, mut generator_to_sha512_rx) = rtrb::RingBuffer::new(1_000_000);
    let (mut generator_to_blake3_tx, mut generator_to_blake3_rx) = rtrb::RingBuffer::new(1_000_000);
    let (mut sha512_to_result_tx, mut sha512_to_result_rx) = rtrb::RingBuffer::new(1_000_000);
    let (mut blake3_to_result_tx, mut blake3_to_result_rx) = rtrb::RingBuffer::new(1_000_000);

    // Generator
    thread::spawn(move || {
        for i in 0..N {
            let preimage = (i as u64).to_le_bytes();
            push(&mut generator_to_sha512_tx, preimage.clone());
            push(&mut generator_to_blake3_tx, preimage);
        }
    });

    // Sha512
    thread::spawn(move || loop {
        let preimage = pop(&mut generator_to_sha512_rx);
        let hash = Sha512::digest(&preimage);
        push(&mut sha512_to_result_tx, hash);
    });

    // Blake3
    thread::spawn(move || loop {
        let preimage = pop(&mut generator_to_blake3_rx);
        let hash = blake3::hash(&preimage);
        push(&mut blake3_to_result_tx, hash);
    });

    // Result
    let result_thread = thread::spawn(move || {
        for _ in 0..N {
            pop(&mut sha512_to_result_rx);
            pop(&mut blake3_to_result_rx);
        }
    });

    result_thread.join().unwrap();

    println!("{:?}", start.elapsed());
}

fn push<T>(tx: &mut rtrb::Producer<T>, mut value: T) {
    loop {
        match tx.push(value) {
            Ok(_) => break,
            Err(rtrb::PushError::Full(v)) => value = v,
        }
        thread::sleep(Duration::from_millis(1));
    }
}

fn pop<T>(rx: &mut rtrb::Consumer<T>) -> T {
    loop {
        if let Ok(value) = rx.pop() {
            return value;
        }
        thread::sleep(Duration::from_millis(1));
    }
}
