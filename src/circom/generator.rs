use anyhow::Result;
use handlebars::Handlebars;
use serde_json::json;
use std::fs::File;
use std::io::Write;
use std::path::Path;

pub fn generate_template(path: &Path, n: usize, function_name: &str) -> Result<()> {
    let mut handlebars = Handlebars::new();
    let template_str = r#"template {{function_name}}({{#each indices as |i|}}{{#if @first}}pk_{{i}}{{else}},pk_{{i}}{{/if}}{{/each}}) {
    signal input s1[{{n}}];
    signal input s2[{{n}}];
    signal input h[{{n}}];
    signal output c[{{n}}];

    var d[{{n}}];

    for (var i = 0; i < {{n}}; i++) {
      d[i] = s1[i] - h[i];
      {{#each indices as |j|}}
      d[i] += s2[(n+i-{{j}})%n] * pk_{{j}};
      {{/each}}

      c[i] <== d[i];
    }
    // Template body goes here
}
"#;

    handlebars.register_template_string("template", template_str)?;

    let indices: Vec<usize> = (0..n).collect();

    let data = json!({
        "n": n,
        "function_name": function_name,
        "indices": indices,
    });

    let output = handlebars.render("template", &data)?;

    let mut file = File::create(path)?;
    file.write_all(output.as_bytes())?;

    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_generate_template() {
        let path = Path::new("src/circom/examples/gen.circom");
        let n = 3;
        let function_name = "MyCircuit";
        generate_template(path, n, function_name).unwrap();

        // fs::remove_file(path).unwrap(); // Keep for inspection
    }
}

