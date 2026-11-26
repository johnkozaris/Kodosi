#include "canvas/VideoDecoder.hpp"

extern "C" {
#include <libavcodec/avcodec.h>
#include <libavutil/buffer.h>
#include <libavutil/frame.h>
}

#include <climits>
#include <cstring>

namespace kodosi {
namespace {
// Frames that this process holds beyond the decoder's own references: the newest waiting to be shown,
// the one on screen and the one before it, which the GPU may still read.
constexpr int heldFrames = 3;

AVPixelFormat chooseFormat(AVCodecContext* context, const AVPixelFormat* offered)
{
    for (const auto* format = offered; *format != AV_PIX_FMT_NONE; ++format) {
        if (*format == gpuPixelFormat())
            return *format;
    }
    return avcodec_default_get_format(context, offered);
}
}

VideoFrame::VideoFrame(AVFrame* frame)
    : m_frame(frame)
{
}

VideoFrame::~VideoFrame()
{
    av_frame_free(&m_frame);
}

QSize VideoFrame::size() const
{
    return { m_frame->width, m_frame->height };
}

bool VideoFrame::onGpu() const
{
    return m_frame->hw_frames_ctx != nullptr;
}

VideoGpu::~VideoGpu()
{
    av_buffer_unref(&device);
}

const std::shared_future<std::shared_ptr<VideoGpu>>& videoGpu()
{
    static const auto found = std::async(std::launch::async, findVideoGpu).share();
    return found;
}

std::unique_ptr<VideoDecoder> VideoDecoder::open(VideoCodec codec, const std::shared_ptr<VideoGpu>& gpu)
{
    const auto* decoder = avcodec_find_decoder(codec == VideoCodec::Hevc ? AV_CODEC_ID_HEVC : AV_CODEC_ID_H264);
    if (decoder == nullptr)
        return nullptr;
    auto* context = avcodec_alloc_context3(decoder);
    if (context == nullptr)
        return nullptr;
    // Kodosi's encoders never reorder frames, so each one is shown as soon as it decodes. Threads that
    // decode whole frames at once would each hold one back.
    context->flags |= AV_CODEC_FLAG_LOW_DELAY;
    context->thread_type = FF_THREAD_SLICE;
    const bool onGpu = gpu && gpu->decodes(codec);
    if (onGpu) {
        context->hw_device_ctx = av_buffer_ref(gpu->device);
        context->get_format = chooseFormat;
        context->extra_hw_frames = heldFrames;
    }
    if (avcodec_open2(context, decoder, nullptr) < 0) {
        avcodec_free_context(&context);
        return nullptr;
    }
    return std::unique_ptr<VideoDecoder>(new VideoDecoder(context, codec, onGpu));
}

VideoDecoder::VideoDecoder(AVCodecContext* context, VideoCodec codec, bool onGpu)
    : m_context(context)
    , m_packet(av_packet_alloc())
    , m_codec(codec)
    , m_onGpu(onGpu)
{
}

VideoDecoder::~VideoDecoder()
{
    av_packet_free(&m_packet);
    avcodec_free_context(&m_context);
}

std::expected<std::shared_ptr<VideoFrame>, int> VideoDecoder::decode(QByteArrayView unit)
{
    if (m_packet == nullptr || unit.size() > INT_MAX)
        return std::unexpected(AVERROR(ENOMEM));
    if (const int made = av_new_packet(m_packet, static_cast<int>(unit.size())); made < 0)
        return std::unexpected(made);
    std::memcpy(m_packet->data, unit.data(), static_cast<size_t>(unit.size()));
    const int sent = avcodec_send_packet(m_context, m_packet);
    av_packet_unref(m_packet);
    if (sent < 0)
        return std::unexpected(sent);
    std::shared_ptr<VideoFrame> newest;
    for (;;) {
        auto* frame = av_frame_alloc();
        if (frame == nullptr)
            return std::unexpected(AVERROR(ENOMEM));
        const int received = avcodec_receive_frame(m_context, frame);
        if (received == AVERROR(EAGAIN) || received == AVERROR_EOF) {
            av_frame_free(&frame);
            return newest;
        }
        if (received < 0 || (frame->flags & AV_FRAME_FLAG_CORRUPT) != 0) {
            av_frame_free(&frame);
            return std::unexpected(received < 0 ? received : AVERROR_INVALIDDATA);
        }
        newest = std::make_shared<VideoFrame>(frame);
    }
}
}
