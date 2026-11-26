#include "canvas/StreamVideo.hpp"
#include "canvas/VideoDecoder.hpp"
#include "canvas/VideoNode.hpp"
#include "presentation/CanvasDocuments.hpp"
#include "presentation/CanvasModel.hpp"

#include <kodosi_runtime.h>

#include <QClipboard>
#include <QCoreApplication>
#include <QGuiApplication>
#include <QHash>
#include <QJsonArray>
#include <QJsonDocument>
#include <QQuickWindow>
#include <QThreadPool>
#include <QTimer>
#include <QtEndian>

#include <algorithm>
#include <array>
#include <atomic>
#include <bit>
#include <chrono>
#include <mutex>
#include <utility>

namespace kodosi {
namespace {
constexpr char messageTag = 0;
constexpr char frameTag = 1;
// A frame is a tag byte, its number, its flags and its codec, then the region it shows when its flags
// say so, then its Annex B data.
constexpr qsizetype frameHeader = 7;
constexpr qsizetype regionBytes = 16;
constexpr char keyFlag = 1;
constexpr char anchorFlag = 2;
constexpr char regionFlag = 4;
// Finding the GPU opens its driver. The first stream waits this long for it, and one that opens
// before the search ends offers software decoding, so a driver that hangs costs one wait in all.
constexpr std::chrono::milliseconds gpuWait { 500 };
constexpr int settleWait = 250;
constexpr qsizetype mostTyped = 16 * 1024;
constexpr int mostWaitingStates = 64;
constexpr qsizetype mostWaitingStateBytes = 8 * 1024 * 1024;
// What the last pictures of feeds that no view shows may take together.
constexpr qsizetype mostKeptBytes = 64 * 1024 * 1024;

int viewerModifiers(int modifiers)
{
    return (modifiers & Qt::AltModifier ? 1 : 0) | (modifiers & Qt::ControlModifier ? 2 : 0)
        | (modifiers & Qt::MetaModifier ? 4 : 0) | (modifiers & Qt::ShiftModifier ? 8 : 0);
}

// A picture that the GPU decoded holds its decoder's surfaces, so a copy in memory takes its place
// where it stays. The copy takes a moment, which the window does not wait for: `done` gets the
// picture and its copy on the window's thread, while `owner` lives.
template <typename Done>
void copyIntoMemory(std::shared_ptr<const VideoFrame> frame, QObject& owner, Done done)
{
    QThreadPool::globalInstance()->start([owner = QPointer(&owner), frame = std::move(frame), done = std::move(done)] {
        auto copy = inMemory(frame);
        QMetaObject::invokeMethod(QCoreApplication::instance(), [owner, frame, copy = std::move(copy), done] {
            if (owner)
                done(frame, copy);
        });
    });
}

// Hovering, letting go and lifting a key follow what the viewer did before, so they do not act.
bool acts(const QJsonObject& event)
{
    const auto kind = event.value(QStringLiteral("kind")).toString();
    const auto action = event.value(QStringLiteral("action")).toString();
    if (kind == QStringLiteral("mouse"))
        return action != QStringLiteral("move") && action != QStringLiteral("release");
    return kind != QStringLiteral("key") || action != QStringLiteral("up");
}
}

// A decoded picture, and the part of the source that it shows when it shows only a part.
struct Picture {
    std::shared_ptr<const VideoFrame> frame;
    std::optional<QRectF> region;
};

// Reads and decodes a video pipe on a dedicated thread. Frames depend on prior frames, so every
// frame is decoded, and a new pipe or a broken frame waits for a keyframe.
class VideoStream final : public QObject {
    Q_OBJECT

public:
    void read(const QByteArray& payload, bool afterDrop, CommandDispatcher& commands)
    {
        const auto event = QJsonDocument::fromJson(payload).object();
        const auto pipe = event.value(QStringLiteral("pipe")).toString();
        if (pipe.isEmpty())
            return;
        if (pipe != m_pipe) {
            m_pipe = pipe;
            m_parts.close();
            reset();
        }
        if (afterDrop)
            m_parts.close();
        const auto type = event.value(QStringLiteral("type")).toString();
        if (type == QStringLiteral("document.opened")) {
            m_parts.open();
            m_holds = Holds::Nothing;
            m_broken = false;
            m_statesDropped = false;
            emit opened(pipe);
        } else if (type == QStringLiteral("document.lost")) {
            m_parts.lose();
            emit lost(pipe, event.value(QStringLiteral("message")).toString());
        } else if (type == QStringLiteral("document.closed")) {
            m_parts.close();
            emit closed(pipe);
        } else if (type == QStringLiteral("document.frame")) {
            const auto taken = m_parts.take(event);
            if (taken.broken)
                breakPipe();
            else if (taken.frame && taken.frame->startsWith(frameTag))
                frame(*taken.frame, commands);
            else if (taken.frame && taken.frame->startsWith(messageTag))
                hand(*taken.frame);
        }
    }

    // Thread-safe: the runtime expects all chunks of a pipe frame to be sent contiguously.
    bool post(CommandDispatcher& commands, const QString& pipe, const QJsonObject& message)
    {
        QByteArray bytes(1, messageTag);
        bytes += QJsonDocument(message).toJson(QJsonDocument::Compact);
        std::lock_guard lock(m_sending);
        return sendPipeFrame(commands, pipe, QString::fromLatin1(bytes.toBase64()));
    }

    // Coalesce decoded frames so a busy GUI thread always picks up only the latest picture.
    std::pair<QString, Picture> takePicture()
    {
        std::lock_guard lock(m_showing);
        return { std::exchange(m_picturePipe, {}), std::exchange(m_picture, {}) };
    }

    // Nobody watches until a new pipe comes, which starts a decoder of its own.
    void forgetDecoder()
    {
        m_decoder.reset();
        std::lock_guard lock(m_showing);
        m_picture = {};
    }

    bool tookState(quint64 generation, qsizetype bytes)
    {
        m_waitingStates.fetch_sub(1);
        m_waitingStateBytes.fetch_sub(bytes);
        return generation == m_stateGeneration.load();
    }

signals:
    void opened(const QString& pipe);
    void received(const QString& pipe, quint64 generation, const QByteArray& message);
    void pictureReady();
    void lost(const QString& pipe, const QString& reason);
    void closed(const QString& pipe);
    void broke(const QString& pipe);

private:
    // What the stream waits for: a keyframe at first, or a keyframe or an anchor after a gap.
    enum class Holds { Nothing, Stream, Gap };

    // The host skips frames for a viewer that falls behind and resumes it on an anchor, so a gap in the
    // numbers asks for a keyframe only when the next frame is neither a keyframe nor an anchor.
    void frame(const QByteArray& message, CommandDispatcher& commands)
    {
        if (message.size() <= frameHeader)
            return;
        const auto id = qFromBigEndian<quint32>(message.constData() + 1);
        const auto flags = message.at(5);
        const bool key = (flags & keyFlag) != 0;
        if (m_holds == Holds::Stream && id != m_lastFrame + 1)
            m_holds = Holds::Gap;
        m_lastFrame = id;
        if (!key && (m_holds == Holds::Nothing || (m_holds == Holds::Gap && (flags & anchorFlag) == 0))) {
            ack(commands, id);
            if (m_holds == Holds::Gap)
                restart(commands);
            return;
        }
        auto body = QByteArrayView(message).sliced(frameHeader);
        std::optional<QRectF> region;
        if ((flags & regionFlag) != 0) {
            if (body.size() <= regionBytes) {
                ack(commands, id);
                return;
            }
            // Four big-endian 32-bit floats: x, y, width and height.
            std::array<float, 4> values {};
            for (std::size_t index = 0; index < values.size(); ++index) {
                const auto bits = qFromBigEndian<quint32>(body.data() + static_cast<qsizetype>(index) * 4);
                values.at(index) = std::bit_cast<float>(bits);
            }
            region = QRectF(values.at(0), values.at(1), values.at(2), values.at(3));
            body = body.sliced(regionBytes);
        }
        auto* decoder = decoderFor(static_cast<quint8>(message.at(6)), key);
        auto decoded = decoder != nullptr ? decoder->decode(body)
                                          : std::expected<std::shared_ptr<VideoFrame>, int>(std::unexpect, 0);
        ack(commands, id);
        if (!decoded) {
            // A keyframe needs no earlier frame, so a GPU that cannot decode one cannot decode the stream.
            if (key && decoder != nullptr && decoder->onGpu()) {
                if (const auto gpu = videoGpu(); gpu && !gpu->failed.exchange(true))
                    qCWarning(kodosiVideo) << "This GPU cannot decode the stream, so video decodes in software";
            }
            m_decoder.reset();
            restart(commands);
            return;
        }
        m_holds = Holds::Stream;
        if (*decoded)
            show({ std::move(*decoded), region });
    }

    // A keyframe starts a decoder for its codec, on the GPU once the search found one that has not failed.
    VideoDecoder* decoderFor(quint8 wire, bool key)
    {
        if (wire > static_cast<quint8>(VideoCodec::Hevc))
            return nullptr;
        const auto codec = static_cast<VideoCodec>(wire);
        const auto gpu = videoGpu();
        const bool current
            = m_decoder && m_decoder->codec() == codec && (!m_decoder->onGpu() || (gpu && gpu->decodes(codec)));
        if (!current && key)
            m_decoder = VideoDecoder::open(codec, gpu);
        else if (!current)
            return nullptr;
        return m_decoder.get();
    }

    // Waits for a keyframe, which the host sends at most once a second.
    void restart(CommandDispatcher& commands)
    {
        m_holds = Holds::Nothing;
        askKeyframe(commands);
    }

    // The host rate-limits keyframes to one per second because each is broadcast to all viewers.
    void askKeyframe(CommandDispatcher& commands)
    {
        if (!post(commands, m_pipe, { { QStringLiteral("type"), QStringLiteral("keyframe") } }))
            breakPipe();
    }

    void ack(CommandDispatcher& commands, quint32 frame)
    {
        if (!post(commands, m_pipe,
                { { QStringLiteral("type"), QStringLiteral("ack") },
                    { QStringLiteral("frame"), static_cast<qint64>(frame) } }))
            breakPipe();
    }

    void show(Picture picture)
    {
        bool waking = false;
        {
            std::lock_guard lock(m_showing);
            waking = !m_picture.frame;
            m_picture = std::move(picture);
            m_picturePipe = m_pipe;
        }
        if (waking)
            emit pictureReady();
    }

    // Bound queued state messages for a busy GUI thread. Once exceeded, drop further messages and
    // reset the pipe so reopening delivers a fresh full state snapshot.
    void hand(const QByteArray& message)
    {
        if (m_statesDropped)
            return;
        if (const auto waiting = m_waitingStates.load(); waiting >= mostWaitingStates
            || (waiting > 0 && m_waitingStateBytes.load() > mostWaitingStateBytes - message.size())) {
            m_statesDropped = true;
            m_stateGeneration.fetch_add(1);
            breakPipe();
            return;
        }
        m_waitingStates.fetch_add(1);
        m_waitingStateBytes.fetch_add(message.size());
        emit received(m_pipe, m_stateGeneration.load(), message);
    }

    // Missing or unsent chunks corrupt the stream, so close and reopen the pipe to resync.
    void breakPipe()
    {
        m_parts.close();
        if (!std::exchange(m_broken, true))
            emit broke(m_pipe);
    }

    void reset() { m_decoder.reset(); }

    QString m_pipe;
    PipeParts m_parts;
    std::unique_ptr<VideoDecoder> m_decoder;
    quint32 m_lastFrame = 0;
    Holds m_holds = Holds::Nothing;
    bool m_broken = false;
    bool m_statesDropped = false;
    std::mutex m_sending;
    std::mutex m_showing;
    QString m_picturePipe;
    Picture m_picture;
    std::atomic<int> m_waitingStates { 0 };
    std::atomic<qsizetype> m_waitingStateBytes { 0 };
    std::atomic<quint64> m_stateGeneration { 0 };
};

// The last picture of each feed, in this process's memory, and what its view left, from when a view of
// the feed goes until the next one comes or the feed leaves the canvas.
class KeptFeeds final : public QObject {
    Q_OBJECT

public:
    struct Feed {
        std::shared_ptr<const VideoFrame> still;
        std::optional<QRectF> region;
        QVariantMap view;
    };

    static KeptFeeds& of(CanvasModel& canvas)
    {
        if (auto* kept = canvas.findChild<KeptFeeds*>(QString(), Qt::FindDirectChildrenOnly))
            return *kept;
        return *new KeptFeeds(canvas);
    }

    void keep(const QString& sessionId, const QString& key, Feed feed)
    {
        if (!m_canvas.holds(sessionId, key))
            return;
        const auto id = feedId(sessionId, key);
        auto still = feed.still;
        m_order.removeOne(id);
        m_order.append(id);
        m_feeds.insert(id, std::move(feed));
        if (still && still->onGpu()) {
            copyIntoMemory(std::move(still), *this, [this, id](const auto& picture, const auto& copy) {
                const auto found = m_feeds.find(id);
                if (found != m_feeds.end() && found->still == picture)
                    found->still = copy;
            });
        }
        fit();
    }

    std::optional<Feed> take(const QString& sessionId, const QString& key)
    {
        const auto id = feedId(sessionId, key);
        m_order.removeOne(id);
        const auto found = m_feeds.find(id);
        if (found == m_feeds.end())
            return std::nullopt;
        auto feed = std::move(*found);
        m_feeds.erase(found);
        return feed;
    }

private:
    explicit KeptFeeds(CanvasModel& canvas)
        : QObject(&canvas)
        , m_canvas(canvas)
    {
        connect(&canvas, &CanvasModel::changed, this, &KeptFeeds::prune);
    }

    static QString feedId(const QString& sessionId, const QString& key) { return sessionId + QLatin1Char('\n') + key; }

    void prune()
    {
        for (const auto& id : QStringList(m_order)) {
            if (!m_canvas.holds(id.section(QLatin1Char('\n'), 0, 0), id.section(QLatin1Char('\n'), 1))) {
                m_feeds.remove(id);
                m_order.removeOne(id);
            }
        }
    }

    // Keeps the pictures of the feeds left last, as many as fit together.
    void fit()
    {
        qsizetype bytes = 0;
        for (auto id = m_order.crbegin(); id != m_order.crend(); ++id) {
            const auto found = m_feeds.find(*id);
            if (found == m_feeds.end() || !found->still)
                continue;
            const auto size = found->still->size();
            bytes += qsizetype(size.width()) * size.height() * 3 / 2;
            if (bytes > mostKeptBytes)
                found->still.reset();
        }
    }

    CanvasModel& m_canvas;
    QHash<QString, Feed> m_feeds;
    // The feeds that views left, the one left last at the end.
    QStringList m_order;
};

StreamVideo::StreamVideo(const QString& key, QQuickItem* parent)
    : QQuickItem(parent)
    , m_key(key)
    , m_lastShowing(QStringLiteral("waiting"))
    , m_stream(new VideoStream)
{
    setFlag(ItemHasContents);
    m_stream->moveToThread(&m_decoding);
    connect(&m_decoding, &QThread::finished, m_stream, &QObject::deleteLater);
    connect(m_stream, &VideoStream::opened, this, &StreamVideo::opened);
    connect(m_stream, &VideoStream::received, this, &StreamVideo::received);
    connect(m_stream, &VideoStream::pictureReady, this, &StreamVideo::showPicture);
    connect(m_stream, &VideoStream::lost, this, &StreamVideo::lost);
    connect(m_stream, &VideoStream::closed, this, [this](const QString& pipe) { closed(pipe, {}); });
    connect(m_stream, &VideoStream::broke, this, &StreamVideo::broke);
    m_decoding.start();
    videoGpu();
    m_settle.setSingleShot(true);
    m_settle.setInterval(settleWait);
    connect(&m_settle, &QTimer::timeout, this, [this] {
        sendSize();
        settled();
    });
}

// Closes the pipe without a signal, since what listens may be going too.
StreamVideo::~StreamVideo()
{
    keep();
    if (m_documents && !m_pipe.isEmpty())
        m_documents->close(m_pipe);
    m_decoding.quit();
    m_decoding.wait();
}

QObject* StreamVideo::documents() const
{
    return m_documents;
}

void StreamVideo::setDocuments(QObject* documents)
{
    m_documents = qobject_cast<CanvasDocuments*>(documents);
    if (!m_documents)
        return;
    connect(m_documents, &CanvasDocuments::lost, this, &StreamVideo::lost);
    connect(m_documents, &CanvasDocuments::closed, this, &StreamVideo::closed);
    join();
}

QObject* StreamVideo::canvas() const
{
    return m_canvas;
}

void StreamVideo::setCanvas(QObject* canvas)
{
    m_canvas = qobject_cast<CanvasModel*>(canvas);
    if (!m_canvas)
        return;
    connect(m_canvas, &CanvasModel::videoChosen, this, &StreamVideo::chosen);
    takeUp();
}

void StreamVideo::setSessionId(const QString& sessionId)
{
    if (sessionId == m_sessionId)
        return;
    keep();
    leave();
    m_sessionId = sessionId;
    forgetTerminal();
    takeUp();
    join();
}

void StreamVideo::chosen(const QString& sessionId, const QString& section)
{
    if (sessionId == m_sessionId && section == m_key && m_watching)
        watch();
}

void StreamVideo::forgetTerminal()
{
    m_problem.clear();
    m_region.reset();
    m_live = false;
    m_tookUp = false;
    if (m_frame) {
        m_frame.reset();
        update();
    }
    forget();
    restate();
}

void StreamVideo::componentComplete()
{
    QQuickItem::componentComplete();
    takeUp();
    join();
}

void StreamVideo::keep()
{
    if (!std::exchange(m_tookUp, false) || !m_canvas)
        return;
    KeptFeeds::of(*m_canvas).keep(m_sessionId, m_key, { .still = m_frame, .region = m_region, .view = kept() });
}

// The view shows what the last view of its feed left, resting, until the stream is live.
void StreamVideo::takeUp()
{
    if (m_tookUp || !m_canvas || m_sessionId.isEmpty() || !isComponentComplete())
        return;
    m_tookUp = true;
    auto feed = KeptFeeds::of(*m_canvas).take(m_sessionId, m_key);
    if (!feed)
        return;
    resume(feed->view);
    if (feed->still && !m_frame) {
        m_frame = std::move(feed->still);
        m_region = feed->region;
        update();
    }
    restate();
}

void StreamVideo::setActive(bool active)
{
    if (active == m_active)
        return;
    m_active = active;
    if (active)
        join();
    else
        pause();
}

// The runtime keeps the pipe waiting while the computer is offline, and one that failed meanwhile
// opens again once the computer is back.
void StreamVideo::setOffline(bool offline)
{
    if (offline == m_offline)
        return;
    m_offline = offline;
    if (!offline && !m_problem.isEmpty())
        retry();
    restate();
}

void StreamVideo::join()
{
    if (!m_active || !watchable() || !m_documents || m_sessionId.isEmpty() || !m_pipe.isEmpty()
        || !isComponentComplete())
        return;
    auto* stream = m_stream;
    auto& commands = m_documents->commands();
    m_pipe = m_documents->openReader(m_sessionId, m_key, *m_stream,
        [stream, &commands](const QByteArray& event, bool afterDrop) { stream->read(event, afterDrop, commands); });
    if (!m_pipe.isEmpty())
        setProblem({});
}

void StreamVideo::leave()
{
    if (m_documents && !m_pipe.isEmpty())
        m_documents->close(m_pipe);
    m_pipe.clear();
    m_watching = false;
    m_live = false;
    restate();
}

void StreamVideo::pause()
{
    leave();
    QMetaObject::invokeMethod(m_stream, [stream = m_stream] { stream->forgetDecoder(); });
    if (m_frame && m_frame->onGpu()) {
        copyIntoMemory(m_frame, *this, [this](const auto& picture, const auto& copy) {
            if (m_frame == picture && copy) {
                m_frame = copy;
                update();
            }
        });
    }
}

void StreamVideo::stale()
{
    m_live = false;
    restate();
}

void StreamVideo::opened(const QString& pipe)
{
    if (pipe != m_pipe)
        return;
    m_holding = false;
    setProblem({});
    seated();
    watch();
}

void StreamVideo::watch()
{
    m_sentSize = sizeMessage();
    m_watching = true;
    static bool waited = false;
    const auto gpu = videoGpu(std::exchange(waited, true) ? std::chrono::milliseconds {} : gpuWait);
    const auto decoder = [&gpu](const QString& codec, VideoCodec decoded) {
        return QJsonObject { { QStringLiteral("codec"), codec },
            { QStringLiteral("hardware"), gpu && gpu->decodes(decoded) } };
    };
    post({ { QStringLiteral("type"), QStringLiteral("watch") },
        { QStringLiteral("size"), m_sentSize },
        { QStringLiteral("decoders"),
            QJsonArray { decoder(QStringLiteral("h264"), VideoCodec::H264),
                decoder(QStringLiteral("hevc"), VideoCodec::Hevc) } },
        { QStringLiteral("anchors"), true },
        { QStringLiteral("opened"), m_canvas && m_canvas->takeChoice(m_sessionId, m_key) } });
}

void StreamVideo::received(const QString& pipe, quint64 generation, const QByteArray& message)
{
    if (!m_stream->tookState(generation, message.size()) || pipe != m_pipe)
        return;
    take(QJsonDocument::fromJson(message.mid(1)).object());
}

void StreamVideo::showPicture()
{
    auto [pipe, picture] = m_stream->takePicture();
    if (pipe != m_pipe || !picture.frame)
        return;
    const bool moved = picture.region != m_region;
    m_frame = std::move(picture.frame);
    m_region = picture.region;
    m_live = true;
    update();
    if (moved)
        this->moved();
    restate();
}

// Without a reason, the runtime opens the pipe again by itself.
void StreamVideo::lost(const QString& pipe, const QString& reason)
{
    if (pipe != m_pipe)
        return;
    m_watching = false;
    m_live = false;
    setProblem(reason);
    restate();
}

void StreamVideo::closed(const QString& pipe, const QString& reason)
{
    if (pipe != m_pipe)
        return;
    if (m_documents)
        m_documents->forget(pipe);
    m_pipe.clear();
    m_watching = false;
    m_live = false;
    setProblem(reason);
    restate();
}

void StreamVideo::retry()
{
    leave();
    setProblem({});
    join();
}

void StreamVideo::broke(const QString& pipe)
{
    if (pipe != m_pipe || !m_documents)
        return;
    m_watching = false;
    m_documents->renew(pipe);
}

QString StreamVideo::showing() const
{
    if (const auto reason = away(); !reason.isEmpty())
        return reason;
    if (!m_problem.isEmpty())
        return QStringLiteral("failed");
    if (m_frame)
        return m_live ? QStringLiteral("live") : QStringLiteral("resting");
    return QStringLiteral("waiting");
}

QString StreamVideo::away() const
{
    return m_offline ? QStringLiteral("offline") : QString();
}

void StreamVideo::restate()
{
    if (auto now = showing(); now != m_lastShowing) {
        m_lastShowing = std::move(now);
        emit showingChanged();
    }
}

void StreamVideo::setProblem(const QString& problem)
{
    if (problem == m_problem)
        return;
    m_problem = problem;
    if (!problem.isEmpty())
        qCWarning(kodosiVideo).noquote() << "A canvas video stopped:" << problem;
    m_lastShowing = showing();
    emit showingChanged();
}

void StreamVideo::setRest(qreal rest)
{
    if (rest == m_rest)
        return;
    m_rest = rest;
    update();
    emit restChanged();
}

void StreamVideo::post(const QJsonObject& message)
{
    if (!m_documents || m_pipe.isEmpty())
        return;
    if (!m_stream->post(m_documents->commands(), m_pipe, message)) {
        m_watching = false;
        m_documents->renew(m_pipe);
    }
}

QJsonObject StreamVideo::sizeMessage() const
{
    const auto scale = QQuickItem::window() ? QQuickItem::window()->devicePixelRatio() : 1.0;
    return { { QStringLiteral("width"), qRound(width()) }, { QStringLiteral("height"), qRound(height()) },
        { QStringLiteral("scale"), scale } };
}

void StreamVideo::input(const QJsonObject& event)
{
    if (!takesInput())
        return;
    if (acts(event))
        acting();
    post({ { QStringLiteral("type"), QStringLiteral("input") }, { QStringLiteral("input"), event } });
    if (event.value(QStringLiteral("kind")).toString() == QStringLiteral("mouse")) {
        const auto action = event.value(QStringLiteral("action")).toString();
        if (action == QStringLiteral("move") && event.value(QStringLiteral("button")).toString() == QStringLiteral("none"))
            return;
        if (action == QStringLiteral("press"))
            m_holding = true;
        else if (action == QStringLiteral("release"))
            m_holding = false;
    }
    sendSize();
}

// The stream adopts the size of whoever interacted last. Send resizes only after the view settles
// and no button is held, and flush any pending resize right after input so the input targets the
// layout the user saw.
void StreamVideo::sendSize()
{
    const auto size = sizeMessage();
    if (m_holding || size == m_sentSize)
        return;
    m_sentSize = size;
    post({ { QStringLiteral("type"), QStringLiteral("size") }, { QStringLiteral("size"), size } });
}

void StreamVideo::step(const QVariantMap& input)
{
    this->input(QJsonObject::fromVariantMap(input));
}

// Clamp pointer coordinates to the stream's edge while a button is held so drags and releases
// outside the bounds still reach the stream.
void StreamVideo::pointer(const QString& action, qreal x, qreal y, int button, int clicks, int modifiers)
{
    const auto at = streamPoint(QPointF(x, y), m_holding);
    if (!at)
        return;
    const bool moving = action == QStringLiteral("move");
    const auto name = button == Qt::LeftButton ? QStringLiteral("left")
        : button == Qt::RightButton            ? QStringLiteral("right")
        : button == Qt::MiddleButton           ? QStringLiteral("middle")
                                               : QStringLiteral("none");
    input({ { QStringLiteral("kind"), QStringLiteral("mouse") }, { QStringLiteral("action"), action },
        { QStringLiteral("x"), at->x() }, { QStringLiteral("y"), at->y() }, { QStringLiteral("button"), name },
        { QStringLiteral("clicks"), moving ? 0 : std::clamp(clicks, 0, 3) },
        { QStringLiteral("modifiers"), viewerModifiers(modifiers) } });
}

void StreamVideo::wheel(qreal x, qreal y, QPoint pixels, QPoint angle, int modifiers)
{
    const auto at = streamPoint(QPointF(x, y), false);
    if (!at)
        return;
    const auto delta = pixels.isNull() ? angle * 100 / 120 : pixels;
    input({ { QStringLiteral("kind"), QStringLiteral("mouse") }, { QStringLiteral("action"), QStringLiteral("wheel") },
        { QStringLiteral("x"), at->x() }, { QStringLiteral("y"), at->y() }, { QStringLiteral("deltaX"), -delta.x() },
        { QStringLiteral("deltaY"), -delta.y() }, { QStringLiteral("modifiers"), viewerModifiers(modifiers) } });
}

bool StreamVideo::key(int key, const QString& text, int modifiers, bool down)
{
    const bool command = modifiers & (Qt::ControlModifier | Qt::AltModifier | Qt::MetaModifier);
    const auto printable = !text.isEmpty() && std::ranges::all_of(text, [](QChar character) { return character.isPrint(); });
    if (!CanvasModel::viewerKey(key)) {
        if (down && printable && !command)
            type(text);
        return down && printable && !command;
    }
    QString typed = printable ? text : QString {};
    if (key >= Qt::Key_A && key <= Qt::Key_Z && !printable)
        typed = modifiers & Qt::ShiftModifier ? QChar(u'A' + (key - Qt::Key_A)) : QChar(u'a' + (key - Qt::Key_A));
    else if (key >= Qt::Key_0 && key <= Qt::Key_9 && !printable)
        typed = QChar(u'0' + (key - Qt::Key_0));
    press(key, typed, down, modifiers);
    return true;
}

void StreamVideo::press(int key, const QString& typed, bool down, int modifiers)
{
    const auto found = CanvasModel::viewerKey(key);
    const bool command = modifiers & (Qt::ControlModifier | Qt::AltModifier | Qt::MetaModifier);
    QString text;
    if (down && found->code == QStringLiteral("Space"))
        text = QStringLiteral(" ");
    else if (down && found->named == QStringLiteral("Enter"))
        text = QStringLiteral("\r");
    else if (down && found->named.isEmpty() && !command)
        text = typed;
    QJsonObject event { { QStringLiteral("kind"), QStringLiteral("key") },
        { QStringLiteral("action"), down ? QStringLiteral("down") : QStringLiteral("up") },
        { QStringLiteral("key"), found->named.isEmpty() ? typed : found->named }, { QStringLiteral("code"), found->code },
        { QStringLiteral("keyCode"), found->windows }, { QStringLiteral("modifiers"), viewerModifiers(modifiers) } };
    if (!text.isEmpty())
        event.insert(QStringLiteral("text"), text);
    input(event);
}

void StreamVideo::type(const QString& text)
{
    if (!text.isEmpty())
        input({ { QStringLiteral("kind"), QStringLiteral("text") }, { QStringLiteral("text"), text.left(mostTyped) } });
}

void StreamVideo::paste()
{
    type(QGuiApplication::clipboard()->text());
}

QSize StreamVideo::pictureSize() const
{
    return m_frame ? m_frame->size() : QSize();
}

QRectF StreamVideo::pictureRect() const
{
    const auto picture = QSizeF(pictureSize()).scaled(size(), Qt::KeepAspectRatio);
    return QRectF(QPointF((width() - picture.width()) / 2, (height() - picture.height()) / 2), picture);
}

QSGNode* StreamVideo::updatePaintNode(QSGNode* node, UpdatePaintNodeData*)
{
    auto* video = static_cast<VideoNode*>(node);
    if (!m_frame) {
        delete video;
        return nullptr;
    }
    if (video == nullptr)
        video = new VideoNode;
    video->show(m_frame, pictureRect(), static_cast<float>(m_rest));
    return video;
}

void StreamVideo::geometryChange(const QRectF& next, const QRectF& previous)
{
    QQuickItem::geometryChange(next, previous);
    if (next.size() != previous.size())
        m_settle.start();
    update();
}

void StreamVideo::itemChange(ItemChange change, const ItemChangeData& value)
{
    QQuickItem::itemChange(change, value);
    if (change == ItemDevicePixelRatioHasChanged) {
        m_settle.start();
        update();
    }
}
}

#include "StreamVideo.moc"
