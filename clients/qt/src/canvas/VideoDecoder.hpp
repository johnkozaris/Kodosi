#pragma once

#include <QByteArrayView>
#include <QSize>
#include <QtGlobal>

#include <atomic>
#include <expected>
#include <future>
#include <memory>

extern "C" {
struct AVBufferRef;
struct AVCodecContext;
struct AVFrame;
struct AVPacket;
}

namespace kodosi {

// The codec byte in a frame's header.
enum class VideoCodec : quint8 { H264 = 0, Hevc = 1 };

// A decoded picture: NV12 in the GPU's memory when the GPU decoded it, else planar YUV 4:2:0 here.
class VideoFrame {
public:
    explicit VideoFrame(AVFrame* frame);
    ~VideoFrame();
    VideoFrame(const VideoFrame&) = delete;
    VideoFrame& operator=(const VideoFrame&) = delete;

    [[nodiscard]] const AVFrame* av() const { return m_frame; }
    [[nodiscard]] QSize size() const;
    [[nodiscard]] bool onGpu() const;

private:
    AVFrame* m_frame;
};

// The GPU that Qt Quick draws with, when it also decodes video and Qt Quick can sample its pictures.
struct VideoGpu {
    VideoGpu() = default;
    ~VideoGpu();
    VideoGpu(const VideoGpu&) = delete;
    VideoGpu& operator=(const VideoGpu&) = delete;

    [[nodiscard]] bool decodes(VideoCodec codec) const
    {
        return !failed.load() && (codec == VideoCodec::Hevc ? hevc : h264);
    }

    AVBufferRef* device = nullptr;
    bool h264 = false;
    bool hevc = false;
    // Set once Qt Quick could not show one of its pictures, so every stream decodes in software.
    std::atomic<bool> failed { false };
};

// Looks for the GPU once, off the GUI thread, since opening a driver takes tens of milliseconds.
const std::shared_future<std::shared_ptr<VideoGpu>>& videoGpu();

// Decodes one stream's access units, on `gpu` when it decodes the codec and in software otherwise.
class VideoDecoder {
public:
    static std::unique_ptr<VideoDecoder> open(VideoCodec codec, const std::shared_ptr<VideoGpu>& gpu);
    ~VideoDecoder();
    VideoDecoder(const VideoDecoder&) = delete;
    VideoDecoder& operator=(const VideoDecoder&) = delete;

    [[nodiscard]] VideoCodec codec() const { return m_codec; }
    [[nodiscard]] bool onGpu() const { return m_onGpu; }
    // The picture that `unit` completes, if any, or FFmpeg's error when the unit broke the stream.
    std::expected<std::shared_ptr<VideoFrame>, int> decode(QByteArrayView unit);

private:
    VideoDecoder(AVCodecContext* context, VideoCodec codec, bool onGpu);

    AVCodecContext* m_context;
    AVPacket* m_packet;
    VideoCodec m_codec;
    bool m_onGpu;
};

// Found by the code for this platform's GPU.
std::shared_ptr<VideoGpu> findVideoGpu();
// The pixel format of the pictures that this platform's GPU decodes.
int gpuPixelFormat();
}
