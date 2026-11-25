//! Converts BGRA pictures to the NV12 that the encoder takes, on the GPU, in BT.709 limited range.

use std::ptr::{self, NonNull};

use objc2::{rc::Retained, runtime::ProtocolObject};
use objc2_core_foundation::{CFBoolean, CFDictionary, CFNumber, CFRetained, CFString, CFType};
use objc2_core_video::{
    CVAttachmentMode, CVBuffer, CVPixelBuffer, CVPixelBufferGetBaseAddress,
    CVPixelBufferGetBytesPerRow, CVPixelBufferGetIOSurface, CVPixelBufferLockBaseAddress,
    CVPixelBufferLockFlags, CVPixelBufferPool, CVPixelBufferUnlockBaseAddress,
    kCVImageBufferColorPrimaries_ITU_R_709_2, kCVImageBufferColorPrimariesKey,
    kCVImageBufferTransferFunction_ITU_R_709_2, kCVImageBufferTransferFunctionKey,
    kCVImageBufferYCbCrMatrix_ITU_R_709_2, kCVImageBufferYCbCrMatrixKey, kCVPixelBufferHeightKey,
    kCVPixelBufferIOSurfacePropertiesKey, kCVPixelBufferMetalCompatibilityKey,
    kCVPixelBufferPixelFormatTypeKey, kCVPixelBufferWidthKey, kCVPixelFormatType_32BGRA,
    kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange,
};
use objc2_foundation::NSString;
use objc2_io_surface::IOSurfaceRef;
use objc2_metal::{
    MTLCommandBuffer, MTLCommandBufferStatus, MTLCommandEncoder, MTLCommandQueue,
    MTLComputeCommandEncoder, MTLComputePipelineState, MTLCreateSystemDefaultDevice, MTLDevice,
    MTLLibrary, MTLPixelFormat, MTLSize, MTLTexture, MTLTextureDescriptor, MTLTextureUsage,
};

const KERNEL: &str = r"
#include <metal_stdlib>
using namespace metal;

constant float3 LUMA = float3(0.2126, 0.7152, 0.0722);

kernel void nv12(texture2d<float, access::read> picture [[texture(0)]],
                 texture2d<float, access::write> luma [[texture(1)]],
                 texture2d<float, access::write> chroma [[texture(2)]],
                 uint2 block [[thread_position_in_grid]]) {
    uint2 corner = block * 2;
    if (corner.x >= luma.get_width() || corner.y >= luma.get_height()) {
        return;
    }
    float3 sum = 0.0;
    for (uint row = 0; row < 2; row++) {
        for (uint column = 0; column < 2; column++) {
            uint2 at = corner + uint2(column, row);
            float3 rgb = picture.read(at).rgb;
            luma.write(float4((16.0 + 219.0 * dot(rgb, LUMA)) / 255.0), at);
            sum += rgb;
        }
    }
    float3 rgb = sum * 0.25;
    float y = dot(rgb, LUMA);
    float cb = (rgb.b - y) / 1.8556;
    float cr = (rgb.r - y) / 1.5748;
    chroma.write(float4((128.0 + 224.0 * cb) / 255.0, (128.0 + 224.0 * cr) / 255.0, 0.0, 1.0), block);
}
";

type Device = ProtocolObject<dyn MTLDevice>;
type Texture = Retained<ProtocolObject<dyn MTLTexture>>;

pub(super) struct Converter {
    device: Retained<Device>,
    queue: Retained<ProtocolObject<dyn MTLCommandQueue>>,
    pipeline: Retained<ProtocolObject<dyn MTLComputePipelineState>>,
    pictures: Option<Pool>,
    staging: Option<Pool>,
}

struct Pool {
    raw: CFRetained<CVPixelBufferPool>,
    width: usize,
    height: usize,
}

#[expect(
    clippy::non_send_fields_in_send_ty,
    reason = "Core Foundation types are thread safe, but objc2 marks them all as !Send"
)]
// SAFETY: Core Video retains and releases pools atomically, and the converter that owns a pool
// uses it from one thread at a time.
unsafe impl Send for Pool {}

impl Pool {
    fn new(format: u32, width: usize, height: usize) -> Result<Self, String> {
        let number = |value: usize| CFNumber::new_i64(i64::try_from(value).unwrap_or(i64::MAX));
        let (format_value, wide, high) = (
            CFNumber::new_i64(i64::from(format)),
            number(width),
            number(height),
        );
        let surface = CFDictionary::<CFString, CFType>::from_slices(&[], &[]);
        // SAFETY: these are Core Video's constant keys.
        let keys = unsafe {
            [
                kCVPixelBufferPixelFormatTypeKey,
                kCVPixelBufferWidthKey,
                kCVPixelBufferHeightKey,
                kCVPixelBufferIOSurfacePropertiesKey,
                kCVPixelBufferMetalCompatibilityKey,
            ]
        };
        let values: [&CFType; 5] = [&format_value, &wide, &high, &surface, CFBoolean::new(true)];
        let attributes = CFDictionary::from_slices(&keys, &values);
        let mut raw = ptr::null_mut();
        // SAFETY: the attributes are a valid dictionary and the out pointer is valid.
        let status = unsafe {
            CVPixelBufferPool::create(
                None,
                None,
                Some(attributes.as_opaque()),
                NonNull::from(&mut raw),
            )
        };
        let raw = NonNull::new(raw)
            .filter(|_| status == 0)
            .ok_or_else(|| format!("no picture pool: {status}"))?;
        Ok(Self {
            // SAFETY: the pool was just created with a +1 retain count.
            raw: unsafe { CFRetained::from_raw(raw) },
            width,
            height,
        })
    }

    fn take(&self) -> Result<CFRetained<CVPixelBuffer>, String> {
        let mut raw = ptr::null_mut();
        // SAFETY: the pool is valid and the out pointer is valid.
        let status = unsafe {
            CVPixelBufferPool::create_pixel_buffer(None, &self.raw, NonNull::from(&mut raw))
        };
        let raw = NonNull::new(raw)
            .filter(|_| status == 0)
            .ok_or_else(|| format!("no picture buffer: {status}"))?;
        // SAFETY: the buffer was just created with a +1 retain count.
        Ok(unsafe { CFRetained::from_raw(raw) })
    }
}

fn kept(
    pool: &mut Option<Pool>,
    format: u32,
    width: usize,
    height: usize,
) -> Result<&Pool, String> {
    if pool
        .as_ref()
        .is_none_or(|pool| (pool.width, pool.height) != (width, height))
    {
        *pool = Some(Pool::new(format, width, height)?);
    }
    pool.as_ref().ok_or_else(|| "no picture pool".to_owned())
}

impl Converter {
    pub(super) fn new() -> Result<Self, String> {
        let device = MTLCreateSystemDefaultDevice().ok_or("no Metal device")?;
        let queue = device.newCommandQueue().ok_or("no Metal queue")?;
        let library = device
            .newLibraryWithSource_options_error(&NSString::from_str(KERNEL), None)
            .map_err(|problem| format!("the colour shader did not compile: {problem}"))?;
        let function = library
            .newFunctionWithName(&NSString::from_str("nv12"))
            .ok_or("no colour shader")?;
        let pipeline = device
            .newComputePipelineStateWithFunction_error(&function)
            .map_err(|problem| format!("no colour pipeline: {problem}"))?;
        Ok(Self {
            device,
            queue,
            pipeline,
            pictures: None,
            staging: None,
        })
    }

    /// Waits for the GPU, because the source surface is only valid during the call.
    pub(super) fn surface(
        &mut self,
        surface: &IOSurfaceRef,
        width: usize,
        height: usize,
    ) -> Result<CFRetained<CVPixelBuffer>, String> {
        let picture = kept(
            &mut self.pictures,
            kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange,
            width,
            height,
        )?
        .take()?;
        let target = CVPixelBufferGetIOSurface(Some(&picture)).ok_or("no picture surface")?;
        let source = self.texture(surface, MTLPixelFormat::BGRA8Unorm, width, height, 0)?;
        let luma = self.texture(&target, MTLPixelFormat::R8Unorm, width, height, 0)?;
        let chroma = self.texture(&target, MTLPixelFormat::RG8Unorm, width / 2, height / 2, 1)?;
        self.convert(&source, &luma, &chroma, width, height)?;
        tag(&picture);
        Ok(picture)
    }

    pub(super) fn pixels(
        &mut self,
        bgra: &[u8],
        width: usize,
        height: usize,
        stride: usize,
    ) -> Result<CFRetained<CVPixelBuffer>, String> {
        let staged = kept(&mut self.staging, kCVPixelFormatType_32BGRA, width, height)?.take()?;
        copy_rows(&staged, bgra, width, height, stride)?;
        let surface = CVPixelBufferGetIOSurface(Some(&staged)).ok_or("no staging surface")?;
        self.surface(&surface, width, height)
    }

    fn texture(
        &self,
        surface: &IOSurfaceRef,
        format: MTLPixelFormat,
        width: usize,
        height: usize,
        plane: usize,
    ) -> Result<Texture, String> {
        // SAFETY: the format and sizes describe a 2D texture without mipmaps.
        let descriptor = unsafe {
            MTLTextureDescriptor::texture2DDescriptorWithPixelFormat_width_height_mipmapped(
                format, width, height, false,
            )
        };
        descriptor.setUsage(MTLTextureUsage::ShaderRead | MTLTextureUsage::ShaderWrite);
        self.device
            .newTextureWithDescriptor_iosurface_plane(&descriptor, surface, plane)
            .ok_or_else(|| format!("no texture for plane {plane}"))
    }

    fn convert(
        &self,
        source: &Texture,
        luma: &Texture,
        chroma: &Texture,
        width: usize,
        height: usize,
    ) -> Result<(), String> {
        let buffer = self.queue.commandBuffer().ok_or("no command buffer")?;
        let encoder = buffer.computeCommandEncoder().ok_or("no compute encoder")?;
        encoder.setComputePipelineState(&self.pipeline);
        // SAFETY: the textures match the shader's three bindings and outlive the command buffer,
        // which this function waits for.
        unsafe {
            encoder.setTexture_atIndex(Some(source), 0);
            encoder.setTexture_atIndex(Some(luma), 1);
            encoder.setTexture_atIndex(Some(chroma), 2);
        }
        encoder.dispatchThreads_threadsPerThreadgroup(
            MTLSize {
                width: width / 2,
                height: height / 2,
                depth: 1,
            },
            MTLSize {
                width: 16,
                height: 16,
                depth: 1,
            },
        );
        encoder.endEncoding();
        buffer.commit();
        buffer.waitUntilCompleted();
        let status = buffer.status();
        (status == MTLCommandBufferStatus::Completed)
            .then_some(())
            .ok_or_else(|| format!("the colour conversion failed: {}", status.0))
    }
}

fn copy_rows(
    buffer: &CVPixelBuffer,
    bgra: &[u8],
    width: usize,
    height: usize,
    stride: usize,
) -> Result<(), String> {
    let row = width * 4;
    if stride < row || bgra.len() < stride * (height - 1) + row {
        return Err("the picture is smaller than its size".to_owned());
    }
    // SAFETY: the buffer is locked while its rows are written, and each written row fits the
    // buffer's own stride.
    unsafe {
        CVPixelBufferLockBaseAddress(buffer, CVPixelBufferLockFlags(0));
        let base = CVPixelBufferGetBaseAddress(buffer).cast::<u8>();
        let target = CVPixelBufferGetBytesPerRow(buffer);
        if !base.is_null() && target >= row {
            for line in 0..height {
                ptr::copy_nonoverlapping(
                    bgra.as_ptr().add(line * stride),
                    base.add(line * target),
                    row,
                );
            }
        }
        CVPixelBufferUnlockBaseAddress(buffer, CVPixelBufferLockFlags(0));
    }
    Ok(())
}

/// Says how the picture's colours were converted, so the encoder writes the same into the stream.
fn tag(picture: &CVBuffer) {
    // SAFETY: these are Core Video's constant keys and values.
    unsafe {
        for (key, value) in [
            (
                kCVImageBufferYCbCrMatrixKey,
                kCVImageBufferYCbCrMatrix_ITU_R_709_2,
            ),
            (
                kCVImageBufferColorPrimariesKey,
                kCVImageBufferColorPrimaries_ITU_R_709_2,
            ),
            (
                kCVImageBufferTransferFunctionKey,
                kCVImageBufferTransferFunction_ITU_R_709_2,
            ),
        ] {
            picture.set_attachment(key, value, CVAttachmentMode::ShouldPropagate);
        }
    }
}
