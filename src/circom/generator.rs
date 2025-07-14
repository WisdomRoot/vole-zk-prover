use anyhow::Result;
use handlebars::Handlebars;
use serde_json::json;
use std::fs::File;
use std::io::Write;
use std::path::Path;

pub fn generate_circom(output_path: &Path, template_path: &Path, pk: Vec<u8>) -> Result<()> {
    let mut handlebars = Handlebars::new();
    handlebars.register_template_file("template", template_path)?;

    let data = json!({
        "pk": pk,
    });

    let output = handlebars.render("template", &data)?;

    let mut file = File::create(output_path)?;
    file.write_all(output.as_bytes())?;

    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;
    use rand::RngCore;

    #[test]
    fn test_generate_template() {
        let output_path = Path::new("src/circom/examples/test.circom");
        let template_path = Path::new("src/circom/examples/test.hbs");
        let mut rng = rand::thread_rng();
        let mut pk_vec = vec![0u8; 3];
        rng.fill_bytes(&mut pk_vec);
        generate_from_template(output_path, template_path, pk_vec).unwrap();
    }
}

