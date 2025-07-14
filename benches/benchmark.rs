use criterion::{black_box, criterion_group, criterion_main, Criterion};
use lazy_static::lazy_static;
use std::{fs::File, io::BufReader};
use volonym::{
    actors::actors::Prover,
    circom::{r1cs::R1CSFile, witness::wtns_from_reader},
    zkp::R1CSWithMetadata,
    FVec, Fr,
};
// use num_modular::{ModularCoreOps};

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

fn criterion_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("slow");
    group.sample_size(10);
    group.bench_function(
        "Load R1CS, Witness, and Create the VOLE in the Head Quicksilver proof",
        |b| b.iter(black_box(load_and_prove)),
    );
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);

