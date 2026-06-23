use std::collections::BTreeMap;

pub(super) struct EnvOverrideGuard {
    previous: Vec<(&'static str, Option<String>)>,
}

impl EnvOverrideGuard {
    pub(super) fn apply(overrides: &BTreeMap<&'static str, String>) -> Self {
        let previous = overrides
            .iter()
            .map(|(key, value)| {
                let previous = std::env::var(key).ok();
                std::env::set_var(key, value);
                (*key, previous)
            })
            .collect();
        Self { previous }
    }
}

impl Drop for EnvOverrideGuard {
    fn drop(&mut self) {
        for (key, value) in self.previous.drain(..).rev() {
            match value {
                Some(value) => std::env::set_var(key, value),
                None => std::env::remove_var(key),
            }
        }
    }
}
