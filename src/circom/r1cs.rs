//! Borrowed extensively from Nova Scotia https://github.com/nalinbhardwaj/Nova-Scotia/

use anyhow::{bail, Error};
use byteorder::{LittleEndian, ReadBytesExt};
use itertools::Itertools;
use num_bigint::{BigInt, Sign};
use num_traits::One as _;
use std::{
    collections::HashMap,
    fmt,
    io::{Read, Seek, SeekFrom},
};

use crate::{
    zkp::{R1CSWithMetadata, SparseR1CS, R1CS},
    Fr, SparseFMatrix, SparseVec,
};
use num_bigint::BigUint;

use super::read_constraint_vec;

// R1CSFile's header
#[derive(Debug)]
pub struct Header {
    pub field_size: u32,
    pub prime_size: BigUint,
    pub n_wires: u32,
    pub n_pub_out: u32,
    pub n_pub_in: u32,
    pub n_prv_in: u32,
    pub n_labels: u64,
    pub n_constraints: u32,
}

#[derive(Debug)]
pub struct Constraints {
    a_rows: SparseFMatrix<Fr>,
    b_rows: SparseFMatrix<Fr>,
    c_rows: SparseFMatrix<Fr>,
}

#[derive(Debug)]
pub struct R1CSFile {
    pub version: u32,
    pub header: Header,
    pub constraints: Constraints,
    pub wire_mapping: Vec<u64>,
}

impl R1CSFile {
    /// Converts this to the R1CS format used by the rest of this crate
    pub fn to_crate_format(self) -> R1CSWithMetadata<Fr> {
        let r1cs_ = SparseR1CS {
            a_rows: self.constraints.a_rows,
            b_rows: self.constraints.b_rows,
            c_rows: self.constraints.c_rows,
        };
        let pub_in_start = 1 + self.header.n_pub_out as usize;
        let public_outputs_indices = (1..pub_in_start).collect_vec();
        let public_inputs_indices =
            (pub_in_start..pub_in_start + self.header.n_pub_in as usize).collect_vec();
        let unpadded_wtns_len = self.header.n_wires as usize; // overflow is possible but not practical given circuits of feasible size
        let r1cs = R1CS::Sparse(r1cs_);
        R1CSWithMetadata {
            r1cs,
            public_inputs_indices,
            public_outputs_indices,
            unpadded_wtns_len,
        }
    }

    /// Parses bytes in a circom .r1cs binary format
    pub fn from_reader<R: Read + Seek>(mut reader: R) -> Result<Self, Error> {
        let mut magic = [0u8; 4];
        reader.read_exact(&mut magic)?;
        if magic != "r1cs".as_bytes() {
            bail!("Invalid magic number");
        }

        let version = reader.read_u32::<LittleEndian>()?;
        if version != 1 {
            bail!("Unsupported version")
        }

        let num_sections = reader.read_u32::<LittleEndian>()?;

        // section type -> file offset
        let mut section_offsets = HashMap::<u32, u64>::new();
        let mut section_sizes = HashMap::<u32, u64>::new();

        // get file offset of each section
        for _ in 0..num_sections {
            let section_type = reader.read_u32::<LittleEndian>()?;
            let section_size = reader.read_u64::<LittleEndian>()?;
            let offset = reader.seek(SeekFrom::Current(0))?;
            section_offsets.insert(section_type, offset);
            section_sizes.insert(section_type, section_size);
            reader.seek(SeekFrom::Current(section_size as i64))?;
        }

        let header_type = 1;
        let constraint_type = 2;
        let wire2label_type = 3;

        reader.seek(SeekFrom::Start(*section_offsets.get(&header_type).unwrap()))?;
        let header = read_header(&mut reader, *section_sizes.get(&header_type).unwrap())?;
        if header.field_size != 32 {
            bail!("This parser only supports 32-byte fields");
        }

        if header.prime_size != Fr::prime() {
            bail!("This parser only supports bn254");
        }

        reader.seek(SeekFrom::Start(
            *section_offsets.get(&constraint_type).unwrap(),
        ))?;

        let constraints = read_constraints(
            &mut reader,
            *section_sizes.get(&constraint_type).unwrap(),
            &header,
        );

        reader.seek(SeekFrom::Start(
            *section_offsets.get(&wire2label_type).unwrap(),
        ))?;
        let wire_mapping = read_map(
            &mut reader,
            *section_sizes.get(&wire2label_type).unwrap(),
            &header,
        )?;

        Ok(R1CSFile {
            version,
            header,
            constraints,
            wire_mapping,
        })
    }
}

fn read_header<R: Read>(mut reader: R, size: u64) -> Result<Header, Error> {
    let field_size = reader.read_u32::<LittleEndian>()?;
    let mut prime_size_bytes = vec![0u8; field_size as usize];
    reader.read_exact(&mut prime_size_bytes)?;
    let prime_size = BigUint::from_bytes_le(&prime_size_bytes);

    if size != 32 + field_size as u64 {
        bail!("Invalid header section size");
    }

    Ok(Header {
        field_size,
        prime_size,
        n_wires: reader.read_u32::<LittleEndian>()?,
        n_pub_out: reader.read_u32::<LittleEndian>()?,
        n_pub_in: reader.read_u32::<LittleEndian>()?,
        n_prv_in: reader.read_u32::<LittleEndian>()?,
        n_labels: reader.read_u64::<LittleEndian>()?,
        n_constraints: reader.read_u32::<LittleEndian>()?,
    })
}

fn read_constraints<R: Read>(mut reader: R, _size: u64, header: &Header) -> Constraints {
    let mut a_rows = Vec::with_capacity(header.n_constraints as usize);
    let mut b_rows = Vec::with_capacity(header.n_constraints as usize);
    let mut c_rows = Vec::with_capacity(header.n_constraints as usize);

    for _ in 0..header.n_constraints {
        a_rows.push(read_constraint_vec(&mut reader));
        b_rows.push(read_constraint_vec(&mut reader));
        c_rows.push(read_constraint_vec(&mut reader));
    }
    let a_rows = SparseFMatrix(a_rows);
    let b_rows = SparseFMatrix(b_rows);
    let c_rows = SparseFMatrix(c_rows);

    Constraints {
        a_rows,
        b_rows,
        c_rows,
    }
}

fn read_map<R: Read>(mut reader: R, size: u64, header: &Header) -> Result<Vec<u64>, Error> {
    if size != header.n_wires as u64 * 8 {
        bail!("Invalid map section size");
    }
    let mut vec = Vec::with_capacity(header.n_wires as usize);
    for _ in 0..header.n_wires {
        vec.push(reader.read_u64::<LittleEndian>()?);
    }
    if vec[0] != 0 {
        bail!("Wire 0 should always be mapped to 0");
    }
    Ok(vec)
}

fn factor_leading_sign(coeffs: &SparseVec<Fr>) -> (i32, String) {
    if coeffs.0.is_empty() {
        return (0, "0".to_string());
    }

    let first_coeff = coeffs.0[0].1.norm();
    let sign = if first_coeff.sign() == Sign::Minus {
        -1
    } else {
        1
    };

    let mut terms = Vec::new();
    for (i, (var_idx, coeff_val)) in coeffs.0.iter().enumerate() {
        let norm_coeff: BigInt = coeff_val.norm() * sign;

        let one = BigInt::one();
        let term = match (i, *var_idx, norm_coeff) {
            (0, 0, c) if c == one => "1".to_string(),
            (0, 0, c) => format!("{c}"),
            (0, idx, c) if c == one => format!("x{idx}"),
            (0, idx, c) => format!("{c}*x{idx}"),
            (_, 0, c) if c.sign() == Sign::Plus => format!("+ {c}"),
            (_, 0, c) => format!("- {}", -c),
            (_, idx, c) if c == one => format!("+ x{idx}"),
            (_, idx, c) if c == BigInt::from(-1) => format!("- x{idx}"),
            (_, idx, c) if c.sign() == Sign::Plus => format!("+ {c}*x{idx}"),
            (_, idx, c) => format!("- {}*x{idx}", -c),
        };
        terms.push(term);
    }
    (sign, terms.join(" "))
}

impl fmt::Display for R1CSFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "=== R1CS Binary Format Parser ===\n")?;
        writeln!(f, "Version: {}", self.version)?;
        writeln!(f, "Number of sections: {}", 3)?;
        writeln!(f, "\n=== Header Details ===\n")?;
        writeln!(f, "  Field size: {} bytes", self.header.field_size)?;
        writeln!(f, "  Prime (field modulus): {}", self.header.prime_size)?;
        writeln!(f, "  Number of wires: {}", self.header.n_wires)?;
        writeln!(f, "  Number of public outputs: {}", self.header.n_pub_out)?;
        writeln!(f, "  Number of public inputs: {}", self.header.n_pub_in)?;
        writeln!(f, "  Number of private inputs: {}", self.header.n_prv_in)?;
        writeln!(f, "  Number of labels: {}", self.header.n_labels)?;
        writeln!(f, "  Number of constraints: {}", self.header.n_constraints)?;
        writeln!(f, "\n=== Constraints Section ===\n")?;
        write!(f, "{}", self.constraints)
    }
}

impl fmt::Display for Constraints {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for i in 0..self.a_rows.0.len() {
            let (a_sign, a_str) = factor_leading_sign(&self.a_rows.0[i]);
            let (b_sign, b_str) = factor_leading_sign(&self.b_rows.0[i]);
            let (c_sign, c_str) = factor_leading_sign(&self.c_rows.0[i]);

            let total_sign = a_sign * b_sign * c_sign;

            let a_print = if a_str.contains(' ') {
                format!("({})", a_str)
            } else {
                a_str.clone()
            };
            let b_print = if b_str.contains(' ') {
                format!("({})", b_str)
            } else {
                b_str.clone()
            };

            if a_str == "0" || b_str == "0" {
                writeln!(f, "  Constraint {i}: {c_str} = 0")?;
            } else {
                let c_str = match total_sign {
                    -1 if c_str.contains(' ') => format!("-({})", c_str),
                    -1 if c_str != "0" => format!("-{}", c_str),
                    _ => c_str,
                };

                writeln!(
                    f,
                    "  Constraint {}: {} * {} = {}",
                    i, a_print, b_print, c_str
                )?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use std::{fs::File, io::BufReader};

    use super::*;
    #[test]
    fn read_r1cs_file() {
        let file = File::open("src/circom/examples/test.r1cs").unwrap();
        let buf_reader = BufReader::new(file);
        let r1cs = R1CSFile::from_reader(buf_reader).unwrap();
    }

    #[test]
    fn correct_public_indices() {
        let file = File::open("src/circom/examples/test.r1cs").unwrap();
        let buf_reader = BufReader::new(file);
        let r1cs = R1CSFile::from_reader(buf_reader).unwrap();
        let r1cs = r1cs.to_crate_format();
        assert!(r1cs.public_outputs_indices == (1..258).collect_vec());
        assert!(r1cs.public_inputs_indices == (258..260).collect_vec());
    }
}
