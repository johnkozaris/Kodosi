import CoreMedia
import CoreVideo
import KodosiBridge
import Metal
import QuartzCore
import Synchronization
import VideoToolbox

/// Decodes a feed's samples with VideoToolbox and draws each picture with Metal as soon as it
/// arrives, a frame sooner than `AVSampleBufferDisplayLayer` shows it. Samples come on the
/// runtime's thread of the video pipe and layout on the main thread, so drawing runs on its own
/// queue and always takes the newest picture.
final class VideoPlayer: VideoDisplay, @unchecked Sendable {
    /// CAMetalLayer hands out drawables on any thread; the views change its other properties on the
    /// main thread only.
    let layer: CAMetalLayer
    /// The feed decodes and stops it one call at a time.
    private var decoder = VideoDecoder()
    private let pictures = Mutex(Pictures())
    private let drawing = DispatchQueue(label: "com.kodosi.video", qos: .userInteractive)

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
    func enqueue(_ sample: VideoSample) -> Bool {
        guard let decoded = decoder.decode(sample.buffer) else { return false }
        pictures.withLock { $0.newest = Picture(buffer: decoded.buffer, region: sample.region) }
        redraw()
        return true
    }

    /// Keeps the picture on screen, which a resize draws again.
    func stop() {
        decoder.stop()
        pictures.withLock { $0.newest = nil }
    }

    /// Shows a feed's still picture until the feed's own frames come.
    func seed(_ still: VideoStill?) {
        guard let still else { return }
        let seeded = pictures.withLock { pictures in
            guard pictures.newest == nil, pictures.shown == nil else { return false }
            pictures.shown = Picture(buffer: still.picture, region: still.region)
            return true
        }
        if seeded {
            redraw()
        }
    }

    /// The picture on screen, copied out of its decoder's pool so that keeping it keeps nothing
    /// else of the decoder.
    func still() -> VideoStill? {
        guard let picture = pictures.withLock({ $0.newest ?? $0.shown }), let copy = Self.copy(picture.buffer) else { return nil }
        return VideoStill(picture: copy, region: picture.region)
    }

    /// Dims the picture while the video rests.
    func rest(_ resting: Bool) {
        let changed = pictures.withLock { pictures in
            defer { pictures.resting = resting }
            return pictures.resting != resting
        }
        if changed {
            redraw()
        }
    }

    /// Places each picture where its part of the source shows; `nil` fits each picture whole into
    /// the layer.
    func place(_ placement: VideoPlacement?) {
        let moved = pictures.withLock { pictures in
            defer { pictures.placement = placement }
            return pictures.placement != placement
        }
        if moved {
            redraw()
        }
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
            let (picture, placement, resting) = pictures.withLock { ($0.take(), $0.placement, $0.resting) }
            guard let picture, let gpu = VideoGPU.shared else { return }
            gpu.draw(picture, placement, resting: resting, on: layer)
        }
    }

    private static func copy(_ picture: CVPixelBuffer) -> CVPixelBuffer? {
        let attributes: [CFString: Any] = [
            kCVPixelBufferMetalCompatibilityKey: true,
            kCVPixelBufferIOSurfacePropertiesKey: [CFString: Any](),
        ]
        var created: CVPixelBuffer?
        guard CVPixelBufferCreate(nil, CVPixelBufferGetWidth(picture), CVPixelBufferGetHeight(picture),
                                  CVPixelBufferGetPixelFormatType(picture), attributes as CFDictionary, &created) == kCVReturnSuccess,
            let copy = created
        else { return nil }
        CVPixelBufferLockBaseAddress(picture, .readOnly)
        CVPixelBufferLockBaseAddress(copy, [])
        defer {
            CVPixelBufferUnlockBaseAddress(copy, [])
            CVPixelBufferUnlockBaseAddress(picture, .readOnly)
        }
        for plane in 0 ..< CVPixelBufferGetPlaneCount(picture) {
            guard let from = CVPixelBufferGetBaseAddressOfPlane(picture, plane), let to = CVPixelBufferGetBaseAddressOfPlane(copy, plane)
            else { return nil }
            let fromRow = CVPixelBufferGetBytesPerRowOfPlane(picture, plane)
            let toRow = CVPixelBufferGetBytesPerRowOfPlane(copy, plane)
            for row in 0 ..< CVPixelBufferGetHeightOfPlane(picture, plane) {
                memcpy(to + row * toRow, from + row * fromRow, min(fromRow, toRow))
            }
        }
        return copy
    }
}

private struct Pictures {
    var newest: Picture?
    var shown: Picture?
    var placement: VideoPlacement?
    var resting = false
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

/// A decoded 420v picture, which nothing writes to once VideoToolbox filled it, and the part of
/// the source that it shows when only a part.
private struct Picture: @unchecked Sendable {
    let buffer: CVPixelBuffer
    let region: CGRect?
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
                decoded.picture.withLock { $0 = Picture(buffer: image, region: nil) }
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

    mutating func stop() {
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

    // `rect` is the picture's left, top, right and bottom in clip space, which may reach past the
    // drawable while a view is zoomed.
    vertex Corner place(uint corner [[vertex_id]], constant float4 &rect [[buffer(0)]]) {
        float2 at = float2(corner & 1, corner >> 1);
        return { float4(mix(rect.x, rect.z, at.x), mix(rect.y, rect.w, at.y), 0, 1), at };
    }

    // BT.709 limited range, as every Kodosi encoder writes it. `look` is the brightness and the
    // saturation, which dim a picture that rests.
    fragment float4 colour(Corner in [[stage_in]],
                           texture2d<float> luma [[texture(0)]],
                           texture2d<float> chroma [[texture(1)]],
                           constant float2 &look [[buffer(0)]]) {
        constexpr sampler smooth(filter::linear);
        float y = (luma.sample(smooth, in.at).r - 16.0 / 255.0) * (255.0 / 219.0);
        float2 c = (chroma.sample(smooth, in.at).rg - 128.0 / 255.0) * (255.0 / 224.0);
        float3 rgb = float3(y + 1.5748 * c.y, y - 0.1873 * c.x - 0.4681 * c.y, y + 1.8556 * c.x);
        float grey = dot(rgb, float3(0.2126, 0.7152, 0.0722));
        return float4(mix(float3(grey), rgb, look.y) * look.x, 1);
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

    private static let resting = SIMD2<Float>(0.5, 0.55)

    /// Draws the picture where `placement` puts its part of the source, or aspect-fits it into the
    /// layer's drawable without one, and presents it at once.
    func draw(_ picture: Picture, _ placement: VideoPlacement?, resting: Bool, on layer: CAMetalLayer) {
        guard let luma = texture(picture.buffer, plane: 0, format: .r8Unorm),
              let chroma = texture(picture.buffer, plane: 1, format: .rg8Unorm),
              let drawable = layer.nextDrawable(), let commands = queue.makeCommandBuffer()
        else { return }
        let pass = MTLRenderPassDescriptor()
        pass.colorAttachments[0].texture = drawable.texture
        pass.colorAttachments[0].loadAction = .clear
        pass.colorAttachments[0].clearColor = MTLClearColor(red: 0, green: 0, blue: 0, alpha: 0)
        pass.colorAttachments[0].storeAction = .store
        guard let encoder = commands.makeRenderCommandEncoder(descriptor: pass) else { return }
        let target = CGSize(width: drawable.texture.width, height: drawable.texture.height)
        let rect = Self.rect(of: picture, placement, in: target)
        var corners = SIMD4<Float>(Float(rect.minX / target.width * 2 - 1), Float(1 - rect.minY / target.height * 2),
                                   Float(rect.maxX / target.width * 2 - 1), Float(1 - rect.maxY / target.height * 2))
        encoder.setVertexBytes(&corners, length: MemoryLayout<SIMD4<Float>>.stride, index: 0)
        encoder.setRenderPipelineState(pipeline)
        encoder.setFragmentTexture(CVMetalTextureGetTexture(luma.texture), index: 0)
        encoder.setFragmentTexture(CVMetalTextureGetTexture(chroma.texture), index: 1)
        var look = resting ? Self.resting : SIMD2<Float>(1, 1)
        encoder.setFragmentBytes(&look, length: MemoryLayout<SIMD2<Float>>.stride, index: 0)
        encoder.drawPrimitives(type: .triangleStrip, vertexStart: 0, vertexCount: 4)
        encoder.endEncoding()
        commands.present(drawable)
        // The textures must outlive the GPU's reads of the picture.
        commands.addCompletedHandler { _ in withExtendedLifetime((luma, chroma)) {} }
        commands.commit()
    }

    private static func rect(of picture: Picture, _ placement: VideoPlacement?, in target: CGSize) -> CGRect {
        if let placement {
            return placement.viewRect(picture.region ?? CGRect(origin: .zero, size: placement.source), in: target)
        }
        let size = CGSize(width: CVPixelBufferGetWidth(picture.buffer), height: CVPixelBufferGetHeight(picture.buffer))
        let fit = min(target.width / size.width, target.height / size.height)
        return CGRect(x: (target.width - size.width * fit) / 2, y: (target.height - size.height * fit) / 2,
                      width: size.width * fit, height: size.height * fit)
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
