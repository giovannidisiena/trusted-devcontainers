use std::fmt;

use clap::ValueEnum;

pub const DEFAULT_CPUS: u16 = 6;
pub const DEFAULT_MEMORY_GB: u16 = 12;
pub const DEFAULT_DISK_GB: u16 = 120;
pub const DEFAULT_ORG: &str = "cantina-forks";

#[derive(Clone, Debug, Eq, PartialEq, ValueEnum)]
#[value(rename_all = "kebab-case")]
pub enum VmType {
    Qemu,
    Vz,
    Krunkit,
}

impl VmType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Qemu => "qemu",
            Self::Vz => "vz",
            Self::Krunkit => "krunkit",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, ValueEnum)]
#[value(rename_all = "kebab-case")]
pub enum Profile {
    Base,
    Node,
    SolidityFoundry,
    SolidityFoundryNode,
}

impl Profile {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Base => "base",
            Self::Node => "node",
            Self::SolidityFoundry => "solidity-foundry",
            Self::SolidityFoundryNode => "solidity-foundry-node",
        }
    }
}

impl fmt::Display for Profile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, ValueEnum)]
#[value(rename_all = "kebab-case")]
pub enum BuildTarget {
    All,
    Base,
    Node,
    SolidityFoundry,
    SolidityFoundryNode,
}

impl BuildTarget {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Base => "base",
            Self::Node => "node",
            Self::SolidityFoundry => "solidity-foundry",
            Self::SolidityFoundryNode => "solidity-foundry-node",
        }
    }
}

impl fmt::Display for BuildTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

pub fn vm_default(client: &str) -> String {
    format!("client-{client}")
}

pub fn lima_host(vm: &str) -> String {
    format!("lima-{vm}")
}

pub fn is_valid_slug(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_slugs() {
        assert!(is_valid_slug("exampleco"));
        assert!(is_valid_slug("example.co_1-2"));
        assert!(!is_valid_slug(""));
        assert!(!is_valid_slug("owner/repo"));
        assert!(!is_valid_slug("repo name"));
    }

    #[test]
    fn derives_vm_names() {
        assert_eq!(vm_default("acme"), "client-acme");
        assert_eq!(lima_host("client-acme"), "lima-client-acme");
    }

    #[test]
    fn renders_vm_types() {
        assert_eq!(VmType::Qemu.as_str(), "qemu");
        assert_eq!(VmType::Vz.as_str(), "vz");
        assert_eq!(VmType::Krunkit.as_str(), "krunkit");
    }
}
