//! H.264 through Media Foundation's encoder transform: Constrained Baseline, low latency, constant bitrate.

use anyhow::{Context, Result, bail};
use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance};
use windows::Win32::System::Variant::VARIANT;
use windows::core::{GUID, Interface};

/// 100 ns units, Media Foundation's clock.
const TICKS_PER_SECOND: i64 = 10_000_000;

pub struct Encoder {
    transform: IMFTransform,
    codec: ICodecAPI,
    fps: u32,
    frames: i64,
    output_size: u32,
}

/// Starts Media Foundation for the process; call once before any [`Encoder`].
pub fn startup() -> Result<()> {
    // SAFETY: documented process-wide initialisation.
    unsafe { MFStartup(MF_VERSION, MFSTARTUP_FULL) }.context("starting Media Foundation")
}

impl Encoder {
    pub fn new(width: u32, height: u32, fps: u32, bitrate: u32) -> Result<Self> {
        // SAFETY: COM calls on interfaces we own; every VARIANT and media type outlives its call.
        unsafe {
            let transform: IMFTransform =
                CoCreateInstance(&CLSID_MSH264EncoderMFT, None, CLSCTX_INPROC_SERVER)
                    .context("no H.264 encoder on this Windows")?;
            let codec: ICodecAPI = transform.cast()?;
            let set = |name: &str, api: &GUID, value: VARIANT| {
                codec
                    .SetValue(api, &value)
                    .with_context(|| format!("encoder setting {name}"))
            };
            set(
                "low latency",
                &CODECAPI_AVLowLatencyMode,
                VARIANT::from(true),
            )?;
            set(
                "rate control",
                &CODECAPI_AVEncCommonRateControlMode,
                VARIANT::from(u32::try_from(eAVEncCommonRateControlMode_CBR.0)?),
            )?;
            set(
                "bitrate",
                &CODECAPI_AVEncCommonMeanBitRate,
                VARIANT::from(bitrate),
            )?;
            set(
                "GOP size",
                &CODECAPI_AVEncMPVGOPSize,
                VARIANT::from(fps * 2),
            )?;
            // B-frames add latency.
            set(
                "B-frames",
                &CODECAPI_AVEncMPVDefaultBPictureCount,
                VARIANT::from(0u32),
            )?;

            let out = media_type(&MFVideoFormat_H264, width, height, fps)?;
            out.SetUINT32(&MF_MT_AVG_BITRATE, bitrate)?;
            out.SetUINT32(
                &MF_MT_MPEG2_PROFILE,
                u32::try_from(eAVEncH264VProfile_Base.0)?,
            )?;
            transform
                .SetOutputType(0, &out, 0)
                .context("the H.264 encoder refused the output format")?;
            let input = media_type(&MFVideoFormat_NV12, width, height, fps)?;
            transform
                .SetInputType(0, &input, 0)
                .context("the H.264 encoder refused NV12 input")?;
            transform.ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, 0)?;
            transform.ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM, 0)?;
            let info = transform.GetOutputStreamInfo(0)?;
            Ok(Self {
                transform,
                codec,
                fps,
                frames: 0,
                output_size: info.cbSize.max(1 << 20),
            })
        }
    }

    /// Makes the next frame a keyframe.
    pub fn force_keyframe(&self) -> Result<()> {
        // SAFETY: plain property call.
        unsafe {
            self.codec
                .SetValue(&CODECAPI_AVEncVideoForceKeyFrame, &VARIANT::from(1u32))
        }
        .context("requesting a keyframe")
    }

    /// Encodes one NV12 frame; returns the Annex B access units the encoder has ready (usually one).
    pub fn encode(&mut self, nv12: &[u8]) -> Result<Vec<Vec<u8>>> {
        // SAFETY: the buffer is locked only while copied; samples are COM objects released on drop.
        unsafe {
            let buffer = MFCreateMemoryBuffer(u32::try_from(nv12.len())?)?;
            let mut data = std::ptr::null_mut();
            buffer.Lock(&mut data, None, None)?;
            std::ptr::copy_nonoverlapping(nv12.as_ptr(), data, nv12.len());
            buffer.Unlock()?;
            buffer.SetCurrentLength(u32::try_from(nv12.len())?)?;
            let sample = MFCreateSample()?;
            sample.AddBuffer(&buffer)?;
            let duration = TICKS_PER_SECOND / i64::from(self.fps);
            sample.SetSampleTime(self.frames * duration)?;
            sample.SetSampleDuration(duration)?;
            self.frames += 1;
            self.transform.ProcessInput(0, &sample, 0)?;
            self.drain()
        }
    }

    unsafe fn drain(&mut self) -> Result<Vec<Vec<u8>>> {
        let mut units = Vec::new();
        loop {
            // SAFETY: the output sample is ours, sized by the encoder's own request.
            unsafe {
                let sample = MFCreateSample()?;
                sample.AddBuffer(&MFCreateMemoryBuffer(self.output_size)?)?;
                let mut output = [MFT_OUTPUT_DATA_BUFFER {
                    dwStreamID: 0,
                    pSample: std::mem::ManuallyDrop::new(Some(sample)),
                    ..Default::default()
                }];
                let mut status = 0;
                let result = self.transform.ProcessOutput(0, &mut output, &mut status);
                let [mut produced] = output;
                let sample = std::mem::ManuallyDrop::take(&mut produced.pSample);
                std::mem::ManuallyDrop::drop(&mut produced.pEvents);
                match result {
                    Ok(()) => {
                        let Some(sample) = sample else {
                            bail!("the encoder returned no sample")
                        };
                        let contiguous = sample.ConvertToContiguousBuffer()?;
                        let (mut data, mut length) = (std::ptr::null_mut(), 0);
                        contiguous.Lock(&mut data, None, Some(&mut length))?;
                        units.push(std::slice::from_raw_parts(data, length as usize).to_vec());
                        contiguous.Unlock()?;
                    }
                    Err(error) if error.code() == MF_E_TRANSFORM_NEED_MORE_INPUT => {
                        return Ok(units);
                    }
                    Err(error) => return Err(error).context("encoding a frame"),
                }
            }
        }
    }
}

impl Drop for Encoder {
    fn drop(&mut self) {
        // SAFETY: ends the stream of a transform we own.
        unsafe {
            let _ = self
                .transform
                .ProcessMessage(MFT_MESSAGE_NOTIFY_END_OF_STREAM, 0);
            let _ = self.transform.ProcessMessage(MFT_MESSAGE_COMMAND_FLUSH, 0);
        }
    }
}

unsafe fn media_type(subtype: &GUID, width: u32, height: u32, fps: u32) -> Result<IMFMediaType> {
    let pack = |hi: u32, lo: u32| (u64::from(hi) << 32) | u64::from(lo);
    // SAFETY: plain attribute setters on a fresh media type.
    unsafe {
        let t = MFCreateMediaType()?;
        t.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
        t.SetGUID(&MF_MT_SUBTYPE, subtype)?;
        t.SetUINT64(&MF_MT_FRAME_SIZE, pack(width, height))?;
        t.SetUINT64(&MF_MT_FRAME_RATE, pack(fps, 1))?;
        t.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, pack(1, 1))?;
        t.SetUINT32(
            &MF_MT_INTERLACE_MODE,
            u32::try_from(MFVideoInterlace_Progressive.0)?,
        )?;
        Ok(t)
    }
}
