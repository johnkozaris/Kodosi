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

#include <algorithm>
#include <atomic>
#include <chrono>
#include <mutex>
#include <utility>

namespace kodosi {
namespace {
static_assert(static_cast<int>(VideoCodec::H264) == KODOSI_VIDEO_H264
    && static_cast<int>(VideoCodec::Hevc) == KODOSI_VIDEO_HEVC);
constexpr char messageTag = 0;
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

// What a stream showed since the view looked last.
struct Shown {
    Picture picture;
    // Whether the picture waits for a repair, which came after the newest picture.
    bool resting = false;
};

// Decodes a watched video pipe on the runtime's thread of the pipe, which hands over only the frames
// that the decoder can take. The GUI thread lets the decoder go only while the pipe is not watched.
class VideoStream final : public QObject, public VideoSink {
    Q_OBJECT

public:
    bool take(const Frame& frame) noexcept override
    {
        auto* decoder = decoderFor(frame.codec, frame.key);
        auto decoded = decoder != nullptr ? decoder->decode(frame.data)
                                          : std::expected<std::shared_ptr<VideoFrame>, int>(std::unexpect, 0);
        if (!decoded) {
            // A keyframe needs no earlier frame, so a GPU that cannot decode one cannot decode the stream.
            if (frame.key && decoder != nullptr && decoder->onGpu()) {
                if (const auto gpu = videoGpu(); gpu && !gpu->failed.exchange(true))
                    qCWarning(kodosiVideo) << "This GPU cannot decode the stream, so video decodes in software";
            }
            m_decoder.reset();
            return false;
        }
        if (*decoded)
            show({ std::move(*decoded), frame.region });
        return true;
    }

    // Bound the states that wait for a busy GUI thread. Past that, drop them until the view opens the
    // pipe again, which brings the whole state.
    void hand(QByteArrayView message) noexcept override
    {
        if (m_behind.load())
            return;
        if (const auto waiting = m_waitingStates.load(); waiting >= mostWaitingStates
            || (waiting > 0 && m_waitingStateBytes.load() > mostWaitingStateBytes - message.size())) {
            m_behind = true;
            m_stateGeneration.fetch_add(1);
            emit fellBehind();
            return;
        }
        m_waitingStates.fetch_add(1);
        m_waitingStateBytes.fetch_add(message.size());
        emit received(m_stateGeneration.load(), message.toByteArray());
    }

    void repair() noexcept override
    {
        bool waking = false;
        {
            std::lock_guard lock(m_showing);
            waking = !m_shown.picture.frame && !m_shown.resting;
            m_shown.resting = true;
        }
        if (waking)
            emit pictureReady();
    }

    // Coalesce decoded frames so a busy GUI thread always picks up only the latest picture.
    Shown takeShown()
    {
        std::lock_guard lock(m_showing);
        return std::exchange(m_shown, {});
    }

    // Nothing of the pipe that went is shown, and a new pipe starts a decoder of its own.
    void forget()
    {
        m_decoder.reset();
        m_stateGeneration.fetch_add(1);
        m_behind = false;
        std::lock_guard lock(m_showing);
        m_shown = {};
    }

    // The pipe opens again, with the whole state.
    void caughtUp() { m_behind = false; }

    bool tookState(quint64 generation, qsizetype bytes)
    {
        m_waitingStates.fetch_sub(1);
        m_waitingStateBytes.fetch_sub(bytes);
        return generation == m_stateGeneration.load();
    }

signals:
    void received(quint64 generation, const QByteArray& message);
    void pictureReady();
    void fellBehind();

private:
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

    void show(Picture picture)
    {
        bool waking = false;
        {
            std::lock_guard lock(m_showing);
            waking = !m_shown.picture.frame && !m_shown.resting;
            m_shown = { .picture = std::move(picture), .resting = false };
        }
        if (waking)
            emit pictureReady();
    }

    std::unique_ptr<VideoDecoder> m_decoder;
    std::mutex m_showing;
    Shown m_shown;
    std::atomic<bool> m_behind { false };
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
    , m_stream(std::make_shared<VideoStream>())
{
    setFlag(ItemHasContents);
    connect(m_stream.get(), &VideoStream::received, this, &StreamVideo::received, Qt::QueuedConnection);
    connect(m_stream.get(), &VideoStream::pictureReady, this, &StreamVideo::showPicture, Qt::QueuedConnection);
    connect(m_stream.get(), &VideoStream::fellBehind, this, &StreamVideo::fellBehind, Qt::QueuedConnection);
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
    connect(m_documents, &CanvasDocuments::opened, this, &StreamVideo::opened);
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
    m_pipe = m_documents->openVideo(m_sessionId, m_key, m_stream);
    if (!m_pipe.isEmpty())
        setProblem({});
}

void StreamVideo::leave()
{
    if (m_documents && !m_pipe.isEmpty())
        m_documents->close(m_pipe);
    m_stream->forget();
    m_pipe.clear();
    m_watching = false;
    m_live = false;
    restate();
}

void StreamVideo::pause()
{
    leave();
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

void StreamVideo::received(quint64 generation, const QByteArray& message)
{
    if (m_stream->tookState(generation, message.size()))
        take(QJsonDocument::fromJson(message).object());
}

// A repair that came after the newest picture rests it.
void StreamVideo::showPicture()
{
    auto [picture, resting] = m_stream->takeShown();
    if (picture.frame) {
        const bool moved = picture.region != m_region;
        m_frame = std::move(picture.frame);
        m_region = picture.region;
        m_live = true;
        update();
        if (moved)
            this->moved();
    }
    if (resting)
        m_live = false;
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
    m_stream->forget();
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

// The view dropped states of the stream, so the pipe opens again with the whole state.
void StreamVideo::fellBehind()
{
    if (m_documents && !m_pipe.isEmpty()) {
        m_watching = false;
        m_documents->renew(m_pipe);
    }
    m_stream->caughtUp();
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
    QByteArray bytes(1, messageTag);
    bytes += QJsonDocument(message).toJson(QJsonDocument::Compact);
    if (!sendPipeFrame(m_documents->commands(), m_pipe, QString::fromLatin1(bytes.toBase64()))) {
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
