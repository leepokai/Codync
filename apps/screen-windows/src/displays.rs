//! The desktop's monitors, as DXGI sees them (the same ones Desktop Duplication captures).

use anyhow::{Context, Result};
use serde_json::{Value, json};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, DXGI_ERROR_NOT_FOUND, IDXGIAdapter1, IDXGIFactory1, IDXGIOutput,
};

/// One monitor attached to the desktop. Ids count adapters' outputs in enumeration order from 1.
#[derive(Clone, Debug)]
pub struct Monitor {
    pub id: u32,
    pub name: String,
    pub adapter: u32,
    pub output: u32,
    /// Top-left corner on the virtual desktop, in pixels.
    pub left: i32,
    pub top: i32,
    pub width: u32,
    pub height: u32,
}

impl Monitor {
    pub fn main(&self) -> bool {
        self.left == 0 && self.top == 0
    }

    /// What the host's `Display` carries; sizes are pixels because the helper is per-monitor DPI aware.
    pub fn status(&self) -> Value {
        json!({"id": self.id, "name": self.name, "width": self.width, "height": self.height, "main": self.main()})
    }
}

pub fn list() -> Result<Vec<Monitor>> {
    // SAFETY: plain DXGI enumeration; every interface is released by its wrapper.
    unsafe {
        let factory: IDXGIFactory1 = CreateDXGIFactory1().context("creating the DXGI factory")?;
        let mut monitors = Vec::new();
        let mut adapter_index = 0;
        while let Ok(adapter) = factory.EnumAdapters1(adapter_index) {
            let mut output_index = 0;
            loop {
                match adapter.EnumOutputs(output_index) {
                    Ok(output) => {
                        if let Some(monitor) =
                            monitor(&output, adapter_index, output_index, monitors.len())?
                        {
                            monitors.push(monitor);
                        }
                    }
                    Err(error) if error.code() == DXGI_ERROR_NOT_FOUND => break,
                    Err(error) => return Err(error).context("listing monitors"),
                }
                output_index += 1;
            }
            adapter_index += 1;
        }
        Ok(monitors)
    }
}

/// The adapter a monitor hangs off (capture must use the same one).
pub fn adapter(monitor: &Monitor) -> Result<IDXGIAdapter1> {
    // SAFETY: plain DXGI enumeration.
    unsafe {
        let factory: IDXGIFactory1 = CreateDXGIFactory1()?;
        Ok(factory.EnumAdapters1(monitor.adapter)?)
    }
}

pub fn output(monitor: &Monitor) -> Result<IDXGIOutput> {
    Ok(unsafe { adapter(monitor)?.EnumOutputs(monitor.output)? })
}

fn monitor(
    output: &IDXGIOutput,
    adapter: u32,
    index: u32,
    count: usize,
) -> Result<Option<Monitor>> {
    // SAFETY: `GetDesc` fills a struct we own.
    let desc = unsafe { output.GetDesc()? };
    if !desc.AttachedToDesktop.as_bool() {
        return Ok(None);
    }
    let rect = desc.DesktopCoordinates;
    let name_len = desc
        .DeviceName
        .iter()
        .position(|&c| c == 0)
        .unwrap_or(desc.DeviceName.len());
    let device = String::from_utf16_lossy(&desc.DeviceName[..name_len]);
    Ok(Some(Monitor {
        id: u32::try_from(count + 1)?,
        name: format!("Display {}", device.trim_start_matches(r"\\.\DISPLAY")),
        adapter,
        output: index,
        left: rect.left,
        top: rect.top,
        width: u32::try_from(rect.right - rect.left)?,
        height: u32::try_from(rect.bottom - rect.top)?,
    }))
}
