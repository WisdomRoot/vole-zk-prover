use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use lazy_static::lazy_static;
use rand::{thread_rng, Rng};
use regex::Regex;
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{BufReader, Write},
    path::{Path, PathBuf},
    process::Command,
    sync::Mutex,
    time::Instant,
};
use volonym::circom::generator::generate_circom;
use volonym::circom::r1cs::R1CSFile;

lazy_static! {
    static ref LOG_FILE: Mutex<Option<File>> = Mutex::new(None);
}

macro_rules! log_println {
    ($($arg:tt)*) => {
        if let Ok(mut guard) = LOG_FILE.lock() {
            if let Some(file) = &mut *guard {
                if let Err(e) = writeln!(file, $($arg)*) {
                    eprintln!("Failed to write to log file: {}", e);
                }
            } else {
                println!($($arg)*);
            }
        }
    };
}

macro_rules! log_eprintln {
    ($($arg:tt)*) => {
        if let Ok(mut guard) = LOG_FILE.lock() {
            if let Some(file) = &mut *guard {
                if let Err(e) = writeln!(file, $($arg)*) {
                    eprintln!("Failed to write to log file: {}", e);
                }
            } else {
                eprintln!($($arg)*);
            }
        }
    };
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
    /// Log output to a file instead of stdout/stderr.
    /// The log file will have the same name as the input file, with a .log extension.
    #[arg(short = 'l', long, global = true)]
    log: bool,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Parse and display R1CS file contents
    Parse {
        /// Path to the .r1cs file to parse.
        #[arg(default_value = "src/circom/examples/falcon.r1cs")]
        r1cs_file: PathBuf,
    },
    /// Compile a Circom file and parse the output
    Compile {
        /// Path to the .circom file to compile.
        #[arg(default_value = "src/circom/examples/falcon.circom")]
        circom_file: PathBuf,

        #[clap(flatten)]
        optimization: Optimization,
    },
    /// Generate a Circom file from a template, compile it, and parse the output
    Generate {
        /// Path to the template file to generate the Circom file from.
        #[arg(default_value = "src/circom/examples/falcon.hbs")]
        template_file: PathBuf,

        /// The size of pk.
        #[arg(long, default_value_t = 512)]
        n: usize,

        #[clap(flatten)]
        optimization: Optimization,
    },
    /// Generate the Falcon R1CS circuit and run tests with the given test cases.
    ///
    /// If no case is specified, all cases from the input file will be run.
    Falcon {
        /// Path to the template file to generate the Circom file from.
        #[arg(default_value = "src/circom/examples/falcon.hbs")]
        template_file: PathBuf,
        /// Path to the .toml file to parse.
        #[arg(long, default_value = "src/bin/falcon.toml")]
        input: PathBuf,
        /// The case index to use from the .toml file.
        #[arg(long)]
        case: Option<usize>,
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

#[derive(Debug, Deserialize)]
struct FalconCases {
    cases: Vec<FalconCase>,
}

#[derive(Debug, Deserialize)]
struct FalconCase {
    #[serde(rename = "N")]
    n: usize,
    #[serde(rename = "Q")]
    q: i64,
    pk: String,
    s1: String,
    s2: String,
    h: String,
    c: String,
}

fn parse_poly(poly: &str) -> Vec<(u32, i64)> {
    let mut terms = Vec::new();

    if !poly.is_empty() {
        // Regex to match terms in a polynomial
        let re = Regex::new(
            r"(?ix)
              (?P<sign>[+-]?)            # optional sign
              \s*                        # optional space
              (?P<coef>\d+)?             # optional coefficient (default 1)
              \s*                        # optional space
              (?P<var>x)?                # optional variable 'x'
              \s*                        # optional space
              (?:\^ (?P<exp>\d+))?       # optional exponent
              \s*                        # optional space
            ",
        )
        .unwrap();

        for cap in re.captures_iter(poly) {
            let coef: i64 = {
                let sign = cap
                    .name("sign")
                    .map_or(1, |m| if m.as_str() == "-" { -1 } else { 1 });
                let base: i64 = cap
                    .name("coef")
                    .map_or(1, |m| m.as_str().parse().unwrap_or(1));
                sign * base
            };

            let exp: u32 = cap.name("var").map_or(0, |_| {
                cap.name("exp")
                    .map_or(1, |m| m.as_str().parse().unwrap_or(1))
            });

            terms.push((exp, coef))
        }

        terms.sort_by_key(|&(exp, _)| exp);
    }
    terms
}

fn to_vec<T>(poly: &Vec<(u32, i64)>, n: usize) -> Vec<T>
where
    T: From<i64> + Default + Clone,
{
    let mut vec = vec![T::default(); n];
    for &(exp, coef) in poly {
        if (exp as usize) < n {
            vec[exp as usize] = T::from(coef);
        }
    }
    vec
}

fn to_string_vec(poly: &Vec<(u32, i64)>, n: usize) -> Vec<String> {
    to_vec(poly, n)
        .into_iter()
        .map(|x: i64| x.to_string())
        .collect()
}

fn to_i64_vec(poly: &Vec<(u32, i64)>, n: usize) -> Vec<i64> {
    to_vec(poly, n)
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    if cli.log {
        if let Commands::Falcon { .. } = &cli.command {
            // Falcon handles logging per case
        } else {
            let input_path: &Path = match &cli.command {
                Commands::Parse { r1cs_file } => r1cs_file,
                Commands::Compile { circom_file, .. } => circom_file,
                Commands::Generate { template_file, .. } => template_file,
                _ => unreachable!(),
            };
            let log_path = input_path.with_extension("log");
            let file = File::create(log_path)?;
            *LOG_FILE.lock().unwrap() = Some(file);
        }
    }

    match &cli.command {
        Commands::Parse { r1cs_file } => parse(r1cs_file),
        Commands::Compile {
            circom_file,
            optimization,
        } => {
            let r1cs_file_path = compile(circom_file, optimization.level())?;
            parse(&r1cs_file_path)
        }
        Commands::Generate {
            template_file,
            n,
            optimization,
        } => {
            let mut rng = thread_rng();
            let pk: Vec<i64> = (0..*n).map(|_| rng.gen()).collect();
            let circom_file_path = generate(template_file, None, 12289, pk)?;
            let r1cs_file_path = compile(&circom_file_path, optimization.level())?;
            parse(&r1cs_file_path)
        }
        Commands::Falcon {
            template_file,
            input,
            case,
            optimization,
        } => {
            let toml_str = fs::read_to_string(input)?;
            let falcon_cases: FalconCases = toml::from_str(&toml_str)?;

            if let Some(case_index) = case {
                let case = &falcon_cases.cases[*case_index];
                run_falcon_case(
                    template_file,
                    case,
                    *case_index,
                    optimization.level(),
                    cli.log,
                )?;
            } else {
                for (i, case) in falcon_cases.cases.iter().enumerate() {
                    run_falcon_case(template_file, case, i, optimization.level(), cli.log)?;
                }
            }

            Ok(())
        }
    }
}

fn run_falcon_case(
    template_file: &Path,
    case: &FalconCase,
    case_index: usize,
    optimization_level: OptimizationLevel,
    log: bool,
) -> Result<()> {
    let file_stem = template_file.file_stem().unwrap().to_str().unwrap();
    let dir = template_file.parent().unwrap();
    let circom_file_name = format!("{}_{}", file_stem, case_index);
    let circom_file_path = dir.join(format!("{}.circom", circom_file_name));

    if log {
        let output_dir = dir.join(&circom_file_name);
        fs::create_dir_all(&output_dir)?;
        let log_path = output_dir.join(format!("{}.log", circom_file_name));
        let file = File::create(log_path)?;
        *LOG_FILE.lock().unwrap() = Some(file);
    }

    log_println!("=== Running Falcon Case {} ===\n", case_index);
    let pk = to_i64_vec(&parse_poly(&case.pk), case.n);
    let s1 = to_string_vec(&parse_poly(&case.s1), case.n);
    let s2 = to_string_vec(&parse_poly(&case.s2), case.n);
    let h = to_string_vec(&parse_poly(&case.h), case.n);
    let c = to_string_vec(&parse_poly(&case.c), case.n);

    let circom_file_path = generate(template_file, Some(circom_file_path), case.q, pk)?;

    let r1cs_file_path = compile(&circom_file_path, optimization_level)?;
    let artifact_dir = r1cs_file_path.parent().unwrap();

    let input_json_path = artifact_dir.join(format!("input_{}.json", case_index));

    log_println!("=== Generating input.json ===\n");
    let mut output_map = BTreeMap::new();
    output_map.insert("s1", s1);
    output_map.insert("s2", s2);
    output_map.insert("c", c);
    output_map.insert("h", h);

    let json_str = serde_json::to_string_pretty(&output_map)?;
    let mut file = File::create(&input_json_path)?;
    file.write_all(json_str.as_bytes())?;

    log_println!("Successfully wrote to {}\n", input_json_path.display());

    parse(&r1cs_file_path)?;

    generate_witness(&artifact_dir, file_stem, case_index, &input_json_path)?;

    Ok(())
}

fn parse(r1cs_file_path: &Path) -> Result<()> {
    log_println!("=== Parsing R1CS File ===\n ");
    let file = File::open(r1cs_file_path).context(format!(
        "Could not open R1CS file: {}",
        r1cs_file_path.display()
    ))?;
    let reader = BufReader::new(file);
    let r1cs_file = R1CSFile::from_reader(reader).context("Failed to parse R1CS file")?;
    log_println!("{}", r1cs_file);
    Ok(())
}

fn compile(circom_file_path: &Path, optimization_level: OptimizationLevel) -> Result<PathBuf> {
    let circom_file_stem = circom_file_path.file_stem().unwrap().to_str().unwrap();
    let parent_dir = circom_file_path.parent().unwrap();
    let output_dir = parent_dir.join(circom_file_stem);

    fs::create_dir_all(&output_dir)?;

    log_println!("=== Compiling Circom File ===\n");
    log_println!(
        "Compiling {} with optimization {}... Outputting to {}",
        circom_file_path.display(),
        optimization_level,
        output_dir.display()
    );

    let start_time = Instant::now();
    let output = Command::new("circom")
        .arg(circom_file_path)
        .arg("--r1cs")
        .arg("--wasm")
        .arg(format!("--{}", optimization_level))
        .arg("-o")
        .arg(&output_dir)
        .output()
        .context("Failed to execute circom command. Is circom installed and in your PATH?")?;
    let elapsed_time = start_time.elapsed();

    if !output.status.success() {
        log_eprintln!("Error during circom compilation:");
        log_eprintln!("{}", String::from_utf8_lossy(&output.stderr));
        anyhow::bail!("Circom compilation failed");
    }
    log_println!(
        "Compilation successful in {:.2?}s.\n",
        elapsed_time.as_secs()
    );

    let r1cs_file_name = circom_file_path
        .file_stem()
        .context("Could not get file stem")?;
    let r1cs_file_path = output_dir.join(format!("{}.r1cs", r1cs_file_name.to_str().unwrap()));

    Ok(r1cs_file_path)
}

fn generate(
    template_file_path: &Path,
    output_path: Option<PathBuf>,
    q: i64,
    pk: Vec<i64>,
) -> Result<PathBuf> {
    log_println!("=== Generating Circom File from Template ===\n");
    let circom_file_path = if let Some(output_path) = output_path {
        output_path
    } else {
        template_file_path.with_extension("circom")
    };
    generate_circom(&circom_file_path, template_file_path, q, pk)?;
    log_println!("Generated Circom file: {}\n", circom_file_path.display());
    Ok(circom_file_path)
}

fn generate_witness(
    artifact_dir: &Path,
    file_stem: &str,
    case_index: usize,
    input_json_path: &Path,
) -> Result<()> {
    let dir = artifact_dir.parent().unwrap();
    // Run the witness generation command
    let generate_witness_js_path = artifact_dir.strip_prefix(dir).unwrap().join(format!(
        "{}_{}_js/generate_witness.js",
        file_stem, case_index
    ));
    let test_wasm_path = artifact_dir.strip_prefix(dir).unwrap().join(format!(
        "{}_{}_js/{}_{}.wasm",
        file_stem, case_index, file_stem, case_index
    ));
    let witness_wtns_path = artifact_dir
        .strip_prefix(dir)
        .unwrap()
        .join(format!("witness_{}.wtns", case_index));
    let input_json_rel_path = input_json_path.strip_prefix(dir).unwrap();

    log_println!("=== Generating Witness ===\n");
    let start_time = Instant::now();
    let output = Command::new("node")
        .current_dir(dir)
        .arg(&generate_witness_js_path)
        .arg(&test_wasm_path)
        .arg(input_json_rel_path)
        .arg(&witness_wtns_path)
        .output()
        .context(
            "Failed to execute node command for witness generation. Is Node.js installed and in your PATH?",
        )?;
    let elapsed_time = start_time.elapsed();

    if !output.status.success() {
        log_eprintln!("Error during witness generation:");
        log_eprintln!("{}", String::from_utf8_lossy(&output.stderr));
        anyhow::bail!("Witness generation failed");
    }
    log_println!(
        "Witness generation successful in {:.2?}s.\n",
        elapsed_time.as_secs()
    );

    Ok(())
}