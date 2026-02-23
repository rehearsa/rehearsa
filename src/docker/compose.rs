use std::collections::HashMap;

// ======================================================
// PUBLIC STRUCTS
// ======================================================

#[derive(Debug)]
pub struct ComposeFile {
    pub services: HashMap<String, Service>,
}

#[derive(Debug)]
pub struct Service {
    pub image: Option<String>,
    pub environment: Option<Vec<String>>,
    #[allow(dead_code)]
    pub volumes: Option<Vec<String>>,
    pub depends_on: Option<Vec<String>>,
    pub command: Option<Vec<String>>,
    pub healthcheck: Option<HealthCheck>,
    pub ports: Option<Vec<String>>,
    pub entrypoint: Option<Vec<String>>,
}

#[derive(Debug, Clone)]
pub struct HealthCheck {
    pub test: Option<Vec<String>>,
    pub interval: Option<String>,
    pub timeout: Option<String>,
    pub retries: Option<u64>,
}

// ======================================================
// ENTRY POINT
// ======================================================

pub fn parse_compose(content: &str) -> Result<ComposeFile, String> {
    let root: serde_yaml::Value =
        serde_yaml::from_str(content).map_err(|e| format!("YAML parse error: {}", e))?;

    let services_raw = match root.get("services") {
        Some(serde_yaml::Value::Mapping(m)) => m,
        _ => return Err("No services block found in Compose file".to_string()),
    };

    let mut services = HashMap::new();

    for (key, value) in services_raw {
        let name = match key.as_str() {
            Some(s) => s.to_string(),
            None => continue,
        };

        let svc_map = match value {
            serde_yaml::Value::Mapping(m) => m,
            _ => continue,
        };

        let service = Service {
            image: extract_string(svc_map, "image"),
            environment: extract_environment(svc_map),
            volumes: extract_string_list(svc_map, "volumes"),
            depends_on: extract_depends_on(svc_map),
            command: extract_string_or_list(svc_map, "command"),
            entrypoint: extract_string_or_list(svc_map, "entrypoint"),
            healthcheck: extract_healthcheck(svc_map),
            ports: extract_ports(svc_map),
        };

        services.insert(name, service);
    }

    Ok(ComposeFile { services })
}

// ======================================================
// FIELD EXTRACTORS
// ======================================================

fn extract_string(map: &serde_yaml::Mapping, key: &str) -> Option<String> {
    match map.get(key) {
        Some(serde_yaml::Value::String(s)) => Some(s.clone()),
        _ => None,
    }
}

fn extract_string_or_list(map: &serde_yaml::Mapping, key: &str) -> Option<Vec<String>> {
    match map.get(key) {
        None | Some(serde_yaml::Value::Null) => None,
        Some(serde_yaml::Value::String(s)) => Some(vec![s.clone()]),
        Some(serde_yaml::Value::Sequence(seq)) => {
            let out: Vec<String> = seq.iter().filter_map(value_to_string).collect();
            if out.is_empty() { None } else { Some(out) }
        }
        _ => None,
    }
}

fn extract_string_list(map: &serde_yaml::Mapping, key: &str) -> Option<Vec<String>> {
    match map.get(key) {
        Some(serde_yaml::Value::Sequence(seq)) => {
            let out: Vec<String> = seq
                .iter()
                .filter_map(|v| match v {
                    serde_yaml::Value::String(s) => Some(s.clone()),
                    serde_yaml::Value::Mapping(m) => {
                        let source = m.get("source").and_then(|v| v.as_str());
                        let target = m.get("target").and_then(|v| v.as_str());
                        match (source, target) {
                            (Some(s), Some(t)) => Some(format!("{}:{}", s, t)),
                            (None, Some(t)) => Some(t.to_string()),
                            _ => None,
                        }
                    }
                    _ => None,
                })
                .collect();
            if out.is_empty() { None } else { Some(out) }
        }
        _ => None,
    }
}

fn extract_environment(map: &serde_yaml::Mapping) -> Option<Vec<String>> {
    match map.get("environment") {
        None | Some(serde_yaml::Value::Null) => None,

        Some(serde_yaml::Value::Sequence(seq)) => {
            let out: Vec<String> = seq.iter().filter_map(value_to_string).collect();
            if out.is_empty() { None } else { Some(out) }
        }

        Some(serde_yaml::Value::Mapping(m)) => {
            let mut out = Vec::new();
            for (k, v) in m {
                if k.as_str() == Some("<<") {
                    continue;
                }
                let key = match k.as_str() {
                    Some(s) => s,
                    None => continue,
                };
                match v {
                    serde_yaml::Value::Null => {
                        out.push(key.to_string());
                    }
                    other => {
                        if let Some(s) = value_to_string(other) {
                            out.push(format!("{}={}", key, s));
                        }
                    }
                }
            }
            if out.is_empty() { None } else { Some(out) }
        }

        _ => None,
    }
}

fn extract_depends_on(map: &serde_yaml::Mapping) -> Option<Vec<String>> {
    match map.get("depends_on") {
        None | Some(serde_yaml::Value::Null) => None,

        Some(serde_yaml::Value::Sequence(seq)) => {
            let out: Vec<String> = seq
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect();
            if out.is_empty() { None } else { Some(out) }
        }

        Some(serde_yaml::Value::Mapping(m)) => {
            let out: Vec<String> = m
                .iter()
                .filter_map(|(k, _)| k.as_str().map(|s| s.to_string()))
                .collect();
            if out.is_empty() { None } else { Some(out) }
        }

        _ => None,
    }
}

fn extract_healthcheck(map: &serde_yaml::Mapping) -> Option<HealthCheck> {
    let hc = match map.get("healthcheck") {
        Some(serde_yaml::Value::Mapping(m)) => m,
        _ => return None,
    };

    if let Some(serde_yaml::Value::Bool(true)) = hc.get("disable") {
        return None;
    }

    let test = match hc.get("test") {
        Some(serde_yaml::Value::Sequence(seq)) => {
            let parts: Vec<String> = seq
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect();
            if parts.is_empty() { None } else { Some(parts) }
        }
        Some(serde_yaml::Value::String(s)) => Some(vec![s.clone()]),
        _ => None,
    };

    let interval = hc.get("interval").and_then(|v| v.as_str()).map(|s| s.to_string());
    let timeout  = hc.get("timeout").and_then(|v| v.as_str()).map(|s| s.to_string());
    let retries  = hc.get("retries").and_then(|v| v.as_u64());

    Some(HealthCheck { test, interval, timeout, retries })
}

fn extract_ports(map: &serde_yaml::Mapping) -> Option<Vec<String>> {
    match map.get("ports") {
        Some(serde_yaml::Value::Sequence(seq)) => {
            let out: Vec<String> = seq
                .iter()
                .filter_map(|v| match v {
                    serde_yaml::Value::String(s) => Some(s.clone()),
                    serde_yaml::Value::Number(n) => Some(n.to_string()),
                    serde_yaml::Value::Mapping(m) => {
                        let published = m.get("published").and_then(value_to_string);
                        let target    = m.get("target").and_then(value_to_string);
                        match (published, target) {
                            (Some(p), Some(t)) => Some(format!("{}:{}", p, t)),
                            (None, Some(t))    => Some(t),
                            _                  => None,
                        }
                    }
                    _ => None,
                })
                .collect();
            if out.is_empty() { None } else { Some(out) }
        }
        _ => None,
    }
}

// ======================================================
// HELPERS
// ======================================================

fn value_to_string(v: &serde_yaml::Value) -> Option<String> {
    match v {
        serde_yaml::Value::String(s) => Some(s.clone()),
        serde_yaml::Value::Number(n) => Some(n.to_string()),
        serde_yaml::Value::Bool(b)   => Some(b.to_string()),
        _ => None,
    }
}
