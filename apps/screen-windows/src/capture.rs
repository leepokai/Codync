//! Desktop Duplication of one monitor, cropped, scaled and converted to NV12 on the GPU.

use crate::displays::{self, Monitor};
use crate::view::{Rect, View};
use anyhow::{Context, Result, bail};
use std::mem::ManuallyDrop;
use std::time::Duration;
use windows::Win32::Foundation::{HMODULE, RECT};
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_UNKNOWN;
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_NV12, DXGI_RATIONAL, DXGI_SAMPLE_DESC};
use windows::Win32::Graphics::Dxgi::{
    DXGI_ERROR_ACCESS_LOST, DXGI_ERROR_WAIT_TIMEOUT, DXGI_OUTDUPL_FRAME_INFO, IDXGIOutput1,
    IDXGIOutputDuplication, IDXGIResource,
};
use windows::core::Interface;

/// The converted frame in system memory: NV12, `width * height * 3 / 2` bytes, rows without padding.
pub struct Nv12 {
    pub data: Vec<u8>,
}

pub enum Grab {
    /// The desktop changed (or a repeat was asked for): a fresh frame in `Capture::frame`.
    Frame,
    /// Nothing changed within the timeout.
    Idle,
}

pub struct Capture {
    monitor: Monitor,
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    duplication: IDXGIOutputDuplication,
    /// A copy of the latest desktop, kept so a crop change or a keyframe can redraw without a new frame.
    desktop: ID3D11Texture2D,
    video: Converter,
    pub frame: Nv12,
}

impl Capture {
    pub fn new(monitor: &Monitor, view: View) -> Result<Self> {
        let adapter = displays::adapter(monitor)?;
        let (device, context) = create_device(&adapter)?;
        let duplication = duplicate(monitor, &device)?;
        let desktop = bgra_texture(&device, monitor.width, monitor.height)?;
        let video = Converter::new(&device, &context, monitor, view)?;
        let (w, h) = view.size;
        Ok(Self {
            monitor: monitor.clone(),
            device,
            context,
            duplication,
            desktop,
            video,
            frame: Nv12 {
                data: vec![0; (w * h * 3 / 2) as usize],
            },
        })
    }

    /// Waits up to `timeout` for the desktop to change, then converts it into `frame`.
    pub fn grab(&mut self, timeout: Duration) -> Result<Grab> {
        let mut info = DXGI_OUTDUPL_FRAME_INFO::default();
        let mut resource: Option<IDXGIResource> = None;
        let millis = u32::try_from(timeout.as_millis()).unwrap_or(u32::MAX);
        // SAFETY: both out parameters are ours; the frame is released right after the copy.
        let acquired = unsafe {
            self.duplication
                .AcquireNextFrame(millis, &mut info, &mut resource)
        };
        match acquired {
            Ok(()) => {}
            Err(error) if error.code() == DXGI_ERROR_WAIT_TIMEOUT => return Ok(Grab::Idle),
            Err(error) if error.code() == DXGI_ERROR_ACCESS_LOST => {
                // A mode change, a secure desktop (UAC, lock screen) or a driver reset: start over.
                self.duplication = duplicate(&self.monitor, &self.device)
                    .context("the desktop can't be captured right now")?;
                return Ok(Grab::Idle);
            }
            Err(error) => return Err(error).context("capturing the desktop"),
        }
        let copied = (|| -> Result<()> {
            let texture: ID3D11Texture2D = resource.context("no desktop image")?.cast()?;
            // SAFETY: same device, same size and format.
            unsafe { self.context.CopyResource(&self.desktop, &texture) };
            Ok(())
        })();
        // SAFETY: releases the frame acquired above.
        let released = unsafe { self.duplication.ReleaseFrame() };
        copied?;
        released?;
        self.render()?;
        Ok(Grab::Frame)
    }

    /// Redraws the last desktop image, e.g. for a keyframe on a still screen.
    pub fn redraw(&mut self) -> Result<()> {
        self.render()
    }

    pub fn set_view(&mut self, view: View) -> Result<()> {
        self.video = Converter::new(&self.device, &self.context, &self.monitor, view)?;
        let (w, h) = view.size;
        self.frame = Nv12 {
            data: vec![0; (w * h * 3 / 2) as usize],
        };
        self.render()
    }

    fn render(&mut self) -> Result<()> {
        self.video.convert(&self.desktop, &mut self.frame)
    }
}

fn create_device(
    adapter: &windows::Win32::Graphics::Dxgi::IDXGIAdapter1,
) -> Result<(ID3D11Device, ID3D11DeviceContext)> {
    let mut device = None;
    let mut context = None;
    // SAFETY: out parameters are ours. The BGRA flag lets the duplication's textures be copied; VIDEO adds the converter.
    unsafe {
        D3D11CreateDevice(
            adapter,
            D3D_DRIVER_TYPE_UNKNOWN,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT | D3D11_CREATE_DEVICE_VIDEO_SUPPORT,
            None,
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            Some(&mut context),
        )
        .context("creating the Direct3D device")?;
    }
    Ok((
        device.context("no Direct3D device")?,
        context.context("no Direct3D context")?,
    ))
}

fn duplicate(monitor: &Monitor, device: &ID3D11Device) -> Result<IDXGIOutputDuplication> {
    let output: IDXGIOutput1 = displays::output(monitor)?.cast()?;
    // SAFETY: `device` outlives the duplication through the `Capture` that owns both.
    unsafe { output.DuplicateOutput(device) }
        .context("duplicating the output (is the desktop locked or on a secure screen?)")
}

fn bgra_texture(device: &ID3D11Device, width: u32, height: u32) -> Result<ID3D11Texture2D> {
    let desc = D3D11_TEXTURE2D_DESC {
        Width: width,
        Height: height,
        MipLevels: 1,
        ArraySize: 1,
        Format: windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Usage: D3D11_USAGE_DEFAULT,
        BindFlags: (D3D11_BIND_SHADER_RESOURCE.0 | D3D11_BIND_RENDER_TARGET.0) as u32,
        ..Default::default()
    };
    let mut texture = None;
    // SAFETY: `desc` is fully initialised.
    unsafe { device.CreateTexture2D(&desc, None, Some(&mut texture)) }
        .context("creating the desktop copy")?;
    texture.context("no desktop copy")
}

/// BGRA desktop to cropped, scaled NV12 through the GPU's video processor, read back into memory.
struct Converter {
    context: ID3D11DeviceContext,
    video_context: ID3D11VideoContext,
    video_device: ID3D11VideoDevice,
    enumerator: ID3D11VideoProcessorEnumerator,
    processor: ID3D11VideoProcessor,
    output_view: ID3D11VideoProcessorOutputView,
    nv12: ID3D11Texture2D,
    staging: ID3D11Texture2D,
    crop: Rect,
    size: (u32, u32),
}

impl Converter {
    fn new(
        device: &ID3D11Device,
        context: &ID3D11DeviceContext,
        monitor: &Monitor,
        view: View,
    ) -> Result<Self> {
        let (w, h) = view.size;
        let video_device: ID3D11VideoDevice =
            device.cast().context("this GPU has no video processor")?;
        let video_context: ID3D11VideoContext = context.cast()?;
        let content = D3D11_VIDEO_PROCESSOR_CONTENT_DESC {
            InputFrameFormat: D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE,
            InputFrameRate: DXGI_RATIONAL {
                Numerator: 60,
                Denominator: 1,
            },
            InputWidth: monitor.width,
            InputHeight: monitor.height,
            OutputFrameRate: DXGI_RATIONAL {
                Numerator: 60,
                Denominator: 1,
            },
            OutputWidth: w,
            OutputHeight: h,
            Usage: D3D11_VIDEO_USAGE_PLAYBACK_NORMAL,
        };
        // SAFETY: descriptors are fully initialised, textures and views stay owned by `Self`.
        unsafe {
            let enumerator = video_device.CreateVideoProcessorEnumerator(&content)?;
            let processor = video_device.CreateVideoProcessor(&enumerator, 0)?;
            let mut desc = D3D11_TEXTURE2D_DESC {
                Width: w,
                Height: h,
                MipLevels: 1,
                ArraySize: 1,
                Format: DXGI_FORMAT_NV12,
                SampleDesc: DXGI_SAMPLE_DESC {
                    Count: 1,
                    Quality: 0,
                },
                Usage: D3D11_USAGE_DEFAULT,
                BindFlags: D3D11_BIND_RENDER_TARGET.0 as u32,
                ..Default::default()
            };
            let mut nv12 = None;
            device
                .CreateTexture2D(&desc, None, Some(&mut nv12))
                .context("this GPU can't make NV12 surfaces")?;
            let nv12 = nv12.context("no NV12 surface")?;
            desc.Usage = D3D11_USAGE_STAGING;
            desc.BindFlags = 0;
            desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
            let mut staging = None;
            device.CreateTexture2D(&desc, None, Some(&mut staging))?;
            let staging = staging.context("no staging surface")?;
            let view_desc = D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC {
                ViewDimension: D3D11_VPOV_DIMENSION_TEXTURE2D,
                Anonymous: D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC_0 {
                    Texture2D: D3D11_TEX2D_VPOV { MipSlice: 0 },
                },
            };
            let mut output_view = None;
            video_device.CreateVideoProcessorOutputView(
                &nv12,
                &enumerator,
                &view_desc,
                Some(&mut output_view),
            )?;
            let output_view = output_view.context("no output view")?;
            // Full-range RGB in; BT.709, limited-range YCbCr out.
            let rgb = D3D11_VIDEO_PROCESSOR_COLOR_SPACE { _bitfield: 0 };
            let yuv = D3D11_VIDEO_PROCESSOR_COLOR_SPACE {
                _bitfield: (1 << 2) | (1 << 4),
            };
            video_context.VideoProcessorSetStreamColorSpace(&processor, 0, &rgb);
            video_context.VideoProcessorSetOutputColorSpace(&processor, &yuv);
            let out = RECT {
                left: 0,
                top: 0,
                right: i32::try_from(w)?,
                bottom: i32::try_from(h)?,
            };
            video_context.VideoProcessorSetOutputTargetRect(&processor, true, Some(&raw const out));
            video_context.VideoProcessorSetStreamDestRect(
                &processor,
                0,
                true,
                Some(&raw const out),
            );
            Ok(Self {
                context: context.clone(),
                video_context,
                video_device,
                enumerator,
                processor,
                output_view,
                nv12,
                staging,
                crop: view.crop,
                size: view.size,
            })
        }
    }

    fn convert(&mut self, desktop: &ID3D11Texture2D, frame: &mut Nv12) -> Result<()> {
        // SAFETY: the views and textures belong to the same device; the staging map is released before returning.
        unsafe {
            let desc = D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC {
                FourCC: 0,
                ViewDimension: D3D11_VPIV_DIMENSION_TEXTURE2D,
                Anonymous: D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC_0 {
                    Texture2D: D3D11_TEX2D_VPIV {
                        MipSlice: 0,
                        ArraySlice: 0,
                    },
                },
            };
            let mut input = None;
            self.video_device
                .CreateVideoProcessorInputView(desktop, &self.enumerator, &desc, Some(&mut input))
                .context("input view")?;
            let input = input.context("no input view")?;
            #[expect(clippy::cast_possible_truncation, reason = "pixel coordinates")]
            let src = RECT {
                left: self.crop.x as i32,
                top: self.crop.y as i32,
                right: (self.crop.x + self.crop.w) as i32,
                bottom: (self.crop.y + self.crop.h) as i32,
            };
            self.video_context.VideoProcessorSetStreamSourceRect(
                &self.processor,
                0,
                true,
                Some(&raw const src),
            );
            let stream = D3D11_VIDEO_PROCESSOR_STREAM {
                Enable: true.into(),
                pInputSurface: ManuallyDrop::new(Some(input)),
                ..Default::default()
            };
            let blt = self.video_context.VideoProcessorBlt(
                &self.processor,
                &self.output_view,
                0,
                std::slice::from_ref(&stream),
            );
            drop(ManuallyDrop::into_inner(stream.pInputSurface));
            blt.context("converting the desktop image")?;
            self.context.CopyResource(&self.staging, &self.nv12);
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            self.context
                .Map(&self.staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
                .context("mapping the frame")?;
            let (w, h) = (self.size.0 as usize, self.size.1 as usize);
            let pitch = mapped.RowPitch as usize;
            let base = mapped.pData.cast::<u8>();
            if base.is_null() || pitch < w {
                self.context.Unmap(&self.staging, 0);
                bail!("the GPU returned an unusable frame");
            }
            // The chroma plane follows the luma plane, `pitch` bytes per row.
            for row in 0..h {
                let src = std::slice::from_raw_parts(base.add(row * pitch), w);
                frame.data[row * w..(row + 1) * w].copy_from_slice(src);
            }
            for row in 0..h / 2 {
                let src = std::slice::from_raw_parts(base.add((h + row) * pitch), w);
                frame.data[(h + row) * w..(h + row + 1) * w].copy_from_slice(src);
            }
            self.context.Unmap(&self.staging, 0);
        }
        Ok(())
    }
}
