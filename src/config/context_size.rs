use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use std::fmt;

#[derive(Clone, PartialEq, Default)]
pub struct ContextSize(i32);

impl ContextSize {
    pub fn new(size: i32) -> Self {
        ContextSize(size)
    }

    pub fn size(&self) -> i32 {
        self.0
    }
}

impl<'de> Deserialize<'de> for ContextSize {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(ContextSizeVisitor)
    }
}

struct ContextSizeVisitor;

impl<'de> de::Visitor<'de> for ContextSizeVisitor {
    type Value = ContextSize;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("an integer or a string ending with 'k'")
    }

    fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        if v > i32::MAX as i64 {
            return Err(E::custom("integer out of range for i32"));
        }
        Ok(ContextSize::new(v as i32))
    }

    fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        if v > i32::MAX as u64 {
            return Err(E::custom("integer out of range for i32"));
        }
        Ok(ContextSize::new(v as i32))
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        if v.is_empty() {
            return Err(E::custom("context string cannot be empty"));
        }

        let last_char = v.chars().last().unwrap();
        if last_char == 'k' || last_char == 'K' {
            let num_part = &v[..v.len() - 1];
            num_part
                .parse::<i32>()
                .map(|n| ContextSize::new(n * 1024))
                .map_err(|_| E::custom("invalid number in context string with 'k'"))
        } else {
            // Try to parse the whole string as an integer
            v.parse::<i32>()
                .map(ContextSize::new)
                .map_err(|_| E::custom("invalid context string"))
        }
    }
}

impl Serialize for ContextSize {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if self.0 % 1024 == 0 {
            let kilobytes = self.0 / 1024;
            let mut buffer = String::new();
            buffer.push_str(&kilobytes.to_string());
            buffer.push('k');
            serializer.serialize_str(&buffer)
        } else {
            serializer.serialize_i32(self.0)
        }
    }
}
