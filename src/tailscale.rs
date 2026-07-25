use std::{
    collections::HashMap,
    io::ErrorKind,
    net::{IpAddr, Ipv4Addr},
    process::{Command, Output},
};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TailscalePeer {
    pub name: String,
    pub ip: Ipv4Addr,
}

#[derive(Debug, Deserialize)]
pub struct TailscaleStatus {
    #[serde(rename = "BackendState")]
    backend_state: String,
    #[serde(rename = "Self")]
    self_node: TailscaleNode,
    #[serde(rename = "Peer", default)]
    peers: HashMap<String, TailscaleNode>,
}

#[derive(Debug, Deserialize)]
struct TailscaleNode {
    #[serde(rename = "DNSName", default)]
    dns_name: String,
    #[serde(rename = "HostName", default)]
    host_name: String,
    #[serde(rename = "TailscaleIPs", default)]
    tailscale_ips: Vec<IpAddr>,
    #[serde(rename = "Online", default)]
    online: bool,
}

impl TailscaleStatus {
    pub fn load() -> Result<Self> {
        let output = run_status_command()?;
        if !output.status.success() {
            let message = String::from_utf8_lossy(&output.stderr);
            bail!(
                "`tailscale status` failed: {}",
                if message.trim().is_empty() {
                    "make sure Tailscale is running and signed in"
                } else {
                    message.trim()
                }
            );
        }
        let status: Self =
            serde_json::from_slice(&output.stdout).context("Tailscale returned invalid status")?;
        status.ensure_running()?;
        Ok(status)
    }

    pub fn local_ipv4(&self) -> Result<Ipv4Addr> {
        self.self_node
            .ipv4()
            .context("this device does not have a Tailscale IPv4 address")
    }

    pub fn online_peers(&self) -> Vec<TailscalePeer> {
        let mut peers: Vec<_> = self
            .peers
            .values()
            .filter(|node| node.online)
            .filter_map(|node| {
                Some(TailscalePeer {
                    name: node.display_name(),
                    ip: node.ipv4()?,
                })
            })
            .collect();
        peers.sort_by(|left, right| left.name.cmp(&right.name));
        peers
    }

    fn ensure_running(&self) -> Result<()> {
        if self.backend_state != "Running" {
            bail!(
                "Tailscale is not connected (state: {}); run `tailscale up` or connect in the Tailscale app",
                self.backend_state
            );
        }
        Ok(())
    }
}

fn run_status_command() -> Result<Output> {
    let candidates = [
        "tailscale",
        "/Applications/Tailscale.app/Contents/MacOS/Tailscale",
    ];
    for executable in candidates {
        match Command::new(executable)
            .env("TAILSCALE_BE_CLI", "1")
            .args(["status", "--json"])
            .output()
        {
            Ok(output) => return Ok(output),
            Err(error) if error.kind() == ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("could not run Tailscale at {executable}"));
            }
        }
    }
    bail!(
        "could not find the Tailscale CLI; install Tailscale and enable its CLI integration first"
    )
}

impl TailscaleNode {
    fn ipv4(&self) -> Option<Ipv4Addr> {
        self.tailscale_ips.iter().find_map(|ip| match ip {
            IpAddr::V4(ip) => Some(*ip),
            IpAddr::V6(_) => None,
        })
    }

    fn display_name(&self) -> String {
        if !self.host_name.is_empty() {
            self.host_name.clone()
        } else if !self.dns_name.is_empty() {
            self.dns_name.trim_end_matches('.').to_owned()
        } else {
            self.ipv4()
                .map(|ip| ip.to_string())
                .unwrap_or_else(|| "unknown device".to_owned())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const STATUS_JSON: &str = r#"{
      "BackendState": "Running",
      "Self": {
        "HostName": "desktop",
        "TailscaleIPs": ["100.64.0.1", "fd7a:115c:a1e0::1"],
        "Online": true
      },
      "Peer": {
        "node-key-1": {
          "DNSName": "macbook.example.ts.net.",
          "HostName": "macbook",
          "TailscaleIPs": ["100.64.0.2"],
          "Online": true
        },
        "node-key-2": {
          "HostName": "offline-server",
          "TailscaleIPs": ["100.64.0.3"],
          "Online": false
        }
      }
    }"#;

    #[test]
    fn reads_local_address_and_online_peers() {
        let status: TailscaleStatus = serde_json::from_str(STATUS_JSON).unwrap();
        status.ensure_running().unwrap();
        assert_eq!(status.local_ipv4().unwrap(), Ipv4Addr::new(100, 64, 0, 1));
        assert_eq!(
            status.online_peers(),
            [TailscalePeer {
                name: "macbook".to_owned(),
                ip: Ipv4Addr::new(100, 64, 0, 2),
            }]
        );
    }
}
