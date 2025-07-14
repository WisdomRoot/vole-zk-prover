use lazy_static::lazy_static;
use std::{fs::File, io::BufReader, time::Instant};
use volonym::{
    actors::actors::Prover,
    circom::{r1cs::R1CSFile, witness::wtns_from_reader},
    zkp::R1CSWithMetadata,
    FVec, Fr,
};

lazy_static! {
    pub static ref WITNESS: FVec<Fr> = {
        let wtns_file = File::open("src/circom/examples/witness.wtns").unwrap();
        let wtns_reader = BufReader::new(wtns_file);
        wtns_from_reader(wtns_reader).unwrap()
    };
    pub static ref CIRCUIT: R1CSWithMetadata<Fr> = {
        let r1cs_file = File::open("src/circom/examples/test.r1cs").unwrap();
        let r1cs_reader = BufReader::new(r1cs_file);
        R1CSFile::from_reader(r1cs_reader)
            .unwrap()
            .to_crate_format()
    };
}

fn load_and_prove() {
    let mut prover = Prover::from_witness_and_circuit_unpadded(WITNESS.clone(), CIRCUIT.clone());
    let _vole_comm = prover.mkvole().unwrap();
    let _proof = prover.prove().unwrap();
}

use std::time::Duration;

fn main() {
    // Full warm-up run.
    load_and_prove();

    let mut durations = Vec::with_capacity(10);
    for _ in 0..10 {
        let start = Instant::now();
        load_and_prove();
        durations.push(start.elapsed());
    }

    let total_duration: Duration = durations.iter().sum();
    let mean_duration = total_duration / durations.len() as u32;

    let std_dev = {
        let mean_micros = mean_duration.as_micros() as f64;
        let variance = durations
            .iter()
            .map(|d| {
                let micros = d.as_micros() as f64;
                (micros - mean_micros).powi(2)
            })
            .sum::<f64>()
            / durations.len() as f64;
        (variance.sqrt() as u64, "µs")
    };

    let min_duration = durations.iter().min().unwrap();
    let max_duration = durations.iter().max().unwrap();

    println!("Benchmark results (10 runs):");
    println!("  Mean: {:?}", mean_duration);
    println!("  Std Dev: {} {}", std_dev.0, std_dev.1);
    println!("  Min:  {:?}", min_duration);
    println!("  Max:  {:?}", max_duration);
}
