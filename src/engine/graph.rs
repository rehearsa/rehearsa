use std::collections::{HashMap, HashSet};

pub fn topological_sort(
    services: &HashMap<String, Vec<String>>,
) -> Result<Vec<String>, String> {
    let mut visited = HashSet::new();
    let mut temp = HashSet::new();
    let mut result = Vec::new();

    for node in services.keys() {
        visit(node, services, &mut visited, &mut temp, &mut result)?;
    }

    Ok(result)
}

fn visit(
    node: &str,
    services: &HashMap<String, Vec<String>>,
    visited: &mut HashSet<String>,
    temp: &mut HashSet<String>,
    result: &mut Vec<String>,
) -> Result<(), String> {
    if visited.contains(node) {
        return Ok(());
    }

    if temp.contains(node) {
        return Err(format!("Circular dependency detected at {}", node));
    }

    temp.insert(node.to_string());

    if let Some(deps) = services.get(node) {
        for dep in deps {
            visit(dep, services, visited, temp, result)?;
        }
    }

    temp.remove(node);
    visited.insert(node.to_string());
    result.push(node.to_string());

    Ok(())
}
