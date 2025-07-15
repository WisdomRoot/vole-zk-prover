use anyhow::Result;
use handlebars::Handlebars;
use serde_json::json;
use std::fs::File;
use std::io::Write;
use std::path::Path;

pub fn generate_circom(output_path: &Path, template_path: &Path, pk: Vec<i64>) -> Result<()> {
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
        let pk = vec![1, 2, 3];
        generate_circom(output_path, template_path, pk).unwrap();
    }
}
