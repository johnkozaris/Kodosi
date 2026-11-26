#include "canvas/VideoDecoder.hpp"
#include "canvas/VideoNode.hpp"

#include <rhi/qrhi.h>

#include <d3d11_4.h>
#include <dxgi1_2.h>
#include <wrl/client.h>

extern "C" {
#include <libavutil/frame.h>
#include <libavutil/hwcontext.h>
#include <libavutil/hwcontext_d3d11va.h>
#include <libavutil/pixfmt.h>
}

#include <array>
#include <cstdint>

using Microsoft::WRL::ComPtr;

namespace kodosi {
namespace {
// Long enough for the GPU to finish a copy, short enough that a lost device cannot freeze the window.
constexpr DWORD handoverWait = 100;
constexpr UINT64 decoderKey = 0;
constexpr UINT64 qtKey = 1;

bool decodes(ID3D11VideoDevice* video, const GUID& profile)
{
    BOOL supported = FALSE;
    return SUCCEEDED(video->CheckVideoDecoderFormat(&profile, DXGI_FORMAT_NV12, &supported)) && supported == TRUE;
}

bool sharesNv12(ID3D11Device* device)
{
    D3D11_FEATURE_DATA_D3D11_OPTIONS4 options {};
    return SUCCEEDED(device->CheckFeatureSupport(D3D11_FEATURE_D3D11_OPTIONS4, &options, sizeof(options)))
        && options.ExtendedNV12SharedTextureSupported == TRUE;
}

// NV12 copies with whole chroma samples only.
UINT even(int length)
{
    return static_cast<UINT>(length + 1) & ~1U;
}

// FFmpeg decodes on its own device, which Qt's device can't sample. Each picture goes to Qt's device in two
// copies on the GPU, through a texture that both devices open and hand over with a keyed mutex.
class D3D11Planes final : public VideoPlanes {
public:
    D3D11Planes(ID3D11Device* device, ID3D11DeviceContext* context)
        : m_device(device)
        , m_context(context)
    {
    }

    bool take(QRhi& rhi, QRhiResourceUpdateBatch&, std::shared_ptr<const VideoFrame> frame) override
    {
        const auto* av = frame->av();
        const auto* frames = reinterpret_cast<const AVHWFramesContext*>(av->hw_frames_ctx->data);
        const auto* decoder = static_cast<const AVD3D11VADeviceContext*>(frames->device_ctx->hwctx);
        const auto size = frame->size();
        if (!fit(rhi, decoder->device, size))
            return false;
        const D3D11_BOX picture { 0, 0, 0, even(size.width()), even(size.height()), 1 };
        decoder->lock(decoder->lock_ctx);
        const bool handed = m_decoderSide->AcquireSync(decoderKey, handoverWait) == S_OK;
        if (handed) {
            decoder->device_context->CopySubresourceRegion(m_shared.Get(), 0, 0, 0, 0,
                reinterpret_cast<ID3D11Texture2D*>(av->data[0]),
                static_cast<UINT>(reinterpret_cast<std::intptr_t>(av->data[1])), &picture);
            m_decoderSide->ReleaseSync(qtKey);
        }
        decoder->unlock(decoder->lock_ctx);
        if (!handed || m_qtSide->AcquireSync(qtKey, handoverWait) != S_OK)
            return false;
        m_context->CopyResource(m_local.Get(), m_opened.Get());
        m_qtSide->ReleaseSync(decoderKey);
        return true;
    }

    [[nodiscard]] QRhiTexture* luma() const override { return m_planes[0].get(); }
    [[nodiscard]] QRhiTexture* chroma() const override { return m_planes[1].get(); }

private:
    bool fit(QRhi& rhi, ID3D11Device* decoderDevice, QSize size)
    {
        if (m_local && m_size == size && m_decoderDevice == decoderDevice)
            return true;
        m_planes = {};
        m_local.Reset();
        m_shared.Reset();
        m_opened.Reset();
        m_size = size;
        m_decoderDevice = decoderDevice;
        D3D11_TEXTURE2D_DESC description {};
        description.Width = even(size.width());
        description.Height = even(size.height());
        description.MipLevels = 1;
        description.ArraySize = 1;
        description.Format = DXGI_FORMAT_NV12;
        description.SampleDesc.Count = 1;
        description.Usage = D3D11_USAGE_DEFAULT;
        description.BindFlags = D3D11_BIND_SHADER_RESOURCE;
        description.MiscFlags = D3D11_RESOURCE_MISC_SHARED_KEYEDMUTEX | D3D11_RESOURCE_MISC_SHARED_NTHANDLE;
        ComPtr<IDXGIResource1> resource;
        ComPtr<ID3D11Device1> device;
        HANDLE handle = nullptr;
        if (FAILED(decoderDevice->CreateTexture2D(&description, nullptr, &m_shared)) || FAILED(m_shared.As(&m_decoderSide))
            || FAILED(m_shared.As(&resource))
            || FAILED(resource->CreateSharedHandle(
                nullptr, DXGI_SHARED_RESOURCE_READ | DXGI_SHARED_RESOURCE_WRITE, nullptr, &handle))) {
            m_shared.Reset();
            return false;
        }
        const bool opened = SUCCEEDED(m_device.As(&device))
            && SUCCEEDED(device->OpenSharedResource1(handle, IID_PPV_ARGS(&m_opened)));
        CloseHandle(handle);
        description.MiscFlags = 0;
        if (!opened || FAILED(m_opened.As(&m_qtSide)) || FAILED(m_device->CreateTexture2D(&description, nullptr, &m_local))) {
            m_local.Reset();
            return false;
        }
        const std::array<QSize, 2> sizes { size, QSize((size.width() + 1) / 2, (size.height() + 1) / 2) };
        for (size_t plane = 0; plane < m_planes.size(); ++plane) {
            // A view of an NV12 texture as R8 sees its luma, and as RG8 its chroma.
            m_planes.at(plane).reset(rhi.newTexture(plane == 0 ? QRhiTexture::R8 : QRhiTexture::RG8, sizes.at(plane)));
            if (!m_planes.at(plane)->createFrom({ reinterpret_cast<quint64>(m_local.Get()), 0 })) {
                m_planes = {};
                m_local.Reset();
                return false;
            }
        }
        return true;
    }

    ComPtr<ID3D11Device> m_device;
    ComPtr<ID3D11DeviceContext> m_context;
    ID3D11Device* m_decoderDevice = nullptr;
    QSize m_size;
    ComPtr<ID3D11Texture2D> m_shared;
    ComPtr<IDXGIKeyedMutex> m_decoderSide;
    ComPtr<ID3D11Texture2D> m_opened;
    ComPtr<IDXGIKeyedMutex> m_qtSide;
    ComPtr<ID3D11Texture2D> m_local;
    std::array<std::unique_ptr<QRhiTexture>, 2> m_planes;
};
}

std::shared_ptr<VideoGpu> findVideoGpu()
{
    AVBufferRef* device = nullptr;
    if (av_hwdevice_ctx_create(&device, AV_HWDEVICE_TYPE_D3D11VA, nullptr, nullptr, 0) < 0)
        return nullptr;
    auto gpu = std::make_shared<VideoGpu>();
    gpu->device = device;
    const auto* context = reinterpret_cast<const AVHWDeviceContext*>(device->data);
    const auto* d3d11 = static_cast<const AVD3D11VADeviceContext*>(context->hwctx);
    if (!sharesNv12(d3d11->device))
        return nullptr;
    gpu->h264 = decodes(d3d11->video_device, D3D11_DECODER_PROFILE_H264_VLD_NOFGT);
    gpu->hevc = decodes(d3d11->video_device, D3D11_DECODER_PROFILE_HEVC_VLD_MAIN);
    return gpu->h264 || gpu->hevc ? gpu : nullptr;
}

int gpuPixelFormat()
{
    return AV_PIX_FMT_D3D11;
}

std::unique_ptr<VideoPlanes> gpuPlanes(QRhi& rhi)
{
    if (rhi.backend() != QRhi::D3D11)
        return nullptr;
    const auto* handles = static_cast<const QRhiD3D11NativeHandles*>(rhi.nativeHandles());
    return std::make_unique<D3D11Planes>(
        static_cast<ID3D11Device*>(handles->dev), static_cast<ID3D11DeviceContext*>(handles->context));
}
}
