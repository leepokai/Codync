//! Build identity. Development and production must never own the same service or data.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Environment {
    Main,
    Dev,
}

impl Environment {
    pub const fn current() -> Self {
        match option_env!("CODYNC_ENV") {
            Some(value) => match value.as_bytes() {
                b"main" => Self::Main,
                _ => Self::Dev,
            },
            None => Self::Dev,
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Main => "main",
            Self::Dev => "dev",
        }
    }

    pub const fn port(self) -> u16 {
        match self {
            Self::Main => 19222,
            Self::Dev => 19223,
        }
    }

    pub const fn data_folder(self) -> &'static str {
        match self {
            Self::Main => ".codync",
            Self::Dev => ".codync-dev",
        }
    }

    #[cfg(any(unix, test))]
    pub const fn service_label(self) -> &'static str {
        match self {
            Self::Main => "com.pokai.codync.host",
            Self::Dev => "com.pokai.codync.dev.host",
        }
    }

    #[cfg(any(unix, test))]
    pub const fn systemd_unit(self) -> &'static str {
        match self {
            Self::Main => "codync-host.service",
            Self::Dev => "codync-dev-host.service",
        }
    }

    #[cfg(any(windows, test))]
    pub const fn windows_value(self) -> &'static str {
        match self {
            Self::Main => "CodyncHost",
            Self::Dev => "CodyncDevHost",
        }
    }

    #[cfg(any(windows, test))]
    pub const fn supervisor(self) -> &'static str {
        match self {
            Self::Main => "codync-hostw.exe",
            Self::Dev => "codync-dev-hostw.exe",
        }
    }

    pub const fn cloud_url(self) -> &'static str {
        match self {
            Self::Main => "https://api.codync.dev",
            Self::Dev => "https://dev-api.codync.dev",
        }
    }

    pub const fn url_scheme(self) -> &'static str {
        match self {
            Self::Main => "codync",
            Self::Dev => "codync-dev",
        }
    }

    pub fn require_release_updates(self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self == Self::Main,
            "Codync Dev does not install production updates; rebuild the development app"
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::Environment::{Dev, Main};

    #[test]
    fn development_cannot_replace_production_resources() {
        assert_eq!(Main.port(), 19222);
        assert_eq!(Main.data_folder(), ".codync");
        assert_eq!(Main.service_label(), "com.pokai.codync.host");
        assert_eq!(Main.systemd_unit(), "codync-host.service");
        assert_eq!(Main.windows_value(), "CodyncHost");
        assert_eq!(Main.supervisor(), "codync-hostw.exe");
        assert_ne!(Main.port(), Dev.port());
        for (main, dev) in [
            (Main.data_folder(), Dev.data_folder()),
            (Main.service_label(), Dev.service_label()),
            (Main.systemd_unit(), Dev.systemd_unit()),
            (Main.windows_value(), Dev.windows_value()),
            (Main.supervisor(), Dev.supervisor()),
            (Main.cloud_url(), Dev.cloud_url()),
            (Main.url_scheme(), Dev.url_scheme()),
        ] {
            assert_ne!(main, dev);
        }
        assert!(Dev.require_release_updates().is_err());
        assert!(Main.require_release_updates().is_ok());
    }
}
