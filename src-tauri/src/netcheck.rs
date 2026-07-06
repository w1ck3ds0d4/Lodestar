//! Windows networking pre-flight for the phone companion server.
//!
//! The server binds `0.0.0.0:8420` fine, but a phone still can't reach it when Windows
//! Firewall blocks inbound LAN traffic - which it does by default on a "Public" network,
//! and whenever no allow-rule exists for the port. This module detects that case so the UI
//! can warn instead of leaving the user staring at "site can't be reached", and offers a
//! one-click fix that marks the active network Private and adds an inbound rule scoped to
//! the local subnet only (never the public internet). The fix needs admin rights, so it is
//! launched through a UAC elevation prompt.

use base64::Engine;
use serde::Serialize;

/// Display name of the firewall rule we manage, so detection and creation agree.
pub const RULE_NAME: &str = "Lodestar phone companion";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhoneNetStatus {
    /// True when nothing obvious should block the phone (network not Public, rule present).
    pub ok: bool,
    /// The active network is categorized Public - Windows blocks inbound LAN by default.
    pub public_network: bool,
    /// No inbound firewall allow-rule for the companion port was found.
    pub rule_missing: bool,
    /// Whether an automatic fix is available on this platform (Windows only).
    pub can_fix: bool,
    /// Short, user-facing explanation of what (if anything) needs attention.
    pub message: String,
}

fn build_status(public_network: bool, rule_missing: bool, can_fix: bool) -> PhoneNetStatus {
    let ok = !public_network && !rule_missing;
    let message = if public_network {
        "Windows treats this network as Public, which blocks phone connections. Allowing \
         access marks it Private and opens the companion port to your local network only."
    } else if rule_missing {
        "A firewall rule is needed so your phone can reach Lodestar. It opens the companion \
         port to your local network only."
    } else {
        "Your phone should be able to connect on this network."
    }
    .to_string();
    PhoneNetStatus {
        ok,
        public_network,
        rule_missing,
        can_fix,
        message,
    }
}

/// Inspect the active network so the UI can warn before the user blames the app.
pub fn status() -> PhoneNetStatus {
    imp::status()
}

/// Mark the active network Private and add a LAN-only inbound rule. Prompts for elevation;
/// returns the refreshed status on success.
pub fn allow() -> Result<PhoneNetStatus, String> {
    imp::allow()?;
    Ok(imp::status())
}

#[cfg(target_os = "windows")]
mod imp {
    use super::*;
    use std::os::windows::process::CommandExt;
    use std::process::Command;

    // Don't flash a console window when shelling out to PowerShell from the GUI app.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    fn powershell(script: &str) -> Result<String, String> {
        let out = Command::new("powershell")
            .creation_flags(CREATE_NO_WINDOW)
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                script,
            ])
            .output()
            .map_err(|e| format!("could not run PowerShell: {e}"))?;
        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
            return Err(if err.is_empty() {
                "PowerShell command failed".to_string()
            } else {
                err
            });
        }
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    }

    pub fn status() -> super::PhoneNetStatus {
        // Read the category of the default-route interface (the one the phone URL points at)
        // and whether our firewall rule exists. On any failure, assume OK - a pre-flight
        // check should never nag in an environment it can't read.
        let script = format!(
            "$ErrorActionPreference='SilentlyContinue'\n\
             $def = Get-NetRoute -DestinationPrefix '0.0.0.0/0' | Sort-Object RouteMetric | Select-Object -First 1\n\
             $cat = 'Unknown'\n\
             if ($def) {{ $cat = (Get-NetConnectionProfile -InterfaceIndex $def.ifIndex | Select-Object -First 1).NetworkCategory.ToString() }}\n\
             $rule = Get-NetFirewallRule -DisplayName '{rule}'\n\
             [pscustomobject]@{{ category = $cat; ruleMissing = [bool]($null -eq $rule) }} | ConvertTo-Json -Compress",
            rule = super::RULE_NAME
        );
        let parsed = powershell(&script)
            .ok()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok());
        let (public_network, rule_missing) = match parsed {
            Some(v) => (
                v.get("category")
                    .and_then(|c| c.as_str())
                    .map(|c| c == "Public")
                    .unwrap_or(false),
                v.get("ruleMissing")
                    .and_then(|b| b.as_bool())
                    .unwrap_or(false),
            ),
            None => (false, false),
        };
        super::build_status(public_network, rule_missing, true)
    }

    pub fn allow() -> Result<(), String> {
        // The privileged half: mark the default-route network Private and (re)create the
        // inbound rule, scoped to the local subnet only. Idempotent so re-running is safe.
        let inner = format!(
            "$ErrorActionPreference='Stop'\n\
             $def = Get-NetRoute -DestinationPrefix '0.0.0.0/0' | Sort-Object RouteMetric | Select-Object -First 1\n\
             if ($def) {{ Set-NetConnectionProfile -InterfaceIndex $def.ifIndex -NetworkCategory Private }}\n\
             Get-NetFirewallRule -DisplayName '{rule}' -ErrorAction SilentlyContinue | Remove-NetFirewallRule\n\
             New-NetFirewallRule -DisplayName '{rule}' -Direction Inbound -Action Allow -Protocol TCP -LocalPort {port} -Profile Private -RemoteAddress LocalSubnet | Out-Null",
            rule = super::RULE_NAME,
            port = crate::phone::PORT
        );
        // Pass the inner script via -EncodedCommand (base64 UTF-16LE) so nested quoting can't
        // break it, and elevate with a UAC prompt. -Wait + the child's exit code surface the
        // result; declining UAC makes Start-Process throw, so the outer call exits non-zero.
        let utf16: Vec<u8> = inner.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
        let b64 = base64::engine::general_purpose::STANDARD.encode(&utf16);
        let outer = format!(
            "$ErrorActionPreference='Stop'; \
             $p = Start-Process powershell -Verb RunAs -WindowStyle Hidden -PassThru -Wait \
             -ArgumentList '-NoProfile','-EncodedCommand','{b64}'; \
             if ($p.ExitCode -ne 0) {{ throw \"firewall setup failed (exit $($p.ExitCode))\" }}"
        );
        powershell(&outer).map(|_| ())
    }
}

#[cfg(not(target_os = "windows"))]
mod imp {
    pub fn status() -> super::PhoneNetStatus {
        // No Windows Firewall to second-guess; assume the LAN is reachable and offer no fix.
        super::build_status(false, false, false)
    }
    pub fn allow() -> Result<(), String> {
        Err("Automatic firewall setup is only available on Windows.".to_string())
    }
}
