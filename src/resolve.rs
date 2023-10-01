use std::{collections::HashMap, io::Read};
use serde_json::Value;
use std::error::Error;


use crate::errors::PipeError;

/// Represents a resolve config. It internally stores Keys as HostAndPort and values as HostAndPort as well.
#[derive(Debug, Clone)]
pub struct ResolveConfig {
    rules: HashMap<String, String>
}


/// Default ResolveConfig. The mapping is empty.
impl Default for ResolveConfig {
    fn default() -> ResolveConfig {
        ResolveConfig {
            rules: HashMap::new()
        }
    }
}

impl ResolveConfig {
    fn load_value_from_json(path:&str)-> Result<Value, Box<dyn Error>> {
        let mut f = std::fs::File::open(path)?;
        let mut str = String::new();
        f.read_to_string(&mut str)?;
        let resolve_config_raw = serde_json::from_str(&str)?;
        return Ok(resolve_config_raw);
    }
    
    /// Resolve a lookup String -> String
    /// It could be empty though
    pub fn resolve(&self, original:&str) -> String {
        if self.rules.len() == 0 {
            return original.to_string();
        }

        let original_lower = original.to_ascii_lowercase();
        let result = self.rules.get(&original_lower);
        if result.is_none() {
            return original.to_string();
        } else {
            return result.unwrap().to_string();
        }
    }
    
    fn value_to_string(value:Value) -> Result<String, Box<dyn Error>> {
        match value {
            Value::String(what) => {
                return Ok(what);
            },
            _ => {
                return Err(PipeError::wrap_box(
                    format!("Unexpected JSON type `{value}`. Expect String")));
            }
        }
    }

    /// Parse ResolveConfig from a Json. The JSON must be a String -> String structure. If key or value is not string, the parse fails.
    pub fn load_from_json_file(path:&str) -> Result<ResolveConfig, Box<dyn Error>> {
        let raw = Self::load_value_from_json(path)?;
        match raw {
            Value::Object(map) => {
                let mut rules = HashMap::<String, String>::new();
                for (k, vraw) in map {
                    let v = Self::value_to_string(vraw)?;
                    rules.insert(k.to_ascii_lowercase(), v);
                }
                return Ok(ResolveConfig{rules});
            },
            _ => {
                return Err(PipeError::wrap_box(format!("Unexpected JSON type `{raw}`. Expect Map")));
            }
        }
    }
}