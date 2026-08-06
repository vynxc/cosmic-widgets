use bitflags::bitflags;
use serde::{Deserialize, Serialize};

/// A host operation a widget package may request.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Capability {
    SystemMetrics,
    MediaControl,
    Network { hosts: Vec<String> },
    InstanceStorage,
    ClipboardRead,
    ClipboardWrite,
    OpenUri,
    SpawnProcess,
}

bitflags! {
    /// Fast runtime representation of granted capabilities.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    pub struct CapabilitySet: u32 {
        const SYSTEM_METRICS = 1 << 0;
        const MEDIA_CONTROL = 1 << 1;
        const NETWORK = 1 << 2;
        const INSTANCE_STORAGE = 1 << 3;
        const CLIPBOARD_READ = 1 << 4;
        const CLIPBOARD_WRITE = 1 << 5;
        const OPEN_URI = 1 << 6;
        const SPAWN_PROCESS = 1 << 7;
    }
}

impl Capability {
    /// Converts a manifest capability to its runtime bit.
    pub const fn bit(&self) -> CapabilitySet {
        match self {
            Self::SystemMetrics => CapabilitySet::SYSTEM_METRICS,
            Self::MediaControl => CapabilitySet::MEDIA_CONTROL,
            Self::Network { .. } => CapabilitySet::NETWORK,
            Self::InstanceStorage => CapabilitySet::INSTANCE_STORAGE,
            Self::ClipboardRead => CapabilitySet::CLIPBOARD_READ,
            Self::ClipboardWrite => CapabilitySet::CLIPBOARD_WRITE,
            Self::OpenUri => CapabilitySet::OPEN_URI,
            Self::SpawnProcess => CapabilitySet::SPAWN_PROCESS,
        }
    }

    /// Returns whether the capability requires an explicit warning during installation.
    pub const fn is_dangerous(&self) -> bool {
        matches!(self, Self::SpawnProcess | Self::ClipboardRead)
    }
}

/// User-owned permission state, stored outside a package archive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityGrant {
    pub package_id: String,
    pub granted: CapabilitySet,
    #[serde(default)]
    pub network_hosts: Vec<String>,
}

impl CapabilityGrant {
    /// Creates an empty deny-by-default grant.
    pub fn denied(package_id: impl Into<String>) -> Self {
        Self {
            package_id: package_id.into(),
            granted: CapabilitySet::empty(),
            network_hosts: Vec::new(),
        }
    }

    /// Checks both the capability bit and, for networking, the declared host allowlist.
    pub fn allows(&self, capability: &Capability) -> bool {
        if !self.granted.contains(capability.bit()) {
            return false;
        }
        match capability {
            Capability::Network { hosts } => hosts
                .iter()
                .all(|host| self.network_hosts.iter().any(|allowed| allowed == host)),
            _ => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn denied_grant_should_reject_capability() {
        let grant = CapabilityGrant::denied("dev.example.Clock");
        assert!(!grant.allows(&Capability::SystemMetrics));
    }

    #[test]
    fn network_grant_should_require_every_declared_host() {
        let grant = CapabilityGrant {
            package_id: "dev.example.Weather".into(),
            granted: CapabilitySet::NETWORK,
            network_hosts: vec!["api.example.com".into()],
        };
        let requested = Capability::Network {
            hosts: vec!["api.example.com".into(), "cdn.example.com".into()],
        };
        assert!(!grant.allows(&requested));
    }
}
