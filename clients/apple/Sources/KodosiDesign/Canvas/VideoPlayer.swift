import CoreMedia
import CoreVideo
import Metal
import QuartzCore
import Synchronization
import VideoToolbox

/// Decodes the browser's samples with VideoToolbox and draws each picture with Metal as soon as it
/// arrives, a frame sooner than `AVSampleBufferDisplayLayer` shows it. Samples come on the
/// `BrowserPipes` task and layout on the main thread, so drawing runs on its own queue and always
/// takes the newest picture.
final class VideoPlayer: @unchecked Sendable {
    /// CAMetalLayer hands out drawables on any thread; the views change its other properties on the
    /// main thread only.
    let layer: CAMetalLayer
    /// Only the feed's pipe task decodes.
    private var decoder = VideoDecoder()
    private let pictures = Mutex(Pictures())
    private let drawing = DispatchQueue(label: "com.kodosi.browser-video", qos: .userInteractive)

    init(layer: CAMetalLayer) {
        self.layer = layer
        layer.device = VideoGPU.device
        layer.pixelFormat = .bgra8Unorm
        layer.colorspace = CGColorSpace(name: CGColorSpace.sRGB)
        layer.framebufferOnly = true
        layer.isOpaque = false
        #if os(macOS)
            layer.displaySyncEnabled = false
        #endif
    }

    /// Returns `false` when the picture did not decode, so the feed asks for a keyframe.
    func enqueue(_ sample: CMSampleBuffer) -> Bool {
        guard let picture = decoder.decode(sample) else { return false }
        pictures.withLock { $0.newest = picture }
        redraw()
        return true
    }

    /// Sizes the drawable to the layer's pixels and draws the last picture again.
    func resize(to size: CGSize, scale: CGFloat) {
        let pixels = CGSize(width: (size.width * scale).rounded(), height: (size.height * scale).rounded())
        guard pixels.width > 0, pixels.height > 0, pixels != layer.drawableSize else { return }
        layer.contentsScale = scale
        layer.drawableSize = pixels
        redraw()
    }

    /// Draws the newest picture on the drawing queue, unless a draw already waits there.
    private func redraw() {
        let waiting = pictures.withLock { pictures in
            defer { pictures.drawing = true }
            return pictures.drawing
        }
        guard !waiting else { return }
        drawing.async { [self] in
            guard let picture = pictures.withLock({ $0.take() }), let gpu = VideoGPU.shared else { return }
            gpu.draw(picture.buffer, on: layer)
        }
    }
}

private struct Pictures {
    var newest: Picture?
    var shown: Picture?
    var drawing = false

    /// The newest picture, or the one on screen again when nothing newer came.
    mutating func take() -> Picture? {
        drawing = false
        if let newest {
            shown = newest
            self.newest = nil
        }
        return shown
    }
}

/// A decoded 420v picture, which nothing writes to once VideoToolbox filled it.
private struct Picture: @unchecked Sendable {
    let buffer: CVPixelBuffer
}

/// One VideoToolbox session for the stream's current format, which the keyframe of a new codec
/// or size replaces.
private struct VideoDecoder: ~Copyable {
    private var session: VTDecompressionSession?
    private var format: CMFormatDescription?

    mutating func decode(_ sample: CMSampleBuffer) -> Picture? {
        guard let format = CMSampleBufferGetFormatDescription(sample) else { return nil }
        if !(self.format.map { CMFormatDescriptionEqual(format, otherFormatDescription: $0) } ?? false) {
            start(format)
        }
        guard let session else { return nil }
        let decoded = Decoded()
        let status = VTDecompressionSessionDecodeFrame(
            session, sampleBuffer: sample, flags: [], infoFlagsOut: nil,
        ) { status, _, image, _, _ in
            if status == noErr, let image {
                decoded.picture.withLock { $0 = Picture(buffer: image) }
            }
        }
        guard status == noErr else {
            // An iPhone app that went to the background loses its session.
            stop()
            return nil
        }
        VTDecompressionSessionWaitForAsynchronousFrames(session)
        return decoded.picture.withLock { $0 }
    }

    private mutating func start(_ format: CMFormatDescription) {
        stop()
        let pictures: [CFString: Any] = [
            kCVPixelBufferPixelFormatTypeKey: kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange,
            kCVPixelBufferMetalCompatibilityKey: true,
            kCVPixelBufferIOSurfacePropertiesKey: [CFString: Any](),
        ]
        var created: VTDecompressionSession?
        VTDecompressionSessionCreate(
            allocator: nil, formatDescription: format, decoderSpecification: nil,
            imageBufferAttributes: pictures as CFDictionary, outputCallback: nil, decompressionSessionOut: &created,
        )
        guard let created else { return }
        VTSessionSetProperty(created, key: kVTDecompressionPropertyKey_RealTime, value: kCFBooleanTrue)
        session = created
        self.format = format
    }

    private mutating func stop() {
        if let session {
            VTDecompressionSessionInvalidate(session)
        }
        session = nil
        format = nil
    }

    deinit {
        if let session {
            VTDecompressionSessionInvalidate(session)
        }
    }
}

private final class Decoded: Sendable {
    let picture = Mutex<Picture?>(nil)
}

/// The Metal device, queue and pipeline that every video view shares. Ghostty keeps its own.
private final class VideoGPU: @unchecked Sendable {
    static let device = MTLCreateSystemDefaultDevice()
    /// Compiles the shader on first use, on the drawing queue rather than the main thread.
    static let shared = device.flatMap(VideoGPU.init)

    private static let shader = """
    #include <metal_stdlib>
    using namespace metal;

    struct Corner {
        float4 position [[position]];
        float2 at;
    };

    vertex Corner place(uint corner [[vertex_id]]) {
        float2 at = float2(corner & 1, corner >> 1);
        return { float4(at.x * 2 - 1, 1 - at.y * 2, 0, 1), at };
    }

    // BT.709 limited range, as every Kodosi encoder writes it.
    fragment float4 colour(Corner in [[stage_in]],
                           texture2d<float> luma [[texture(0)]],
                           texture2d<float> chroma [[texture(1)]]) {
        constexpr sampler smooth(filter::linear);
        float y = (luma.sample(smooth, in.at).r - 16.0 / 255.0) * (255.0 / 219.0);
        float2 c = (chroma.sample(smooth, in.at).rg - 128.0 / 255.0) * (255.0 / 224.0);
        return float4(y + 1.5748 * c.y, y - 0.1873 * c.x - 0.4681 * c.y, y + 1.8556 * c.x, 1);
    }
    """

    private let queue: any MTLCommandQueue
    private let pipeline: any MTLRenderPipelineState
    private let textures: CVMetalTextureCache

    private init?(device: any MTLDevice) {
        guard let queue = device.makeCommandQueue(), let library = try? device.makeLibrary(source: Self.shader, options: nil)
        else { return nil }
        let description = MTLRenderPipelineDescriptor()
        description.vertexFunction = library.makeFunction(name: "place")
        description.fragmentFunction = library.makeFunction(name: "colour")
        description.colorAttachments[0].pixelFormat = .bgra8Unorm
        var textures: CVMetalTextureCache?
        CVMetalTextureCacheCreate(nil, nil, device, nil, &textures)
        guard let pipeline = try? device.makeRenderPipelineState(descriptor: description), let textures else {
            return nil
        }
        self.queue = queue
        self.pipeline = pipeline
        self.textures = textures
    }

    /// Aspect-fits the picture into the layer's drawable and presents it at once.
    func draw(_ picture: CVPixelBuffer, on layer: CAMetalLayer) {
        guard let luma = texture(picture, plane: 0, format: .r8Unorm),
              let chroma = texture(picture, plane: 1, format: .rg8Unorm),
              let drawable = layer.nextDrawable(), let commands = queue.makeCommandBuffer()
        else { return }
        let pass = MTLRenderPassDescriptor()
        pass.colorAttachments[0].texture = drawable.texture
        pass.colorAttachments[0].loadAction = .clear
        pass.colorAttachments[0].clearColor = MTLClearColor(red: 0, green: 0, blue: 0, alpha: 0)
        pass.colorAttachments[0].storeAction = .store
        guard let encoder = commands.makeRenderCommandEncoder(descriptor: pass) else { return }
        let target = CGSize(width: drawable.texture.width, height: drawable.texture.height)
        let size = CGSize(width: CVPixelBufferGetWidth(picture), height: CVPixelBufferGetHeight(picture))
        let fit = min(target.width / size.width, target.height / size.height)
        encoder.setViewport(MTLViewport(
            originX: (target.width - size.width * fit) / 2, originY: (target.height - size.height * fit) / 2,
            width: size.width * fit, height: size.height * fit, znear: 0, zfar: 1,
        ))
        encoder.setRenderPipelineState(pipeline)
        encoder.setFragmentTexture(CVMetalTextureGetTexture(luma.texture), index: 0)
        encoder.setFragmentTexture(CVMetalTextureGetTexture(chroma.texture), index: 1)
        encoder.drawPrimitives(type: .triangleStrip, vertexStart: 0, vertexCount: 4)
        encoder.endEncoding()
        commands.present(drawable)
        // The textures must outlive the GPU's reads of the picture.
        commands.addCompletedHandler { _ in withExtendedLifetime((luma, chroma)) {} }
        commands.commit()
    }

    private func texture(_ picture: CVPixelBuffer, plane: Int, format: MTLPixelFormat) -> PlaneTexture? {
        var texture: CVMetalTexture?
        CVMetalTextureCacheCreateTextureFromImage(
            nil, textures, picture, nil, format,
            CVPixelBufferGetWidthOfPlane(picture, plane), CVPixelBufferGetHeightOfPlane(picture, plane), plane, &texture,
        )
        return texture.map(PlaneTexture.init)
    }
}

/// A plane of a picture that Metal reads, held until the GPU is done with it.
private struct PlaneTexture: @unchecked Sendable {
    let texture: CVMetalTexture
}
