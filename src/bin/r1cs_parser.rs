use anyhow::{Context, Result};
use clap::{Parser, ValueEnum};
use std::{
    fs::File,
    io::BufReader,
    path::{Path, PathBuf},
    process::Command,
};
use volonym::circom::r1cs::R1CSFile;

/// Simple program to parse and display R1CS file contents
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to the .circom file to compile and parse.
    #[arg(default_value = "src/circom/examples/test.circom")]
    circom_file: String,

    /// Optimization level for circom compiler.
    #[arg(short, long, value_enum, default_value_t = OptimizationLevel::O1)]
    optimization_level: OptimizationLevel,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug)]
enum OptimizationLevel {
    O0,
    O1,
    O2,
}

impl std::fmt::Display for OptimizationLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.to_possible_value()
            .expect("no values are skipped")
            .get_name()
            .to_uppercase()
            .fmt(f)
    }
}

fn main() -> Result<()> {
    let args = Args::parse();

    let circom_file_path = PathBuf::from(&args.circom_file);
    let output_dir = circom_file_path.parent().unwrap_or_else(|| Path::new("."));
    let r1cs_file_name = circom_file_path
        .file_stem()
        .context("Could not get file stem")?
        .to_str()
        .context("Could not convert file stem to string")?;
    let r1cs_file_path = output_dir.join(format!("{}.r1cs", r1cs_file_name));

    println!("=== Compiling Circom File ===\n");
    println!(
        "Compiling {} with optimization {}...",
        circom_file_path.display(),
        args.optimization_level
    );

    let output = Command::new("circom")
        .arg(&circom_file_path)
        .arg("--r1cs")
        .arg(format!("--{}", args.optimization_level))
        .arg("-o")
        .arg(output_dir)
        .output()
        .context("Failed to execute circom command. Is circom installed and in your PATH?")?;

    if !output.status.success() {
        eprintln!("Error during circom compilation:");
        eprintln!("{}", String::from_utf8_lossy(&output.stderr));
        anyhow::bail!("Circom compilation failed");
    }
    println!("Compilation successful.\n");

    let file = File::open(&r1cs_file_path).context(format!(
        "Could not open R1CS file: {}",
        r1cs_file_path.display()
    ))?;
    let reader = BufReader::new(file);
    let r1cs_file = R1CSFile::from_reader(reader).context("Failed to parse R1CS file")?;

    println!("{}", r1cs_file);

    Ok(())
}
