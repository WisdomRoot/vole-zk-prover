use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use rand::thread_rng;
use rand::RngCore;
use std::{
    fs::File,
    io::BufReader,
    path::{Path, PathBuf},
    process::Command,
    time::Instant,
};
use volonym::circom::generator::generate_circom;
use volonym::circom::r1cs::R1CSFile;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Parse and display R1CS file contents
    Parse {
        /// Path to the .r1cs file to parse.
        #[arg(default_value = "src/circom/examples/test.r1cs")]
        r1cs_file: String,
    },
    /// Compile a Circom file and parse the output
    Compile {
        /// Path to the .circom file to compile.
        #[arg(default_value = "src/circom/examples/test.circom")]
        circom_file: String,

        #[clap(flatten)]
        optimization: Optimization,
    },
    /// Generate a Circom file from a template, compile it, and parse the output
    Generate {
        /// Path to the template file to generate the Circom file from.
        #[arg(default_value = "src/circom/examples/test.hbs")]
        template_file: String,

        /// The size of pk.
        #[arg(long, default_value_t = 512)]
        n: usize,

        #[clap(flatten)]
        optimization: Optimization,
    },
}

#[derive(Parser, Debug)]
#[group(required = false, multiple = false)]
struct Optimization {
    #[arg(long, name = "O0", aliases = ["o0"])]
    o0: bool,
    #[arg(long, name = "O1", default_value = "true", aliases = ["o1"])]
    o1: bool,
    #[arg(long, name = "O2", aliases = ["o2"])]
    o2: bool,
}

impl Optimization {
    fn level(&self) -> OptimizationLevel {
        if self.o0 {
            OptimizationLevel::O0
        } else if self.o2 {
            OptimizationLevel::O2
        } else {
            OptimizationLevel::O1
        }
    }
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
    let cli = Cli::parse();

    match &cli.command {
        Commands::Parse { r1cs_file } => {
            let r1cs_file_path = PathBuf::from(r1cs_file);
            parse(&r1cs_file_path)
        }
        Commands::Compile {
            circom_file,
            optimization,
        } => {
            let circom_file_path = PathBuf::from(circom_file);
            let r1cs_file_path = compile(&circom_file_path, optimization.level())?;
            parse(&r1cs_file_path)
        }
        Commands::Generate {
            template_file,
            n,
            optimization,
        } => {
            let template_file_path = PathBuf::from(template_file);
            let circom_file_path = generate(&template_file_path, *n)?;
            let r1cs_file_path = compile(&circom_file_path, optimization.level())?;
            parse(&r1cs_file_path)
        }
    }
}

fn parse(r1cs_file_path: &Path) -> Result<()> {
    println!("=== Parsing R1CS File ===\n");
    let file = File::open(r1cs_file_path).context(format!(
        "Could not open R1CS file: {}",
        r1cs_file_path.display()
    ))?;
    let reader = BufReader::new(file);
    let r1cs_file = R1CSFile::from_reader(reader).context("Failed to parse R1CS file")?;
    println!("{}", r1cs_file);
    Ok(())
}

fn compile(circom_file_path: &Path, optimization_level: OptimizationLevel) -> Result<PathBuf> {
    let output_dir = circom_file_path.parent().unwrap_or_else(|| Path::new("."));
    println!("=== Compiling Circom File ===\n");
    println!(
        "Compiling {} with optimization {}...",
        circom_file_path.display(),
        optimization_level
    );

    let start_time = Instant::now();
    let output = Command::new("circom")
        .arg(circom_file_path)
        .arg("--r1cs")
        .arg(format!("--{}", optimization_level))
        .arg("-o")
        .arg(output_dir)
        .output()
        .context("Failed to execute circom command. Is circom installed and in your PATH?")?;
    let elapsed_time = start_time.elapsed();

    if !output.status.success() {
        eprintln!("Error during circom compilation:");
        eprintln!("{}", String::from_utf8_lossy(&output.stderr));
        anyhow::bail!("Circom compilation failed");
    }
    println!(
        "Compilation successful in {:.2?}s.\n",
        elapsed_time.as_secs()
    );

    let r1cs_file_name = circom_file_path
        .file_stem()
        .context("Could not get file stem")?
        .to_str()
        .context("Could not convert file stem to string")?;
    let r1cs_file_path = output_dir.join(format!("{}.r1cs", r1cs_file_name));

    Ok(r1cs_file_path)
}

fn generate(template_file_path: &Path, n: usize) -> Result<PathBuf> {
    println!("=== Generating Circom File from Template ===\n");
    let mut rng = thread_rng();
    let mut pk_vec = vec![0u8; n];
    let circom_file_path = template_file_path.with_extension("circom");
    rng.fill_bytes(&mut pk_vec);
    println!("pk_vec: {:?}", pk_vec);
    generate_circom(&circom_file_path, template_file_path, pk_vec)?;
    println!("Generated Circom file: {}\n", circom_file_path.display());
    Ok(circom_file_path)
}
