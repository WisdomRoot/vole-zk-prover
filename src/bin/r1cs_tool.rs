use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use rand::{thread_rng, Rng};
use regex::Regex;
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{BufReader, Write},
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
        /// Path to the directory containing input and output files.
        #[arg(default_value = "src/circom/examples")]
        dir: PathBuf,
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

    match &cli.command {
        Commands::Parse { r1cs_file } => parse(r1cs_file),
        Commands::Compile {
            circom_file,
            optimization,
        } => {
            let r1cs_file_path = compile(circom_file, None, optimization.level())?;
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
            let r1cs_file_path = compile(&circom_file_path, None, optimization.level())?;
            parse(&r1cs_file_path)
        }
        Commands::Falcon {
            dir,
            input,
            case,
            optimization,
        } => {
            let toml_str = fs::read_to_string(input)?;
            let falcon_cases: FalconCases = toml::from_str(&toml_str)?;

            if let Some(case_index) = case {
                let case = &falcon_cases.cases[*case_index];
                run_falcon_case(dir, case, *case_index, optimization.level())?;
            } else {
                for (i, case) in falcon_cases.cases.iter().enumerate() {
                    run_falcon_case(dir, case, i, optimization.level())?;
                }
            }

            Ok(())
        }
    }
}

fn run_falcon_case(
    dir: &Path,
    case: &FalconCase,
    case_index: usize,
    optimization_level: OptimizationLevel,
) -> Result<()> {
    println!("=== Running Falcon Case {} ===\n", case_index);
    let pk = to_i64_vec(&parse_poly(&case.pk), case.n);
    let s1 = to_string_vec(&parse_poly(&case.s1), case.n);
    let s2 = to_string_vec(&parse_poly(&case.s2), case.n);
    let h = to_string_vec(&parse_poly(&case.h), case.n);
    let c = to_string_vec(&parse_poly(&case.c), case.n);

    let file_stem = "falcon";
    let artifact_dir_name = format!("{}_{}", file_stem, case_index);
    let artifact_dir = dir.join(artifact_dir_name);
    fs::create_dir_all(&artifact_dir)?;

    let input_json_path = artifact_dir.join(format!("input_{}.json", case_index));

    println!("=== Generating input.json ===\n");
    let mut output_map = BTreeMap::new();
    output_map.insert("s1", s1);
    output_map.insert("s2", s2);
    output_map.insert("c", c);
    output_map.insert("h", h);

    let json_str = serde_json::to_string_pretty(&output_map)?;
    let mut file = File::create(&input_json_path)?;
    file.write_all(json_str.as_bytes())?;

    println!("Successfully wrote to {}\n", input_json_path.display());

    // Pass pk_raw to generate function
    let template_file_path = dir.join(format!("{file_stem}.hbs"));
    let circom_file_path = generate(
        &template_file_path,
        Some(dir.join(format!("{}_{}.circom", file_stem, case_index))),
        case.q,
        pk,
    )?;
    let r1cs_file_path = compile(&circom_file_path, Some(&artifact_dir), optimization_level)?;
    parse(&r1cs_file_path)?;

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

    println!("=== Generating Witness ===\n");
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
        eprintln!("Error during witness generation:");
        eprintln!("{}", String::from_utf8_lossy(&output.stderr));
        anyhow::bail!("Witness generation failed");
    }
    println!(
        "Witness generation successful in {:.2?}s.\n",
        elapsed_time.as_secs()
    );

    Ok(())
}

fn parse(r1cs_file_path: &Path) -> Result<()> {
    println!("=== Parsing R1CS File ===\n ");
    let file = File::open(r1cs_file_path).context(format!(
        "Could not open R1CS file: {}",
        r1cs_file_path.display()
    ))?;
    let reader = BufReader::new(file);
    let r1cs_file = R1CSFile::from_reader(reader).context("Failed to parse R1CS file")?;
    println!("{}", r1cs_file);
    Ok(())
}

fn compile(
    circom_file_path: &Path,
    output_dir_opt: Option<&Path>,
    optimization_level: OptimizationLevel,
) -> Result<PathBuf> {
    let output_dir = match output_dir_opt {
        Some(dir) => dir.to_path_buf(),
        None => {
            let circom_file_stem = circom_file_path.file_stem().unwrap().to_str().unwrap();
            let parent_dir = circom_file_path.parent().unwrap();
            parent_dir.join(circom_file_stem)
        }
    };

    fs::create_dir_all(&output_dir)?;

    println!("=== Compiling Circom File ===\n");
    println!(
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

fn generate(
    template_file_path: &Path,
    output_path: Option<PathBuf>,
    q: i64,
    pk: Vec<i64>,
) -> Result<PathBuf> {
    println!("=== Generating Circom File from Template ===\n");
    let circom_file_path = if let Some(output_path) = output_path {
        output_path
    } else {
        template_file_path.with_extension("circom")
    };
    generate_circom(&circom_file_path, template_file_path, q, pk)?;
    println!("Generated Circom file: {}\n", circom_file_path.display());
    Ok(circom_file_path)
}