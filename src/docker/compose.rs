use serde::Deserialize;
use std::collections::HashMap;

// ======================================================
// ROOT STRUCT
// ======================================================

#[derive(Debug, Deserialize)]
pub struct ComposeFile {
    pub services: HashMap<String, Service>,
}

// ======================================================
// SERVICE
// ======================================================

#[derive(Debug, Deserialize)]
pub struct Service {
    pub image: Option<String>,

    #[serde(default, deserialize_with = "deserialize_environment")]
    pub environment: Option<Vec<String>>,

    // Reserved for future volume simulation support
    #[allow(dead_code)]
    pub volumes: Option<Vec<String>>,

    pub depends_on: Option<Vec<String>>,
    pub command: Option<Vec<String>>,

    // ✅ NEW: Healthcheck support
    pub healthcheck: Option<HealthCheck>,
}

// ======================================================
// HEALTHCHECK STRUCT
// ======================================================

#[derive(Debug, Deserialize, Clone)]
pub struct HealthCheck {
    pub test: Option<Vec<String>>,
    pub interval: Option<String>,
    pub timeout: Option<String>,
    pub retries: Option<u64>,
}

// ======================================================
// ENVIRONMENT DESERIALIZER
// ======================================================

fn deserialize_environment<'de, D>(
    deserializer: D,
) -> Result<Option<Vec<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::{MapAccess, SeqAccess, Visitor};
    use std::fmt;

    struct EnvVisitor;

    impl<'de> Visitor<'de> for EnvVisitor {
        type Value = Option<Vec<String>>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("map or sequence for environment")
        }

        fn visit_none<E>(self) -> Result<Self::Value, E> {
            Ok(None)
        }

        fn visit_unit<E>(self) -> Result<Self::Value, E> {
            Ok(None)
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: SeqAccess<'de>,
        {
            let mut env = Vec::new();
            while let Some(value) = seq.next_element::<String>()? {
                env.push(value);
            }
            Ok(Some(env))
        }

        fn visit_map<M>(self, mut map: M) -> Result<Self::Value, M::Error>
        where
            M: MapAccess<'de>,
        {
            let mut env = Vec::new();
            while let Some((key, value)) =
                map.next_entry::<String, String>()?
            {
                env.push(format!("{}={}", key, value));
            }
            Ok(Some(env))
        }
    }

    deserializer.deserialize_any(EnvVisitor)
}
