//! Parse inert HTML only; never evaluate plugin scripts or fetch config.xml.
use super::{parameter_definitions, parameter_kind};
use dom_query::Document;
use serde_json::{json, Value};
use std::collections::HashMap;

pub(super) fn is_checkbox(definition: &Value) -> bool {
    definition["_class"] == "com.cwctravel.hudson.plugins.extended_choice_parameter.ExtendedChoiceParameterDefinition"
        && definition["type"] == "PT_CHECKBOX"
}

pub(super) fn enrich(job: &mut Value, html: &str) -> Result<(), String> {
    let document = Document::from(html);
    let mut fields = HashMap::new();
    // Parameter containers must belong to one POST form, not a login/search form.
    let forms = document.select("form[method]");
    let mut candidates = forms.iter().filter(|form| {
        form.attr("method").is_some_and(|v| v.eq_ignore_ascii_case("post"))
            && form.select("[name=parameter]").iter().next().is_some()
    });
    let form = candidates.next().ok_or("JENKINS_PARAMETER: Build parameter form is missing")?;
    if candidates.next().is_some() {
        return Err("JENKINS_PARAMETER: Ambiguous build form".into());
    }
    let containers = form.select("[name=parameter]");
    for container in containers.iter() {
        let names = container.select("input[name=name][type=hidden]");
        if names.iter().count() != 1 {
            return Err("JENKINS_PARAMETER: Missing or ambiguous parameter name".into());
        }
        let name = names.attr("value").ok_or("JENKINS_PARAMETER: Missing parameter name value")?.to_string();
        if fields.insert(name, container).is_some() {
            return Err("JENKINS_PARAMETER: Duplicate parameter in build form".into());
        }
    }
    let definitions = parameter_definitions(job);
    if fields.len() != definitions.len() {
        return Err("JENKINS_PARAMETER: Build form differs from parameter definitions".into());
    }
    let mut normalized = HashMap::new();
    for mut definition in definitions {
        let name = definition["name"].as_str().ok_or("JENKINS_PARAMETER: Missing name")?.to_owned();
        let field = fields.get(&name).ok_or_else(|| format!("JENKINS_PARAMETER: Missing form field {name}"))?;
        let invalid = || format!("JENKINS_PARAMETER: Unsupported or malformed form field {name}");
        let default = if is_checkbox(&definition) {
            let inputs = field.select("input[type=checkbox]");
            let mut choices = Vec::new();
            let mut selected = Vec::new();
            let input_name = format!("{name}.value");
            for input in inputs.iter() {
                if input.attr("disabled").is_some() || input.attr("name").as_deref() != Some(input_name.as_str()) {
                    return Err(invalid());
                }
                let value = input.attr("value").ok_or_else(&invalid)?.to_string();
                if choices.contains(&value) {
                    return Err(invalid());
                }
                if input.attr("checked").is_some() {
                    selected.push(value.clone());
                }
                choices.push(value);
            }
            if choices.is_empty() {
                return Err(invalid());
            }
            definition["choices"] = json!(choices);
            json!(selected)
        } else {
            match parameter_kind(&definition) {
                "ChoiceParameterDefinition" => {
                    let select = field.select("select[name=value]");
                    if select.iter().count() != 1 || select.attr("multiple").is_some() || select.attr("disabled").is_some() {
                        return Err(invalid());
                    }
                    let mut choices = Vec::new();
                    let mut selected = Vec::new();
                    for option in select.select("option").iter() {
                        if option.attr("disabled").is_some() { return Err(invalid()); }
                        let value = option.attr("value").map(|v| v.to_string()).unwrap_or_else(|| option.text().to_string());
                        if option.attr("selected").is_some() { selected.push(value.clone()); }
                        choices.push(value);
                    }
                    if choices.is_empty() || selected.len() > 1 || definition["choices"] != json!(choices) {
                        return Err(invalid());
                    }
                    json!(selected.first().unwrap_or(&choices[0]))
                }
                "StringParameterDefinition" | "TextParameterDefinition" | "BooleanParameterDefinition" => {
                    let kind = parameter_kind(&definition);
                    let selector = match kind {
                        "TextParameterDefinition" => "textarea[name=value]",
                        "BooleanParameterDefinition" => "input[name=value][type=checkbox]",
                        _ => "input[name=value][type=text]",
                    };
                    let input = field.select(selector);
                    if input.iter().count() != 1 || input.attr("disabled").is_some() { return Err(invalid()); }
                    match kind {
                        "BooleanParameterDefinition" => json!(input.attr("checked").is_some()),
                        "TextParameterDefinition" => json!(input.text().to_string()),
                        _ => json!(input.attr("value").map(|v| v.to_string()).unwrap_or_default()),
                    }
                }
                _ => return Err(format!("JENKINS_PARAMETER: Unsupported parameter {name} ({})", parameter_kind(&definition))),
            }
        };
        definition["defaultParameterValue"] = json!({"value": default});
        normalized.insert(name, definition);
    }
    for property in job["property"].as_array_mut().ok_or("Invalid parameter properties")? {
        if let Some(definitions) = property["parameterDefinitions"].as_array_mut() {
            for definition in definitions {
                let name = definition["name"].as_str().ok_or("Invalid parameter name")?;
                *definition = normalized.remove(name).ok_or("Duplicate parameter definition")?;
            }
        }
    }
    Ok(())
}
