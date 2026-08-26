use std::collections::HashMap;
use std::net::TcpListener;

use anyhow::{anyhow, Result};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

const RANGE_START: u16 = 4100;
const RANGE_END: u16 = 4999;

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct PortRegistry {
    #[serde(default)]
    assignments: HashMap<String, u16>,
}

impl PortRegistry {
    pub fn assignment(&self, key: &str) -> Option<u16> {
        self.assignments.get(key).copied()
    }

    pub fn assignments(&self) -> &HashMap<String, u16> {
        &self.assignments
    }

    pub fn release(&mut self, key: &str) -> Option<u16> {
        self.assignments.remove(key)
    }

    pub fn allocate(&mut self, key: &str) -> Result<u16> {
        if let Some(port) = self.assignments.get(key) {
            return Ok(*port);
        }

        let taken: Vec<u16> = self.assignments.values().copied().collect();
        for port in RANGE_START..=RANGE_END {
            if taken.contains(&port) || !is_free(port) {
                continue;
            }

            self.assignments.insert(key.to_owned(), port);
            return Ok(port);
        }

        Err(anyhow!("no free port in range {RANGE_START}-{RANGE_END}"))
    }
}

fn is_free(port: u16) -> bool {
    TcpListener::bind(("127.0.0.1", port)).is_ok()
}

pub struct SharedPorts {
    inner: Mutex<PortRegistry>,
}

impl SharedPorts {
    pub fn new(registry: PortRegistry) -> Self {
        Self {
            inner: Mutex::new(registry),
        }
    }

    pub fn allocate(&self, key: &str) -> Result<u16> {
        self.inner.lock().allocate(key)
    }

    pub fn release(&self, key: &str) -> Option<u16> {
        self.inner.lock().release(key)
    }

    pub fn snapshot(&self) -> PortRegistry {
        self.inner.lock().clone()
    }
}
