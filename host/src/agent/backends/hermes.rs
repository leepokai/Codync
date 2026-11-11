//! Source reference: Hermes scripts/install.sh and scripts/install.ps1 (MIT, 10d25f8a).
//! Adaptation: pin the bootstrap and installed revision; reuse upstream locked PM provisioning.

#[cfg(test)]
const REVISION: &str = "10d25f8ad65fdf71304e9272959421a0c22e5d23";

pub(super) const INSTALL_UNIX: &str = r#"bash -o pipefail -c 'curl -fsSL https://raw.githubusercontent.com/NousResearch/hermes-agent/10d25f8ad65fdf71304e9272959421a0c22e5d23/scripts/install.sh | bash -s -- --commit 10d25f8ad65fdf71304e9272959421a0c22e5d23 --non-interactive --skip-browser --skip-computer-use'"#;

pub(super) const INSTALL_WINDOWS: &str = r#"powershell -NoProfile -ExecutionPolicy Bypass -Command "& ([scriptblock]::Create((Invoke-RestMethod -ErrorAction Stop 'https://raw.githubusercontent.com/NousResearch/hermes-agent/10d25f8ad65fdf71304e9272959421a0c22e5d23/scripts/install.ps1'))) -Commit '10d25f8ad65fdf71304e9272959421a0c22e5d23' -NonInteractive -SkipBrowser -SkipComputerUse""#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hermes_installer_pins_both_bootstrap_and_runtime_without_duplicate_desktop_tools() {
        for command in [INSTALL_UNIX, INSTALL_WINDOWS] {
            assert_eq!(command.matches(REVISION).count(), 2);
            assert!(command.contains("raw.githubusercontent.com/NousResearch/hermes-agent/"));
            assert!(!command.contains("hermes-agent.nousresearch.com"));
            let command = command.to_ascii_lowercase();
            assert!(command.contains("skip-browser") || command.contains("skipbrowser"));
            assert!(command.contains("skip-computer-use") || command.contains("skipcomputeruse"));
            assert!(command.contains("non-interactive") || command.contains("noninteractive"));
        }
        assert!(INSTALL_UNIX.contains("pipefail"));
        assert!(INSTALL_WINDOWS.contains("Invoke-RestMethod -ErrorAction Stop"));
    }
}
